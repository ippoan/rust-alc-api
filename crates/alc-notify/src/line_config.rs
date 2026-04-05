use axum::{extract::State, http::StatusCode, Extension, Json, Router};

use alc_core::auth_lineworks::encrypt_secret;
use alc_core::auth_middleware::TenantId;
use alc_core::repository::notify_line_config::UpsertLineConfig;
use alc_core::AppState;

pub fn tenant_router() -> Router<AppState> {
    Router::new()
        .route("/notify/line-config", axum::routing::get(get_config))
        .route("/notify/line-config", axum::routing::post(upsert_config))
        .route("/notify/line-config", axum::routing::delete(delete_config))
}

async fn get_config(
    State(state): State<AppState>,
    Extension(tenant): Extension<TenantId>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    match state.notify_line_config.get(tenant.0).await {
        Ok(Some(config)) => Ok(Json(serde_json::to_value(config).unwrap())),
        Ok(None) => Ok(Json(serde_json::json!(null))),
        Err(e) => {
            tracing::error!("get line config: {e}");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn upsert_config(
    State(state): State<AppState>,
    Extension(tenant): Extension<TenantId>,
    Json(input): Json<UpsertLineConfig>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let key = std::env::var("SSO_ENCRYPTION_KEY")
        .or_else(|_| std::env::var("JWT_SECRET"))
        .map_err(|_| {
            tracing::error!("SSO_ENCRYPTION_KEY or JWT_SECRET not set");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let channel_secret_encrypted = encrypt_secret(&input.channel_secret, &key).map_err(|e| {
        tracing::error!("encrypt channel_secret: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let private_key_encrypted = encrypt_secret(&input.private_key, &key).map_err(|e| {
        tracing::error!("encrypt private_key: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let config = state
        .notify_line_config
        .upsert(
            tenant.0,
            &input.name,
            &input.channel_id,
            &channel_secret_encrypted,
            &input.key_id,
            &private_key_encrypted,
            input.bot_basic_id.as_deref(),
            input.public_key_jwk.as_deref(),
        )
        .await
        .map_err(|e| {
            tracing::error!("upsert line config: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(serde_json::to_value(config).unwrap()))
}

async fn delete_config(
    State(state): State<AppState>,
    Extension(tenant): Extension<TenantId>,
) -> Result<StatusCode, StatusCode> {
    state
        .notify_line_config
        .delete(tenant.0)
        .await
        .map_err(|e| {
            tracing::error!("delete line config: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(StatusCode::NO_CONTENT)
}
