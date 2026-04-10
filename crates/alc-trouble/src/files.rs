use axum::{
    extract::{Multipart, Path, State},
    http::StatusCode,
    routing::{delete, get, post},
    Json, Router,
};
use uuid::Uuid;

use crate::TroubleState;
use alc_core::auth_middleware::TenantId;
use alc_core::models::TroubleFile;

pub fn tenant_router<S>() -> Router<S>
where
    TroubleState: axum::extract::FromRef<S>,
    S: Clone + Send + Sync + 'static,
{
    Router::new()
        .route(
            "/trouble/tickets/{ticket_id}/files",
            post(upload_file).get(list_files),
        )
        .route("/trouble/files/{file_id}", delete(delete_file))
        .route("/trouble/files/{file_id}/download", get(download_file))
}

async fn upload_file(
    State(state): State<TroubleState>,
    tenant: axum::Extension<TenantId>,
    Path(ticket_id): Path<Uuid>,
    mut multipart: Multipart,
) -> Result<(StatusCode, Json<TroubleFile>), StatusCode> {
    let tenant_id = tenant.0 .0;

    // チケットの存在確認
    state
        .trouble_tickets
        .get(tenant_id, ticket_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    let storage = state
        .trouble_storage
        .as_ref()
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;

    let field = multipart
        .next_field()
        .await
        .map_err(|_| StatusCode::BAD_REQUEST)?
        .ok_or(StatusCode::BAD_REQUEST)?;

    let filename = field.file_name().unwrap_or("unknown").to_string();
    let content_type = field
        .content_type()
        .unwrap_or("application/octet-stream")
        .to_string();
    let data = field.bytes().await.map_err(|_| StatusCode::BAD_REQUEST)?;
    let size_bytes = data.len() as i64;

    let file_uuid = Uuid::new_v4();
    let ext = filename.rsplit('.').next().unwrap_or("bin");
    let storage_key = format!("{tenant_id}/trouble/{ticket_id}/{file_uuid}.{ext}");

    storage
        .upload(&storage_key, &data, &content_type)
        .await
        .map_err(|e| {
            tracing::error!("storage upload error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let file = state
        .trouble_files
        .create(
            tenant_id,
            ticket_id,
            &filename,
            &content_type,
            size_bytes,
            &storage_key,
        )
        .await
        .map_err(|e| {
            tracing::error!("create_file DB error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok((StatusCode::CREATED, Json(file)))
}

async fn list_files(
    State(state): State<TroubleState>,
    tenant: axum::Extension<TenantId>,
    Path(ticket_id): Path<Uuid>,
) -> Result<Json<Vec<TroubleFile>>, StatusCode> {
    let files = state
        .trouble_files
        .list_by_ticket(tenant.0 .0, ticket_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(files))
}

async fn download_file(
    State(state): State<TroubleState>,
    tenant: axum::Extension<TenantId>,
    Path(file_id): Path<Uuid>,
) -> Result<impl axum::response::IntoResponse, StatusCode> {
    let file = state
        .trouble_files
        .get(tenant.0 .0, file_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    let storage = state
        .trouble_storage
        .as_ref()
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;

    let data = storage.download(&file.storage_key).await.map_err(|e| {
        tracing::error!("storage download error: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok((
        [
            (axum::http::header::CONTENT_TYPE, file.content_type.clone()),
            (
                axum::http::header::CONTENT_DISPOSITION,
                format!(
                    "attachment; filename=\"{}\"",
                    file.filename.replace('"', "_")
                ),
            ),
        ],
        data,
    ))
}

async fn delete_file(
    State(state): State<TroubleState>,
    tenant: axum::Extension<TenantId>,
    Path(file_id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let deleted = state
        .trouble_files
        .delete(tenant.0 .0, file_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}
