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

#[derive(Debug, Serialize)]
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

    Ok((StatusCode::CREATED, Json(row)))
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
