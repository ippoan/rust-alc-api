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
        .storage
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

    // JSON ファイルの場合、車検証データをパースして UPSERT + ファイルリンク作成
    if body.file_type == "application/json" {
        if let Err(e) = try_parse_car_inspection(
            state.car_inspections.as_ref(),
            tenant_id.0,
            file_uuid,
            &data,
            &body.file_type,
        )
        .await
        {
            tracing::warn!("car inspection parse skipped for {file_uuid}: {e}");
        }
    }

    Ok((StatusCode::CREATED, Json(row)))
}

/// 車検証 JSON をパースして car_inspection UPSERT + car_inspection_files_a リンク作成
async fn try_parse_car_inspection(
    repo: &dyn CarInspectionRepository,
    tenant_id: Uuid,
    file_uuid: Uuid,
    data: &[u8],
    file_type: &str,
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

    // GrantdateE/Y/M/D をスペース除去して取得
    let grantdate_e = strip_spaces_field(cert_info, "GrantdateE");
    let grantdate_y = strip_spaces_field(cert_info, "GrantdateY");
    let grantdate_m = strip_spaces_field(cert_info, "GrantdateM");
    let grantdate_d = strip_spaces_field(cert_info, "GrantdateD");

    repo.upsert_from_json(tenant_id, cert_info, version).await?;

    repo.create_file_link(&CreateFileLinkParams {
        tenant_id,
        file_uuid,
        file_type,
        elect_cert_mg_no,
        grantdate_e: &grantdate_e,
        grantdate_y: &grantdate_y,
        grantdate_m: &grantdate_m,
        grantdate_d: &grantdate_d,
    })
    .await?;

    tracing::info!(
        "car inspection parsed: ElectCertMgNo={}, file={}",
        elect_cert_mg_no,
        file_uuid
    );
    Ok(())
}

fn strip_spaces_field(v: &serde_json::Value, key: &str) -> String {
    let s = v.get(key).and_then(|v| v.as_str()).unwrap_or("");
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
