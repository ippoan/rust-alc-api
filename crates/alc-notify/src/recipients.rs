use axum::{
    extract::{Path, State},
    http::StatusCode,
    Extension, Json, Router,
};

use alc_core::auth_middleware::TenantId;
use alc_core::repository::notify_recipients::{CreateNotifyRecipient, UpdateNotifyRecipient};
use alc_core::AppState;

pub fn tenant_router() -> Router<AppState> {
    Router::new()
        .route("/notify/recipients", axum::routing::get(list))
        .route("/notify/recipients", axum::routing::post(create))
        .route("/notify/recipients/{id}", axum::routing::get(get))
        .route("/notify/recipients/{id}", axum::routing::put(update))
        .route("/notify/recipients/{id}", axum::routing::delete(delete))
}

async fn list(
    State(state): State<AppState>,
    Extension(tenant): Extension<TenantId>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let recipients = state.notify_recipients.list(tenant.0).await.map_err(|e| {
        tracing::error!("list notify_recipients: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(serde_json::to_value(recipients).unwrap()))
}

async fn get(
    State(state): State<AppState>,
    Extension(tenant): Extension<TenantId>,
    Path(id): Path<uuid::Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let recipient = state
        .notify_recipients
        .get(tenant.0, id)
        .await
        .map_err(|e| {
            tracing::error!("get notify_recipient: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(serde_json::to_value(recipient).unwrap()))
}

async fn create(
    State(state): State<AppState>,
    Extension(tenant): Extension<TenantId>,
    Json(input): Json<CreateNotifyRecipient>,
) -> Result<(StatusCode, Json<serde_json::Value>), StatusCode> {
    let recipient = state
        .notify_recipients
        .create(tenant.0, &input)
        .await
        .map_err(|e| {
            tracing::error!("create notify_recipient: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok((
        StatusCode::CREATED,
        Json(serde_json::to_value(recipient).unwrap()),
    ))
}

async fn update(
    State(state): State<AppState>,
    Extension(tenant): Extension<TenantId>,
    Path(id): Path<uuid::Uuid>,
    Json(input): Json<UpdateNotifyRecipient>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let recipient = state
        .notify_recipients
        .update(tenant.0, id, &input)
        .await
        .map_err(|e| {
            tracing::error!("update notify_recipient: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(Json(serde_json::to_value(recipient).unwrap()))
}

async fn delete(
    State(state): State<AppState>,
    Extension(tenant): Extension<TenantId>,
    Path(id): Path<uuid::Uuid>,
) -> Result<StatusCode, StatusCode> {
    state
        .notify_recipients
        .delete(tenant.0, id)
        .await
        .map_err(|e| {
            tracing::error!("delete notify_recipient: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(StatusCode::NO_CONTENT)
}
