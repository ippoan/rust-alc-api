use axum::{
    extract::{Path, Query, State},
    http::{header, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Extension, Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use alc_core::auth_middleware::TenantId;
use alc_core::repository::car_inspections::{CarInspectionRepository, CreateFileLinkParams};
use alc_core::repository::carins_files::FileRow;
use alc_core::AppState;

pub fn tenant_router() -> Router<AppState> {
    Router::new()
        .route("/files", get(list_files).post(create_file))
        .route("/files/recent", get(list_recent))
        .route("/files/not-attached", get(list_not_attached))
        .route("/files/{uuid}", get(get_file))
        .route("/files/{uuid}/download", get(download_file))
        .route("/files/{uuid}/delete", post(delete_file))
        .route("/files/{uuid}/restore", post(restore_file))
}

#[derive(Debug, Serialize, ts_rs::TS)]
#[ts(export, rename = "FileListResponse")]
struct ListResponse {
    files: Vec<FileRow>,
}

#[derive(Debug, Deserialize)]
struct ListQuery {
    #[serde(rename = "type")]
    type_filter: Option<String>,
}

async fn list_files(
    State(state): State<AppState>,
    Extension(tenant_id): Extension<TenantId>,
    Query(q): Query<ListQuery>,
) -> Result<Json<ListResponse>, StatusCode> {
    let rows = state
        .carins_files
        .list_files(tenant_id.0, q.type_filter.as_deref())
        .await
        .map_err(|e| {
            tracing::error!("list_files failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(ListResponse { files: rows }))
}

async fn list_recent(
    State(state): State<AppState>,
    Extension(tenant_id): Extension<TenantId>,
) -> Result<Json<ListResponse>, StatusCode> {
    let rows = state
        .carins_files
        .list_recent(tenant_id.0)
        .await
        .map_err(|e| {
            tracing::error!("list_recent failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(ListResponse { files: rows }))
}

async fn list_not_attached(
    State(state): State<AppState>,
    Extension(tenant_id): Extension<TenantId>,
) -> Result<Json<ListResponse>, StatusCode> {
    let rows = state
        .carins_files
        .list_not_attached(tenant_id.0)
        .await
        .map_err(|e| {
            tracing::error!("list_not_attached failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(ListResponse { files: rows }))
}

async fn get_file(
    State(state): State<AppState>,
    Extension(tenant_id): Extension<TenantId>,
    Path(uuid): Path<String>,
) -> Result<Json<FileRow>, StatusCode> {
    let row = state
        .carins_files
        .get_file(tenant_id.0, &uuid)
        .await
        .map_err(|e| {
            tracing::error!("get_file failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(row))
}

async fn download_file(
    State(state): State<AppState>,
    Extension(tenant_id): Extension<TenantId>,
    Path(uuid): Path<String>,
) -> Result<impl IntoResponse, StatusCode> {
    // Get file metadata (includes blob for legacy storage)
    let row = state
        .carins_files
        .get_file_for_download(tenant_id.0, &uuid)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    // Download from GCS
    if let Some(ref s3_key) = row.s3_key {
        let data = state
            .carins_storage
            .as_ref()
            .unwrap_or(&state.storage)
            .download(s3_key)
            .await
            .map_err(|e| {
                tracing::error!("GCS download failed: {e}");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;

        let content_type = row.file_type.clone();
        let filename = row.filename.clone();

        Ok((
            [
                (header::CONTENT_TYPE, content_type),
                (
                    header::CONTENT_DISPOSITION,
                    format!("attachment; filename=\"{}\"", filename),
                ),
            ],
            data,
        ))
    } else if let Some(ref blob) = row.blob {
        // Legacy blob storage (base64)
        use base64::{engine::general_purpose::STANDARD, Engine};
        let data = STANDARD
            .decode(blob)
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        let content_type = row.file_type.clone();
        let filename = row.filename.clone();

        Ok((
            [
                (header::CONTENT_TYPE, content_type),
                (
                    header::CONTENT_DISPOSITION,
                    format!("attachment; filename=\"{}\"", filename),
                ),
            ],
            data,
        ))
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

#[derive(Debug, Deserialize)]
struct CreateFileRequest {
    filename: String,
    #[serde(rename = "type")]
    file_type: String,
    content: String, // base64 encoded
}

async fn create_file(
    State(state): State<AppState>,
    Extension(tenant_id): Extension<TenantId>,
    Json(body): Json<CreateFileRequest>,
) -> Result<(StatusCode, Json<FileRow>), StatusCode> {
    let file_uuid = Uuid::new_v4();
    let now = chrono::Utc::now();
    let gcs_key = format!("{}/{}", tenant_id.0, file_uuid);

    // Decode base64 and upload to GCS
    use base64::{engine::general_purpose::STANDARD, Engine};
    let data = STANDARD
        .decode(&body.content)
        .map_err(|_| StatusCode::BAD_REQUEST)?;

    state
        .carins_storage
        .as_ref()
        .unwrap_or(&state.storage)
        .upload(&gcs_key, &data, &body.file_type)
        .await
        .map_err(|e| {
            tracing::error!("GCS upload failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let row = state
        .carins_files
        .create_file(
            tenant_id.0,
            file_uuid,
            &body.filename,
            &body.file_type,
            &gcs_key,
            now,
        )
        .await
        .map_err(|e| {
            tracing::error!("create_file DB insert failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    // 車検証ファイル自動パース (JSON / PDF)
    if body.file_type == "application/json" {
        if let Err(e) = try_parse_car_inspection_json(
            state.car_inspections.as_ref(),
            tenant_id.0,
            file_uuid,
            &data,
        )
        .await
        {
            tracing::warn!("car inspection JSON parse skipped for {file_uuid}: {e}");
        }
    } else if body.file_type == "application/pdf" {
        if let Err(e) = try_parse_car_inspection_pdf(
            state.car_inspections.as_ref(),
            tenant_id.0,
            file_uuid,
            &data,
        )
        .await
        {
            tracing::warn!("car inspection PDF parse skipped for {file_uuid}: {e}");
        }
    }

    Ok((StatusCode::CREATED, Json(row)))
}

/// 車検証 JSON をパースして car_inspection UPSERT + files_a リンク + pending PDF チェック
async fn try_parse_car_inspection_json(
    repo: &dyn CarInspectionRepository,
    tenant_id: Uuid,
    file_uuid: Uuid,
    data: &[u8],
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let json: serde_json::Value = serde_json::from_slice(data)?;

    let cert_info = json.get("CertInfo").ok_or("missing CertInfo")?;

    let elect_cert_mg_no = cert_info
        .get("ElectCertMgNo")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .ok_or("missing or empty ElectCertMgNo")?;

    let version = json
        .get("CertInfoImportFileVersion")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let grantdate_e = strip_spaces_field(cert_info, "GrantdateE");
    let grantdate_y = strip_spaces_field(cert_info, "GrantdateY");
    let grantdate_m = strip_spaces_field(cert_info, "GrantdateM");
    let grantdate_d = strip_spaces_field(cert_info, "GrantdateD");

    // 1. car_inspection UPSERT
    repo.upsert_from_json(tenant_id, cert_info, version).await?;

    // 2. car_inspection_files_a リンク
    repo.create_file_link(&CreateFileLinkParams {
        tenant_id,
        file_uuid,
        file_type: "application/json",
        elect_cert_mg_no,
        grantdate_e: &grantdate_e,
        grantdate_y: &grantdate_y,
        grantdate_m: &grantdate_m,
        grantdate_d: &grantdate_d,
    })
    .await?;

    tracing::info!(
        "car inspection JSON parsed: ElectCertMgNo={}, file={}",
        elect_cert_mg_no,
        file_uuid
    );

    // 3. pending PDF チェック — PDF が先にアップロードされていれば files_b にリンク
    if let Ok(Some(pdf_uuid_str)) = repo.find_pending_pdf(tenant_id, elect_cert_mg_no).await {
        if let Ok(pdf_uuid) = pdf_uuid_str.parse::<Uuid>() {
            let _ = repo
                .create_file_link(&CreateFileLinkParams {
                    tenant_id,
                    file_uuid: pdf_uuid,
                    file_type: "application/pdf",
                    elect_cert_mg_no,
                    grantdate_e: &grantdate_e,
                    grantdate_y: &grantdate_y,
                    grantdate_m: &grantdate_m,
                    grantdate_d: &grantdate_d,
                })
                .await;
            let _ = repo.delete_pending_pdf(tenant_id, elect_cert_mg_no).await;
            tracing::info!(
                "linked pending PDF: pdf={}, ElectCertMgNo={}",
                pdf_uuid,
                elect_cert_mg_no
            );
        }
    }

    Ok(())
}

// PDF 解析用の正規表現パターン (rust-logi file_auto_parser.rs から移植)
use std::sync::LazyLock;

/// 車検証判定
static RE_CAR_INSPECTION: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"自\s*動\s*車\s*検\s*査\s*証\s*記\s*録\s*事\s*項").unwrap()
});

/// ElectCertMgNo: 12桁数字
static RE_ELECT_CERT_MG_NO: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"\d{12}").unwrap());

/// Grantdate: pdf-extract 形式 "令 和  8  2  13 月 日"
static RE_GRANTDATE_HEADER: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(
        r"(?s)記録年月日.*?(令\s*和|平\s*成|昭\s*和)\s+(\d{1,2})\s+(\d{1,2})\s+(\d{1,2})",
    )
    .unwrap()
});

/// Grantdate: 標準日本語形式 "令和8年2月13日"
static RE_GRANTDATE_STANDARD: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(
        r"(?s)記録年月日.*?(令\s*和|平\s*成|昭\s*和)\s*(\d{1,2})\s*年\s*(\d{1,2})\s*月\s*(\d{1,2})\s*日",
    )
    .unwrap()
});

/// Grantdate: 備考セクション内フォールバック
static RE_GRANTDATE_BIKO: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(
        r"(?s)４[.．]\s*備考.*?(令\s*和|平\s*成|昭\s*和)\s+(\d{1,2})\s+(\d{1,2})\s+(\d{1,2})",
    )
    .unwrap()
});

/// PDF アップロード時の車検証自動パース
async fn try_parse_car_inspection_pdf(
    repo: &dyn CarInspectionRepository,
    tenant_id: Uuid,
    file_uuid: Uuid,
    data: &[u8],
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // 1. PDF テキスト抽出 (1ページ目のみ)
    let pages = pdf_extract::extract_text_from_mem_by_pages(data)?;
    let page1 = pages.first().ok_or("PDF has no pages")?;
    if page1.is_empty() {
        return Err("PDF page 1 has no text".into());
    }

    // 2. 車検証 PDF 判定
    if !RE_CAR_INSPECTION.is_match(page1) {
        return Ok(()); // 車検証ではない PDF → スキップ
    }

    // 3. ElectCertMgNo 抽出 (12桁数字)
    let elect_cert_mg_no = RE_ELECT_CERT_MG_NO
        .find(page1)
        .ok_or("car inspection PDF but no ElectCertMgNo found")?
        .as_str();

    // 4. Grantdate 抽出 (3パターンのフォールバック)
    let caps = RE_GRANTDATE_HEADER
        .captures(page1)
        .or_else(|| RE_GRANTDATE_STANDARD.captures(page1))
        .or_else(|| RE_GRANTDATE_BIKO.captures(page1))
        .ok_or("car inspection PDF but Grantdate not found")?;

    let grantdate_e = strip_spaces_str(&caps[1]);
    let grantdate_y = strip_spaces_str(&caps[2]);
    let grantdate_m = strip_spaces_str(&caps[3]);
    let grantdate_d = strip_spaces_str(&caps[4]);

    tracing::info!(
        "car inspection PDF parsed: ElectCertMgNo={}, Grantdate={}-{}-{}-{}, file={}",
        elect_cert_mg_no,
        grantdate_e,
        grantdate_y,
        grantdate_m,
        grantdate_d,
        file_uuid
    );

    let params = CreateFileLinkParams {
        tenant_id,
        file_uuid,
        file_type: "application/pdf",
        elect_cert_mg_no,
        grantdate_e: &grantdate_e,
        grantdate_y: &grantdate_y,
        grantdate_m: &grantdate_m,
        grantdate_d: &grantdate_d,
    };

    // 5. JSON が既に存在するか確認
    let json_exists = repo
        .json_file_exists(
            tenant_id,
            elect_cert_mg_no,
            &grantdate_e,
            &grantdate_y,
            &grantdate_m,
            &grantdate_d,
        )
        .await?;

    if json_exists {
        // JSON あり → files_b に直接リンク
        repo.create_file_link(&params).await?;
        tracing::info!(
            "PDF linked to files_b: uuid={}, ElectCertMgNo={}",
            file_uuid,
            elect_cert_mg_no
        );
    } else {
        // JSON なし → pending に保存 (JSON 待ち)
        repo.upsert_pending_pdf(&params).await?;
        tracing::info!(
            "PDF stored as pending: uuid={}, ElectCertMgNo={}",
            file_uuid,
            elect_cert_mg_no
        );
    }

    Ok(())
}

fn strip_spaces_field(v: &serde_json::Value, key: &str) -> String {
    let s = v.get(key).and_then(|v| v.as_str()).unwrap_or("");
    s.replace([' ', '\u{3000}'], "")
}

fn strip_spaces_str(s: &str) -> String {
    s.replace([' ', '\u{3000}'], "")
}

async fn delete_file(
    State(state): State<AppState>,
    Extension(tenant_id): Extension<TenantId>,
    Path(uuid): Path<String>,
) -> Result<StatusCode, StatusCode> {
    let affected = state
        .carins_files
        .delete_file(tenant_id.0, &uuid)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if !affected {
        return Err(StatusCode::NOT_FOUND);
    }

    Ok(StatusCode::NO_CONTENT)
}

async fn restore_file(
    State(state): State<AppState>,
    Extension(tenant_id): Extension<TenantId>,
    Path(uuid): Path<String>,
) -> Result<StatusCode, StatusCode> {
    let affected = state
        .carins_files
        .restore_file(tenant_id.0, &uuid)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if !affected {
        return Err(StatusCode::NOT_FOUND);
    }

    Ok(StatusCode::NO_CONTENT)
}
