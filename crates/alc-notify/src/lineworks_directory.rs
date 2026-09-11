//! LINE WORKS Directory API proxy: list organization members.
//!
//! Uses the tenant's existing LINE WORKS Bot config (from `bot_configs` table)
//! with `directory.read` scope to fetch the member list.
//! A recipient flag (`already_registered`) is set for members that already
//! exist in `notify_recipients` with matching `lineworks_user_id`.

use axum::{extract::State, http::StatusCode, Extension, Json, Router};
use serde::Serialize;
use std::collections::HashSet;

use alc_core::auth_middleware::TenantId;
use alc_core::AppState;

use crate::clients::lineworks::LineworksBotClient;
use crate::lineworks_config::{internal_error, resolve_lineworks_config};

pub fn tenant_router() -> Router<AppState> {
    Router::new().route("/notify/lineworks/users", axum::routing::get(list_users))
}

#[derive(Debug, Serialize)]
pub struct DirectoryUser {
    pub user_id: String,
    pub user_name: Option<String>,
    pub email: Option<String>,
    pub already_registered: bool,
}

async fn list_users(
    State(state): State<AppState>,
    Extension(tenant): Extension<TenantId>,
) -> Result<Json<Vec<DirectoryUser>>, (StatusCode, Json<serde_json::Value>)> {
    let (config_id, config) = resolve_lineworks_config(&state, tenant.0).await?;

    // Fetch LINE WORKS members.
    let client = LineworksBotClient::new();
    let members = match client.list_org_users(config_id, &config).await {
        Ok(m) => m,
        Err(e) => {
            let msg = e.to_string();
            tracing::error!("LINE WORKS list_org_users: {msg}");
            // 403 in the upstream response often maps to SendFailed("403: ...")
            if msg.contains("403") || msg.to_lowercase().contains("forbidden") {
                return Err((
                    StatusCode::FORBIDDEN,
                    Json(serde_json::json!({
                        "error": "missing_scope",
                        "scope": "directory.read",
                        "message": "LINE WORKS Developer Console で directory.read scope を追加してください",
                    })),
                ));
            }
            return Err((
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({
                    "error": "upstream_error",
                    "message": msg,
                })),
            ));
        }
    };

    // Build set of already-registered lineworks_user_ids.
    let existing = state.notify_recipients.list(tenant.0).await.map_err(|e| {
        tracing::error!("list notify_recipients: {e}");
        internal_error("list_recipients_failed")
    })?;
    let registered: HashSet<String> = existing
        .into_iter()
        .filter_map(|r| r.lineworks_user_id)
        .collect();

    let users = members
        .into_iter()
        .map(|m| DirectoryUser {
            already_registered: registered.contains(&m.user_id),
            user_id: m.user_id,
            user_name: m.user_name,
            email: m.email,
        })
        .collect();

    Ok(Json(users))
}
