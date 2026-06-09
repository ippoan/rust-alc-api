use axum::{
    extract::{Multipart, Path, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Extension, Json, Router,
};
use uuid::Uuid;

use alc_core::auth_middleware::TenantId;
use alc_core::tenant::set_current_tenant;
use alc_core::AppState;

use crate::ingest::sanitize_filename;

pub fn tenant_router() -> Router<AppState> {
    Router::new()
        .route("/notify/documents", axum::routing::get(list))
        .route("/notify/documents/search", axum::routing::get(search))
        .route("/notify/documents/upload", axum::routing::post(upload))
        .route("/notify/documents/{id}", axum::routing::get(get))
        .route(
            "/notify/documents/{id}/preview",
            axum::routing::get(preview),
        )
        .route(
            "/notify/documents/{id}/redact-recompute",
            axum::routing::post(redact_recompute),
        )
        .route(
            "/notify/documents/{id}/extract-recompute",
            axum::routing::post(extract_recompute),
        )
}

#[derive(serde::Deserialize)]
struct ListQuery {
    limit: Option<i64>,
    offset: Option<i64>,
}

async fn list(
    State(state): State<AppState>,
    Extension(tenant): Extension<TenantId>,
    Query(q): Query<ListQuery>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let docs = state
        .notify_documents
        .list(tenant.0, q.limit.unwrap_or(50), q.offset.unwrap_or(0))
        .await
        .map_err(|e| {
            tracing::error!("list notify_documents: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(Json(serde_json::to_value(docs).unwrap()))
}

#[derive(serde::Deserialize)]
struct SearchQuery {
    q: String,
}

async fn search(
    State(state): State<AppState>,
    Extension(tenant): Extension<TenantId>,
    Query(sq): Query<SearchQuery>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let docs = state
        .notify_documents
        .search(tenant.0, &sq.q)
        .await
        .map_err(|e| {
            tracing::error!("search notify_documents: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(Json(serde_json::to_value(docs).unwrap()))
}

async fn get(
    State(state): State<AppState>,
    Extension(tenant): Extension<TenantId>,
    Path(id): Path<uuid::Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let doc = state
        .notify_documents
        .get(tenant.0, id)
        .await
        .map_err(|e| {
            tracing::error!("get notify_document: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    let deliveries = state
        .notify_deliveries
        .list_by_document(tenant.0, id)
        .await
        .map_err(|e| {
            tracing::error!("list deliveries for document: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(serde_json::json!({
        "document": doc,
        "deliveries": deliveries,
    })))
}

const MAX_UPLOAD_FILES: usize = 20;
const MAX_UPLOAD_TOTAL_BYTES: usize = 25 * 1024 * 1024;
const ALLOWED_EXTENSIONS: &[&str] = &["pdf", "docx", "xlsx", "png", "jpg", "jpeg"];

#[derive(serde::Serialize)]
struct UploadResponse {
    document_ids: Vec<Uuid>,
    count: usize,
}

async fn upload(
    State(state): State<AppState>,
    Extension(tenant): Extension<TenantId>,
    mut multipart: Multipart,
) -> Result<(StatusCode, Json<UploadResponse>), StatusCode> {
    let storage = state.notify_storage.as_ref().ok_or_else(|| {
        tracing::error!("notify_storage not configured");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let mut files: Vec<(String, String, Vec<u8>)> = Vec::new();
    let mut total: usize = 0;

    while let Some(field) = multipart.next_field().await.map_err(|e| {
        tracing::warn!("multipart read: {e}");
        StatusCode::BAD_REQUEST
    })? {
        // file part 以外 (テキストフィールド等) は無視
        let Some(filename) = field.file_name().map(|s| s.to_string()) else {
            continue;
        };
        let content_type = field
            .content_type()
            .map(|s| s.to_string())
            .unwrap_or_else(|| "application/octet-stream".to_string());

        if !is_allowed_extension(&filename) {
            return Err(StatusCode::BAD_REQUEST);
        }

        let bytes = field.bytes().await.map_err(|e| {
            tracing::warn!("multipart bytes: {e}");
            StatusCode::BAD_REQUEST
        })?;

        total = total.saturating_add(bytes.len());
        if total > MAX_UPLOAD_TOTAL_BYTES {
            return Err(StatusCode::PAYLOAD_TOO_LARGE);
        }
        files.push((filename, content_type, bytes.to_vec()));
        if files.len() > MAX_UPLOAD_FILES {
            return Err(StatusCode::PAYLOAD_TOO_LARGE);
        }
    }

    if files.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let upload_batch_id = Uuid::new_v4();
    let mut keys_with_meta: Vec<(String, String, i64, String)> = Vec::with_capacity(files.len());
    for (filename, content_type, bytes) in &files {
        let key = format!(
            "{}/manual/{}/{}",
            tenant.0,
            upload_batch_id,
            sanitize_filename(filename)
        );
        storage
            .upload(&key, bytes, content_type)
            .await
            .map_err(|e| {
                tracing::error!("notify_storage.upload: {e}");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
        keys_with_meta.push((
            key,
            filename.clone(),
            bytes.len() as i64,
            content_type.clone(),
        ));
    }

    let pool = state.pool();
    let mut conn = pool.acquire().await.map_err(|e| {
        tracing::error!("pool acquire: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    set_current_tenant(&mut conn, &tenant.0.to_string())
        .await
        .map_err(|e| {
            tracing::error!("set_current_tenant: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let mut document_ids: Vec<Uuid> = Vec::with_capacity(keys_with_meta.len());
    for (r2_key, file_name, size, _ct) in &keys_with_meta {
        let id: Uuid = sqlx::query_scalar(
            r#"
            INSERT INTO notify_documents (
                tenant_id, source_type,
                r2_key, file_name, file_size_bytes,
                source_received_at,
                extraction_status, distribution_status
            )
            VALUES ($1, 'manual', $2, $3, $4, NOW(), 'pending', 'pending')
            RETURNING id
            "#,
        )
        .bind(tenant.0)
        .bind(r2_key)
        .bind(file_name)
        .bind(size)
        .fetch_one(&mut *conn)
        .await
        .map_err(|e| {
            tracing::error!("insert notify_document (manual): {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
        document_ids.push(id);

        // PDF だけ background redact (tokio::spawn で fire-and-forget)。
        // 結果は notify_documents.redaction_status カラムで追跡。
        if file_name.to_lowercase().ends_with(".pdf") {
            crate::background_redaction::spawn_redact_document(state.clone(), tenant.0, id);
        }

        // 配車手配票 PDF の積地/卸地/日時/注意事項抽出 (LINE 本文用)。
        // 拡張子チェックは background_extract 側でやるので毎回呼んで OK。
        crate::background_extract::spawn_extract_document(state.clone(), tenant.0, id);
    }

    let count = document_ids.len();
    Ok((
        StatusCode::CREATED,
        Json(UploadResponse {
            document_ids,
            count,
        }),
    ))
}

/// admin auth 付き redacted (or 原本) PDF inline ストリーム。
///
/// nuxt-notify の管理者ページで「ドキュメント詳細 → プレビュー」が呼ぶ。
/// `?original=true` で原本 PDF も取得可能 (admin 用、監査ログ用途)。
/// `redacted_r2_key` が NULL (まだ redact してない / skipped) の場合は原本にフォールバック。
#[derive(serde::Deserialize)]
struct PreviewQuery {
    original: Option<bool>,
}

async fn preview(
    State(state): State<AppState>,
    Extension(tenant): Extension<TenantId>,
    Path(id): Path<Uuid>,
    Query(q): Query<PreviewQuery>,
) -> Result<Response, StatusCode> {
    let doc = state
        .notify_documents
        .get(tenant.0, id)
        .await
        .map_err(|e| {
            tracing::error!("preview: get notify_document: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    let storage = state.notify_storage.as_ref().ok_or_else(|| {
        tracing::error!("preview: notify_storage not configured");
        StatusCode::SERVICE_UNAVAILABLE
    })?;

    let want_original = q.original.unwrap_or(false);
    let key = if want_original {
        doc.r2_key.as_str()
    } else {
        doc.redacted_r2_key.as_deref().unwrap_or(&doc.r2_key)
    };

    let bytes = storage.download(key).await.map_err(|e| {
        tracing::error!("preview: storage download: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // Content-Type は **実際の R2 key の拡張子** で決める。
    // doc.file_name (原本ファイル名 e.g. "3164_001.pdf") で判定すると、
    // redacted が .jpg になっていても application/pdf が返ってフロント側の
    // PDF.js が "Invalid PDF structure" でコケる。
    let mut headers = HeaderMap::new();
    let content_type = crate::viewer::guess_content_type(Some(key));
    if let Ok(v) = content_type.parse() {
        headers.insert(header::CONTENT_TYPE, v);
    }
    // download 時のファイル名は原本ファイル名 (UX の都合) で出す。
    let cd = crate::viewer::build_inline_disposition(doc.file_name.as_deref());
    if let Ok(v) = cd.parse() {
        headers.insert(header::CONTENT_DISPOSITION, v);
    }

    Ok((StatusCode::OK, headers, bytes).into_response())
}

/// force re-redact: redaction_status を pending に戻して spawn。即 202 Accepted。
///
/// 結果は redaction_status を polling で追跡する (UI 側で 5 秒間隔リフレッシュを想定)。
async fn redact_recompute(
    State(state): State<AppState>,
    Extension(tenant): Extension<TenantId>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let doc = state
        .notify_documents
        .get(tenant.0, id)
        .await
        .map_err(|e| {
            tracing::error!("redact_recompute: get notify_document: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    state
        .notify_documents
        .reset_redaction(tenant.0, doc.id)
        .await
        .map_err(|e| {
            tracing::error!("redact_recompute: reset_redaction: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    crate::background_redaction::spawn_redact_document(state.clone(), tenant.0, doc.id);
    Ok(StatusCode::ACCEPTED)
}

/// force re-extract: extracted_data.logistics を Gemini で再抽出して spawn。即 202 Accepted。
///
/// 既存ドキュメントの logistics 抽出をやり直したい場合 (staging で初回反映後の確認、
/// 本番で抽出ミスがあった場合の手動再実行) に使う。redaction_status とは独立。
async fn extract_recompute(
    State(state): State<AppState>,
    Extension(tenant): Extension<TenantId>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    // 存在確認 (RLS でテナント越境はそもそも 404)
    let _doc = state
        .notify_documents
        .get(tenant.0, id)
        .await
        .map_err(|e| {
            tracing::error!("extract_recompute: get notify_document: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    // extraction_status を 'pending' に戻し、extraction_error を NULL、updated_at
    // を NOW() に倒してから再 spawn (Refs ippoan/nuxt-notify#66)。これにより:
    //   - 前回 stuck (pending のまま固まった) / failed の状態が truthful にリセット
    //   - frontend の stuck 検知 (updated_at 起点の経過時間) も 0 に戻る
    // reset 失敗は致命ではない (background が完走時に上書きする) ので、エラーでも
    // spawn は続行し、ログだけ残す。
    if let Err(e) = state.notify_documents.reset_extraction(tenant.0, id).await {
        tracing::warn!("extract_recompute: reset_extraction failed (continuing): {e}");
    }

    crate::background_extract::spawn_extract_document(state.clone(), tenant.0, id);
    Ok(StatusCode::ACCEPTED)
}

fn is_allowed_extension(filename: &str) -> bool {
    let lower = filename.to_lowercase();
    ALLOWED_EXTENSIONS
        .iter()
        .any(|ext| lower.ends_with(&format!(".{ext}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allowed_extensions_basic() {
        assert!(is_allowed_extension("a.pdf"));
        assert!(is_allowed_extension("A.PDF"));
        assert!(is_allowed_extension("foo.docx"));
        assert!(is_allowed_extension("foo.bar.xlsx"));
        assert!(is_allowed_extension("photo.jpeg"));
        assert!(is_allowed_extension("photo.JPG"));
        assert!(is_allowed_extension("img.png"));
    }

    #[test]
    fn rejected_extensions() {
        assert!(!is_allowed_extension("a.exe"));
        assert!(!is_allowed_extension("a"));
        assert!(!is_allowed_extension(""));
        assert!(!is_allowed_extension(".pdf.exe"));
        assert!(!is_allowed_extension("pdf"));
    }

    #[test]
    fn upload_limits_constants() {
        assert_eq!(MAX_UPLOAD_FILES, 20);
        assert_eq!(MAX_UPLOAD_TOTAL_BYTES, 25 * 1024 * 1024);
    }
}
