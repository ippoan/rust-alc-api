//! LINE WORKS Directory API proxy: list organization members.
//!
//! Uses the tenant's existing LINE WORKS Bot config (from `bot_configs` table)
//! with `directory.read` scope to fetch the member list.
//! A recipient flag (`already_registered`) is set for members that already
//! exist in `notify_recipients` with matching `lineworks_user_id`.

use axum::{extract::State, http::StatusCode, Extension, Json, Router};
use serde::Serialize;
use std::collections::HashSet;

use alc_core::auth_lineworks::{decrypt_pem_secret, decrypt_secret};
use alc_core::auth_middleware::TenantId;
use alc_core::AppState;

use crate::clients::lineworks::{LineworksBotClient, LineworksBotConfig};

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

fn encryption_key() -> Result<String, (StatusCode, Json<serde_json::Value>)> {
    std::env::var("SSO_ENCRYPTION_KEY").map_err(|_| {
        tracing::error!("SSO_ENCRYPTION_KEY not set");
        internal_error("encryption_key_missing")
    })
}

fn internal_error(msg: &str) -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({
            "error": "internal_error",
            "message": msg,
        })),
    )
}

async fn list_users(
    State(state): State<AppState>,
    Extension(tenant): Extension<TenantId>,
) -> Result<Json<Vec<DirectoryUser>>, (StatusCode, Json<serde_json::Value>)> {
    // Resolve the tenant's LINE WORKS bot config.
    let configs = state.bot_admin.list_configs(tenant.0).await.map_err(|e| {
        tracing::error!("list bot_configs: {e}");
        internal_error("list_bot_configs_failed")
    })?;
    let bot_cfg = configs
        .iter()
        .find(|c| c.provider == "lineworks" && c.enabled)
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({
                    "error": "no_lineworks_config",
                    "message": "LINE WORKS bot config not found for this tenant",
                })),
            )
        })?;

    let full = state
        .bot_admin
        .get_config_with_secrets(tenant.0, bot_cfg.id)
        .await
        .map_err(|e| {
            tracing::error!("get_config_with_secrets: {e}");
            internal_error("get_bot_config_failed")
        })?
        .ok_or_else(|| internal_error("bot_config_not_found"))?;

    let key = encryption_key()?;
    let client_secret = decrypt_secret(&full.client_secret_encrypted, &key).map_err(|e| {
        tracing::error!("decrypt client_secret: {e}");
        internal_error("decrypt_failed")
    })?;
    let private_key = decrypt_pem_secret(&full.private_key_encrypted, &key).map_err(|e| {
        tracing::error!("decrypt private_key: {e}");
        internal_error("decrypt_failed")
    })?;

    let config = LineworksBotConfig {
        client_id: full.client_id.clone(),
        client_secret,
        service_account: full.service_account.clone(),
        private_key,
        bot_id: full.bot_id.clone(),
    };

    // Fetch LINE WORKS members.
    let client = LineworksBotClient::new();
    let members = match client.list_org_users(full.id, &config).await {
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
