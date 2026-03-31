use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use uuid::Uuid;

use alc_core::auth_middleware::TenantId;
use alc_core::models::{CreateWebhookConfig, WebhookConfig, WebhookDelivery};
use alc_core::AppState;

/// テナント対応ルート (JWT or X-Tenant-ID)
pub fn tenant_router() -> Router<AppState> {
    Router::new()
        .route("/tenko/webhooks", post(upsert_webhook).get(list_webhooks))
        .route(
            "/tenko/webhooks/{id}",
            get(get_webhook).delete(delete_webhook),
        )
        .route("/tenko/webhooks/{id}/deliveries", get(list_deliveries))
}

/// Webhook 作成/更新 (event_type が同じなら UPSERT)
async fn upsert_webhook(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Json(body): Json<CreateWebhookConfig>,
) -> Result<(StatusCode, Json<WebhookConfig>), StatusCode> {
    let tenant_id = tenant.0 .0;

    let valid_events = [
        "alcohol_detected",
        "tenko_overdue",
        "tenko_completed",
        "tenko_cancelled",
        "tenko_interrupted",
        "inspection_ng",
        "safety_judgment_fail",
        "equipment_failure",
        "report_submitted",
    ];
    if !valid_events.contains(&body.event_type.as_str()) {
        return Err(StatusCode::BAD_REQUEST);
    }

    let config = state
        .tenko_webhooks
        .upsert(tenant_id, &body)
        .await
        .map_err(|e| {
            tracing::error!("upsert_webhook error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok((StatusCode::CREATED, Json(config)))
}

async fn list_webhooks(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
) -> Result<Json<Vec<WebhookConfig>>, StatusCode> {
    let tenant_id = tenant.0 .0;

    let configs = state
        .tenko_webhooks
        .list(tenant_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(configs))
}

async fn get_webhook(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Path(id): Path<Uuid>,
) -> Result<Json<WebhookConfig>, StatusCode> {
    let tenant_id = tenant.0 .0;

    let config = state
        .tenko_webhooks
        .get(tenant_id, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(config))
}

async fn delete_webhook(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let tenant_id = tenant.0 .0;

    let deleted = state
        .tenko_webhooks
        .delete(tenant_id, id)
        .await
        .map_err(|e| {
            tracing::error!("delete_webhook error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    if !deleted {
        return Err(StatusCode::NOT_FOUND);
    }

    Ok(StatusCode::NO_CONTENT)
}

async fn list_deliveries(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<WebhookDelivery>>, StatusCode> {
    let tenant_id = tenant.0 .0;

    let deliveries = state
        .tenko_webhooks
        .list_deliveries(tenant_id, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(deliveries))
}
