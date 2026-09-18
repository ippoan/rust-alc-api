//! LINE WORKS bot config (`bot_configs` テーブル) の共通解決ヘルパー。
//!
//! `lineworks_directory.rs` (`GET /notify/lineworks/users`) と
//! `lineworks_login_activity.rs` (`GET /notify/lineworks/login-activity`, Refs #540) の
//! 両方が「テナントの有効な LINE WORKS bot config を取得し秘密情報を復号する」という
//! 同一のロジックを必要とするため、3 箇所目のコピーを増やさずここへ切り出した。
//!
//! `distribute.rs::resolve_lineworks_config` も同じロジックの別実装を持つが、
//! 戻り値のエラー型が `String` (呼び出し側で任意に変換) であり axum レスポンス型を
//! 直接返すこの2箇所とは形が異なるため、本 PR の範囲では統合しない。

use axum::http::StatusCode;
use uuid::Uuid;

use alc_core::api_error::{internal_error_msg, ApiError};
use alc_core::auth_lineworks::{decrypt_pem_secret, decrypt_secret};
use alc_core::AppState;

use crate::clients::lineworks::LineworksBotConfig;

pub(crate) fn encryption_key() -> Result<String, ApiError> {
    std::env::var("SSO_ENCRYPTION_KEY").map_err(|_| {
        tracing::error!("SSO_ENCRYPTION_KEY not set");
        internal_error_msg("encryption_key_missing")
    })
}

/// テナントの有効な LINE WORKS bot config を取得し、秘密情報を復号する。
pub(crate) async fn resolve_lineworks_config(
    state: &AppState,
    tenant_id: Uuid,
) -> Result<(Uuid, LineworksBotConfig), ApiError> {
    let configs = state.bot_admin.list_configs(tenant_id).await.map_err(|e| {
        tracing::error!("list bot_configs: {e}");
        internal_error_msg("list_bot_configs_failed")
    })?;
    let bot_cfg = configs
        .iter()
        .find(|c| c.provider == "lineworks" && c.enabled)
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                axum::Json(serde_json::json!({
                    "error": "no_lineworks_config",
                    "message": "LINE WORKS bot config not found for this tenant",
                })),
            )
        })?;

    let full = state
        .bot_admin
        .get_config_with_secrets(tenant_id, bot_cfg.id)
        .await
        .map_err(|e| {
            tracing::error!("get_config_with_secrets: {e}");
            internal_error_msg("get_bot_config_failed")
        })?
        .ok_or_else(|| internal_error_msg("bot_config_not_found"))?;

    let key = encryption_key()?;
    let client_secret = decrypt_secret(&full.client_secret_encrypted, &key).map_err(|e| {
        tracing::error!("decrypt client_secret: {e}");
        internal_error_msg("decrypt_failed")
    })?;
    let private_key = decrypt_pem_secret(&full.private_key_encrypted, &key).map_err(|e| {
        tracing::error!("decrypt private_key: {e}");
        internal_error_msg("decrypt_failed")
    })?;

    Ok((
        full.id,
        LineworksBotConfig {
            client_id: full.client_id.clone(),
            client_secret,
            service_account: full.service_account.clone(),
            private_key,
            bot_id: full.bot_id.clone(),
        },
    ))
}
