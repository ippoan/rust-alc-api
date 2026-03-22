use axum::{
    extract::{Multipart, State},
    http::StatusCode,
    routing::post,
    Json, Router,
};
use serde::Serialize;

use crate::AppState;
use crate::middleware::auth::TenantId;

pub fn tenant_router() -> Router<AppState> {
    Router::new()
        .route("/upload/face-photo", post(upload_face_photo))
        .route("/upload/report-audio", post(upload_report_audio))
        .route("/upload/blow-video", post(upload_blow_video))
}

#[derive(Debug, Serialize)]
pub struct UploadResponse {
    pub url: String,
    pub filename: String,
}

async fn upload_face_photo(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    mut multipart: Multipart,
) -> Result<Json<UploadResponse>, StatusCode> {
    let tenant_id = tenant.0 .0;

    let field = multipart
        .next_field()
        .await
        .map_err(|_| StatusCode::BAD_REQUEST)?
        .ok_or(StatusCode::BAD_REQUEST)?;

    let filename = format!("{}.jpg", uuid::Uuid::new_v4());

    let data = field
        .bytes()
        .await
        .map_err(|_| StatusCode::BAD_REQUEST)?;

    let object_path = format!("{tenant_id}/{filename}");

    let url = state
        .storage
        .upload(&object_path, &data, "image/jpeg")
        .await
        .map_err(|e| {
            tracing::error!("Storage upload failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(UploadResponse { url, filename }))
}

async fn upload_report_audio(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    mut multipart: Multipart,
) -> Result<Json<UploadResponse>, StatusCode> {
    let tenant_id = tenant.0 .0;

    let field = multipart
        .next_field()
        .await
        .map_err(|_| StatusCode::BAD_REQUEST)?
        .ok_or(StatusCode::BAD_REQUEST)?;

    let filename = format!("{}.webm", uuid::Uuid::new_v4());

    let data = field
        .bytes()
        .await
        .map_err(|_| StatusCode::BAD_REQUEST)?;

    let object_path = format!("{tenant_id}/report-audio/{filename}");

    let url = state
        .storage
        .upload(&object_path, &data, "audio/webm")
        .await
        .map_err(|e| {
            tracing::error!("Storage upload failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(UploadResponse { url, filename }))
}

async fn upload_blow_video(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    mut multipart: Multipart,
) -> Result<Json<UploadResponse>, StatusCode> {
    let tenant_id = tenant.0 .0;

    let field = multipart
        .next_field()
        .await
        .map_err(|_| StatusCode::BAD_REQUEST)?
        .ok_or(StatusCode::BAD_REQUEST)?;

    let filename = format!("{}.webm", uuid::Uuid::new_v4());

    let data = field
        .bytes()
        .await
        .map_err(|_| StatusCode::BAD_REQUEST)?;

    let object_path = format!("{tenant_id}/blow-video/{filename}");

    let url = state
        .storage
        .upload(&object_path, &data, "video/webm")
        .await
        .map_err(|e| {
            tracing::error!("Storage upload failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(UploadResponse { url, filename }))
}
