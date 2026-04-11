use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::post,
    Json, Router,
};
use uuid::Uuid;

use crate::TroubleState;
use alc_core::auth_middleware::TenantId;
use alc_core::models::{TroubleNotificationPref, UpsertNotificationPref};

const VALID_EVENT_TYPES: &[&str] = &[
    "trouble_created",
    "trouble_status_changed",
    "trouble_comment_added",
    "trouble_assigned",
];

const VALID_CHANNELS: &[&str] = &["lineworks"];

pub fn tenant_router<S>() -> Router<S>
where
    TroubleState: axum::extract::FromRef<S>,
    S: Clone + Send + Sync + 'static,
{
    Router::new()
        .route(
            "/trouble/notification-prefs",
            post(upsert_pref).get(list_prefs),
        )
        .route(
            "/trouble/notification-prefs/{id}",
            axum::routing::delete(delete_pref),
        )
}

async fn upsert_pref(
    State(state): State<TroubleState>,
    tenant: axum::Extension<TenantId>,
    Json(body): Json<UpsertNotificationPref>,
) -> Result<(StatusCode, Json<TroubleNotificationPref>), StatusCode> {
    if !VALID_EVENT_TYPES.contains(&body.event_type.as_str()) {
        return Err(StatusCode::BAD_REQUEST);
    }
    if !VALID_CHANNELS.contains(&body.notify_channel.as_str()) {
        return Err(StatusCode::BAD_REQUEST);
    }

    let pref = state
        .trouble_notification_prefs
        .upsert(tenant.0 .0, &body)
        .await
        .map_err(|e| {
            tracing::error!("upsert_pref error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok((StatusCode::CREATED, Json(pref)))
}

async fn list_prefs(
    State(state): State<TroubleState>,
    tenant: axum::Extension<TenantId>,
) -> Result<Json<Vec<TroubleNotificationPref>>, StatusCode> {
    let prefs = state
        .trouble_notification_prefs
        .list(tenant.0 .0)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(prefs))
}

async fn delete_pref(
    State(state): State<TroubleState>,
    tenant: axum::Extension<TenantId>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let deleted = state
        .trouble_notification_prefs
        .delete(tenant.0 .0, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}
