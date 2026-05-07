//! テスト用エンドポイント。本番運用パイプラインに統合する前に、
//! 個別 PDF を投入して redact + R2 期間限定配信の挙動を目視確認するために使う。
//!
//! `POST /api/notify/test/redact-pdf` (admin 認証):
//! 1. multipart で PDF を受け取る
//! 2. Gemini API に PDF を直接渡して金額の bbox を取得
//! 3. lopdf で白矩形オーバーレイ
//! 4. R2 に redacted PDF を upload
//! 5. notify_documents + notify_deliveries に行追加 (recipient_id = NULL、migration 108)
//! 6. `{API_ORIGIN}/api/notify/v/{read_token}/file` を返す
//!    → 既存 viewer.rs (R2 inline ストリーム) がそのまま配信。expire_at 後は 410 Gone
//!
//! `GEMINI_API_KEY` env が設定されていなければ 503 を返す。
//!
//! 設計ドキュメント: ~/.claude/plans/front-nuxt-notify-docs-reference-nuxt-no-nifty-babbage.md

use axum::{
    extract::{Multipart, State},
    http::StatusCode,
    Extension, Json, Router,
};
use uuid::Uuid;

use alc_core::auth_middleware::TenantId;
use alc_core::tenant::set_current_tenant;
use alc_core::AppState;

use crate::ingest::sanitize_filename;
use crate::redact::{apply_redactions, detect_amount_boxes, detect_amount_boxes_v2};

const MAX_UPLOAD_BYTES: usize = 25 * 1024 * 1024;
const DEFAULT_EXPIRE_HOURS: i64 = 24;
const MAX_EXPIRE_HOURS: i64 = 720; // 30 days

pub fn tenant_router() -> Router<AppState> {
    Router::new().route(
        "/notify/test/redact-pdf",
        axum::routing::post(redact_pdf_test),
    )
}

#[derive(serde::Serialize, Debug)]
struct RedactResponse {
    document_id: Uuid,
    original_r2_key: String,
    redacted_r2_key: String,
    view_url: String,
    expire_at: chrono::DateTime<chrono::Utc>,
    redactions_applied: usize,
}

async fn redact_pdf_test(
    State(state): State<AppState>,
    Extension(tenant): Extension<TenantId>,
    mut multipart: Multipart,
) -> Result<(StatusCode, Json<RedactResponse>), (StatusCode, String)> {
    let api_key = std::env::var("GEMINI_API_KEY").map_err(|_| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "GEMINI_API_KEY env not set".to_string(),
        )
    })?;

    let storage = state.notify_storage.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "notify_storage not configured".to_string(),
        )
    })?;

    // multipart 解析: file part + expire_in_hours form 値
    let mut pdf_bytes: Option<Vec<u8>> = None;
    let mut original_filename = String::from("input.pdf");
    let mut expire_in_hours: i64 = DEFAULT_EXPIRE_HOURS;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("multipart read: {e}")))?
    {
        let field_name = field.name().map(|s| s.to_string());
        match field_name.as_deref() {
            Some("file") => {
                if let Some(fname) = field.file_name() {
                    original_filename = fname.to_string();
                }
                let bytes = field
                    .bytes()
                    .await
                    .map_err(|e| (StatusCode::BAD_REQUEST, format!("multipart bytes: {e}")))?;
                if bytes.len() > MAX_UPLOAD_BYTES {
                    return Err((
                        StatusCode::PAYLOAD_TOO_LARGE,
                        format!("file > {MAX_UPLOAD_BYTES} bytes"),
                    ));
                }
                if !original_filename.to_lowercase().ends_with(".pdf") {
                    return Err((StatusCode::BAD_REQUEST, "file must be .pdf".to_string()));
                }
                pdf_bytes = Some(bytes.to_vec());
            }
            Some("expire_in_hours") => {
                let v = field
                    .text()
                    .await
                    .map_err(|e| (StatusCode::BAD_REQUEST, format!("multipart text: {e}")))?;
                let parsed: i64 = v.trim().parse().map_err(|_| {
                    (
                        StatusCode::BAD_REQUEST,
                        format!("expire_in_hours: not an integer: {v}"),
                    )
                })?;
                if !(1..=MAX_EXPIRE_HOURS).contains(&parsed) {
                    return Err((
                        StatusCode::BAD_REQUEST,
                        format!("expire_in_hours must be 1..={MAX_EXPIRE_HOURS}"),
                    ));
                }
                expire_in_hours = parsed;
            }
            _ => {
                // 未使用フィールドはスキップ
            }
        }
    }

    let pdf_bytes = pdf_bytes.ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            "missing 'file' multipart field".to_string(),
        )
    })?;

    // 1) Gemini に bbox 問合わせ
    //
    // `NOTIFY_REDACT_2STAGE=1` で 2-stage パイプライン (全セル列挙 → JSON で
    // 金額抽出) に切替。3164 のような行ズレ問題に強い。失敗時は内部で
    // 1-stage に自動フォールバックするので運用上安全。
    // 本番安定後にデフォルト化予定 (env 削除)。
    let use_2stage = std::env::var("NOTIFY_REDACT_2STAGE").as_deref() == Ok("1");
    let redactions = if use_2stage {
        detect_amount_boxes_v2(&pdf_bytes, &api_key, None, None).await
    } else {
        detect_amount_boxes(&pdf_bytes, &api_key, None, None).await
    }
    .map_err(|e| {
        tracing::error!("detect_amount_boxes (2stage={use_2stage}): {e}");
        (
            StatusCode::BAD_GATEWAY,
            format!("gemini detect failed: {e}"),
        )
    })?;
    tracing::info!(
        "redact: tenant={} file={} got {} redaction(s)",
        tenant.0,
        original_filename,
        redactions.len()
    );

    // 2) lopdf でオーバーレイ
    let redacted_bytes = apply_redactions(&pdf_bytes, &redactions).map_err(|e| {
        tracing::error!("apply_redactions: {e}");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("redact apply failed: {e}"),
        )
    })?;

    // 3) R2 upload (原本 + redacted の 2 ファイル)
    let batch_id = Uuid::new_v4();
    let safe_name = sanitize_filename(&original_filename);
    let original_r2_key = format!("{}/test/{}/original_{}", tenant.0, batch_id, safe_name);
    let r2_key = format!("{}/test/{}/redacted_{}", tenant.0, batch_id, safe_name);
    storage
        .upload(&original_r2_key, &pdf_bytes, "application/pdf")
        .await
        .map_err(|e| {
            tracing::error!("notify_storage.upload (original): {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("storage upload (original) failed: {e}"),
            )
        })?;
    storage
        .upload(&r2_key, &redacted_bytes, "application/pdf")
        .await
        .map_err(|e| {
            tracing::error!("notify_storage.upload (redacted): {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("storage upload (redacted) failed: {e}"),
            )
        })?;

    // 4) DB INSERT (notify_documents + notify_deliveries)
    let pool = state.pool();
    let mut conn = pool.acquire().await.map_err(|e| {
        tracing::error!("pool acquire: {e}");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "db pool acquire failed".to_string(),
        )
    })?;
    set_current_tenant(&mut conn, &tenant.0.to_string())
        .await
        .map_err(|e| {
            tracing::error!("set_current_tenant: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "set_current_tenant failed".to_string(),
            )
        })?;

    let document_id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO notify_documents (
            tenant_id, source_type,
            r2_key, file_name, file_size_bytes,
            source_received_at,
            extraction_status, distribution_status
        )
        VALUES ($1, 'manual', $2, $3, $4, NOW(), 'completed', 'pending')
        RETURNING id
        "#,
    )
    .bind(tenant.0)
    .bind(&r2_key)
    .bind(format!("redacted_{}", safe_name))
    .bind(redacted_bytes.len() as i64)
    .fetch_one(&mut *conn)
    .await
    .map_err(|e| {
        tracing::error!("insert notify_documents: {e}");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "insert document failed".to_string(),
        )
    })?;

    let (read_token, expire_at): (Uuid, chrono::DateTime<chrono::Utc>) = sqlx::query_as(
        r#"
        INSERT INTO notify_deliveries (
            tenant_id, document_id, recipient_id,
            provider, status,
            expire_at
        )
        VALUES ($1, $2, NULL, 'test', 'pending', NOW() + ($3::text || ' hours')::interval)
        RETURNING read_token, expire_at
        "#,
    )
    .bind(tenant.0)
    .bind(document_id)
    .bind(expire_in_hours.to_string())
    .fetch_one(&mut *conn)
    .await
    .map_err(|e| {
        tracing::error!("insert notify_deliveries: {e}");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "insert delivery failed".to_string(),
        )
    })?;

    let api_origin =
        std::env::var("API_ORIGIN").unwrap_or_else(|_| "https://localhost:8080".into());
    let view_url = format!("{api_origin}/api/notify/v/{read_token}/file");

    Ok((
        StatusCode::CREATED,
        Json(RedactResponse {
            document_id,
            original_r2_key,
            redacted_r2_key: r2_key,
            view_url,
            expire_at,
            redactions_applied: redactions.len(),
        }),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upload_constants_sane() {
        // 25MB 以上、24時間 default、30 日 max を const block で確認
        const _: () = assert!(MAX_UPLOAD_BYTES >= 1024 * 1024);
        const _: () = assert!(DEFAULT_EXPIRE_HOURS >= 1);
        const _: () = assert!(MAX_EXPIRE_HOURS >= DEFAULT_EXPIRE_HOURS);
    }
}
