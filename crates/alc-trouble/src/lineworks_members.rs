use std::sync::Arc;

use axum::{extract::Extension, http::StatusCode, routing::get, Json, Router};

use alc_core::auth_middleware::TenantId;
use alc_core::repository::bot_admin::BotAdminRepository;
use alc_notify::clients::lineworks::{LineworksBotClient, LineworksMember};

use crate::notifier::resolve_lineworks_config;

/// lineworks/members ルーター (tenant_protected 内に merge)
/// BotAdminRepository と LineworksBotClient は Extension で注入
pub fn tenant_router<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    Router::new().route("/trouble/lineworks/members", get(list_members))
}

async fn list_members(
    tenant: Extension<TenantId>,
    bot_admin: Extension<Arc<dyn BotAdminRepository>>,
    lw_client: Extension<Arc<LineworksBotClient>>,
) -> Result<Json<Vec<LineworksMember>>, StatusCode> {
    let tenant_id = tenant.0 .0;

    let (config_id, config) = resolve_lineworks_config(bot_admin.as_ref(), tenant_id)
        .await
        .map_err(|e| {
            tracing::warn!("resolve_lineworks_config: {e}");
            StatusCode::SERVICE_UNAVAILABLE
        })?;

    let members = lw_client
        .list_org_users(config_id, &config)
        .await
        .map_err(|e| {
            tracing::error!("list_org_users: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(members))
}
