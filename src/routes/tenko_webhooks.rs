use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use uuid::Uuid;

use crate::db::models::{CreateWebhookConfig, WebhookConfig, WebhookDelivery};
use crate::db::tenant::set_current_tenant;
use crate::middleware::auth::TenantId;
use crate::AppState;

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

    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let config = sqlx::query_as::<_, WebhookConfig>(
        r#"
        INSERT INTO webhook_configs (tenant_id, event_type, url, secret, enabled)
        VALUES ($1, $2, $3, $4, $5)
        ON CONFLICT (tenant_id, event_type)
        DO UPDATE SET url = $3, secret = $4, enabled = $5, updated_at = NOW()
        RETURNING *
        "#,
    )
    .bind(tenant_id)
    .bind(&body.event_type)
    .bind(&body.url)
    .bind(&body.secret)
    .bind(body.enabled)
    .fetch_one(&mut *conn)
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

    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let configs = sqlx::query_as::<_, WebhookConfig>(
        "SELECT * FROM webhook_configs WHERE tenant_id = $1 ORDER BY event_type",
    )
    .bind(tenant_id)
    .fetch_all(&mut *conn)
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

    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let config = sqlx::query_as::<_, WebhookConfig>(
        "SELECT * FROM webhook_configs WHERE id = $1 AND tenant_id = $2",
    )
    .bind(id)
    .bind(tenant_id)
    .fetch_optional(&mut *conn)
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

    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let result = sqlx::query("DELETE FROM webhook_configs WHERE id = $1 AND tenant_id = $2")
        .bind(id)
        .bind(tenant_id)
        .execute(&mut *conn)
        .await
        .map_err(|e| {
            tracing::error!("delete_webhook error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    if result.rows_affected() == 0 {
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

    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let deliveries = sqlx::query_as::<_, WebhookDelivery>(
        r#"
        SELECT * FROM webhook_deliveries
        WHERE config_id = $1 AND tenant_id = $2
        ORDER BY created_at DESC
        LIMIT 100
        "#,
    )
    .bind(id)
    .bind(tenant_id)
    .fetch_all(&mut *conn)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(deliveries))
}
