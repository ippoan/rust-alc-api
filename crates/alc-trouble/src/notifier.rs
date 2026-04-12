use std::sync::Arc;

use async_trait::async_trait;
use uuid::Uuid;

use alc_core::auth_lineworks::decrypt_secret;
use alc_core::repository::bot_admin::BotAdminRepository;
use alc_notify::clients::lineworks::{LineworksBotClient, LineworksBotConfig};

/// テスト可能なトラブル通知サービス
#[async_trait]
pub trait TroubleNotifier: Send + Sync {
    async fn notify(
        &self,
        tenant_id: Uuid,
        event_type: &str,
        message: &str,
        lineworks_user_ids: &[String],
    );
}

fn encryption_key() -> Result<String, String> {
    std::env::var("SSO_ENCRYPTION_KEY")
        .or_else(|_| std::env::var("JWT_SECRET"))
        .map_err(|_| "SSO_ENCRYPTION_KEY or JWT_SECRET not set".to_string())
}

pub async fn resolve_lineworks_config(
    bot_admin: &dyn BotAdminRepository,
    tenant_id: Uuid,
) -> Result<(Uuid, LineworksBotConfig), String> {
    let configs = bot_admin
        .list_configs(tenant_id)
        .await
        .map_err(|e| format!("DB error: {e}"))?;

    let bot_cfg = configs
        .iter()
        .find(|c| c.provider == "lineworks" && c.enabled)
        .ok_or_else(|| "No LINE WORKS bot config".to_string())?;

    let full = bot_admin
        .get_config_with_secrets(tenant_id, bot_cfg.id)
        .await
        .map_err(|e| format!("DB error: {e}"))?
        .ok_or_else(|| "Bot config not found".to_string())?;

    let key = encryption_key()?;
    let client_secret = decrypt_secret(&full.client_secret_encrypted, &key)
        .map_err(|e| format!("decrypt client_secret: {e}"))?;
    let private_key = decrypt_secret(&full.private_key_encrypted, &key)
        .map_err(|e| format!("decrypt private_key: {e}"))?;

    Ok((
        full.id,
        LineworksBotConfig {
            client_id: full.client_id,
            client_secret,
            service_account: full.service_account,
            private_key,
            bot_id: full.bot_id,
        },
    ))
}

/// LINE WORKS Bot API 経由でトラブル通知を送信
pub struct LineworksTroubleNotifier {
    bot_admin: Arc<dyn BotAdminRepository>,
    lw_client: Arc<LineworksBotClient>,
}

impl LineworksTroubleNotifier {
    pub fn new(bot_admin: Arc<dyn BotAdminRepository>, lw_client: Arc<LineworksBotClient>) -> Self {
        Self {
            bot_admin,
            lw_client,
        }
    }
}

#[async_trait]
impl TroubleNotifier for LineworksTroubleNotifier {
    async fn notify(
        &self,
        tenant_id: Uuid,
        event_type: &str,
        message: &str,
        lineworks_user_ids: &[String],
    ) {
        if lineworks_user_ids.is_empty() {
            return;
        }

        let (config_id, config) =
            match resolve_lineworks_config(self.bot_admin.as_ref(), tenant_id).await {
                Ok(v) => v,
                Err(e) => {
                    tracing::error!(
                        tenant_id = %tenant_id,
                        event_type = %event_type,
                        "resolve_lineworks_config failed: {e}"
                    );
                    return;
                }
            };

        for user_id in lineworks_user_ids {
            if let Err(e) = self
                .lw_client
                .send_text_to_user(config_id, &config, user_id, message)
                .await
            {
                tracing::error!(
                    tenant_id = %tenant_id,
                    event_type = %event_type,
                    user_id = %user_id,
                    "LINE WORKS send failed: {e}"
                );
            }
        }
    }
}
