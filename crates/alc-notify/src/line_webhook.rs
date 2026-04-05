//! LINE Bot webhook handler
//! follow イベントで user_id を自動登録

use axum::{
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode},
    Router,
};
use hmac::{Hmac, Mac};
use sha2::Sha256;

use alc_core::auth_lineworks::decrypt_secret;
use alc_core::AppState;

use crate::clients::line::{LineClient, LineConfig};

pub fn public_router() -> Router<AppState> {
    Router::new().route("/notify/line/webhook", axum::routing::post(handle_webhook))
}

#[derive(serde::Deserialize)]
struct WebhookBody {
    #[allow(dead_code)]
    destination: Option<String>,
    events: Vec<WebhookEvent>,
}

#[derive(serde::Deserialize)]
struct WebhookEvent {
    #[serde(rename = "type")]
    event_type: String,
    source: Option<EventSource>,
    #[serde(rename = "replyToken")]
    #[allow(dead_code)]
    reply_token: Option<String>,
}

#[derive(serde::Deserialize)]
struct EventSource {
    #[serde(rename = "type")]
    #[allow(dead_code)]
    source_type: String,
    #[serde(rename = "userId")]
    user_id: Option<String>,
}

async fn handle_webhook(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<StatusCode, StatusCode> {
    let signature = headers
        .get("x-line-signature")
        .and_then(|v| v.to_str().ok())
        .ok_or(StatusCode::BAD_REQUEST)?;

    let webhook_body: WebhookBody =
        serde_json::from_slice(&body).map_err(|_| StatusCode::BAD_REQUEST)?;

    let resolved = find_config_by_signature(&state, &body, signature).await?;

    // JWT 方式でアクセストークンを取得
    let line_client = LineClient::new();
    let line_config = resolved.to_line_config().map_err(|e| {
        tracing::error!("resolve LINE config: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    for event in &webhook_body.events {
        if event.event_type == "follow" {
            if let Some(source) = &event.source {
                if let Some(user_id) = &source.user_id {
                    tracing::info!(
                        "LINE follow event: user_id={}, tenant_id={}",
                        user_id,
                        resolved.tenant_id
                    );

                    // JWT でアクセストークン取得 → プロフィール取得
                    let name = match line_client.get_access_token(&line_config).await {
                        Ok(token) => get_user_display_name(&token, user_id)
                            .await
                            .unwrap_or_else(|_| format!("LINE User {}", &user_id[..8])),
                        Err(e) => {
                            tracing::warn!("LINE token error, using default name: {e}");
                            format!("LINE User {}", &user_id[..8])
                        }
                    };

                    if let Err(e) = state
                        .notify_recipients
                        .upsert_by_line_user_id(resolved.tenant_id, user_id, &name)
                        .await
                    {
                        tracing::error!("upsert LINE recipient failed: {e}");
                    }
                }
            }
        }
    }

    Ok(StatusCode::OK)
}

struct ResolvedConfig {
    tenant_id: uuid::Uuid,
    channel_id: String,
    channel_secret: String,
    key_id: Option<String>,
    private_key: Option<String>,
}

impl ResolvedConfig {
    fn to_line_config(&self) -> Result<LineConfig, String> {
        Ok(LineConfig {
            channel_id: self.channel_id.clone(),
            channel_secret: self.channel_secret.clone(),
            key_id: self.key_id.clone().ok_or("missing key_id")?,
            private_key: self.private_key.clone().ok_or("missing private_key")?,
        })
    }
}

/// 署名検証で正しい config を特定
async fn find_config_by_signature(
    state: &AppState,
    body: &[u8],
    signature: &str,
) -> Result<ResolvedConfig, StatusCode> {
    let pool = state.pool();

    #[derive(sqlx::FromRow)]
    struct ConfigRow {
        tenant_id: uuid::Uuid,
        channel_id: String,
        channel_secret_encrypted: String,
        key_id: Option<String>,
        private_key_encrypted: Option<String>,
    }

    let configs: Vec<ConfigRow> = sqlx::query_as(
        "SELECT tenant_id, channel_id, channel_secret_encrypted, key_id, private_key_encrypted FROM alc_api.notify_line_configs WHERE enabled = TRUE",
    )
    .fetch_all(pool)
    .await
    .map_err(|e| {
        tracing::error!("fetch line configs: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let enc_key = std::env::var("SSO_ENCRYPTION_KEY")
        .or_else(|_| std::env::var("JWT_SECRET"))
        .map_err(|_| {
            tracing::error!("SSO_ENCRYPTION_KEY or JWT_SECRET not set");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    for cfg in &configs {
        let Ok(channel_secret) = decrypt_secret(&cfg.channel_secret_encrypted, &enc_key) else {
            continue;
        };
        if verify_signature(body, &channel_secret, signature) {
            let private_key = cfg
                .private_key_encrypted
                .as_deref()
                .map(|pk| decrypt_secret(pk, &enc_key))
                .transpose()
                .map_err(|e| {
                    tracing::error!("decrypt private_key: {e}");
                    StatusCode::INTERNAL_SERVER_ERROR
                })?;

            return Ok(ResolvedConfig {
                tenant_id: cfg.tenant_id,
                channel_id: cfg.channel_id.clone(),
                channel_secret,
                key_id: cfg.key_id.clone(),
                private_key,
            });
        }
    }

    tracing::warn!("No matching LINE config found for webhook signature");
    Err(StatusCode::UNAUTHORIZED)
}

fn verify_signature(body: &[u8], channel_secret: &str, signature: &str) -> bool {
    let Ok(mut mac) = Hmac::<Sha256>::new_from_slice(channel_secret.as_bytes()) else {
        return false;
    };
    mac.update(body);
    let expected = base64::engine::general_purpose::STANDARD.encode(mac.finalize().into_bytes());
    expected == signature
}

use base64::Engine;

async fn get_user_display_name(
    access_token: &str,
    user_id: &str,
) -> Result<String, reqwest::Error> {
    #[derive(serde::Deserialize)]
    struct Profile {
        #[serde(rename = "displayName")]
        display_name: String,
    }

    let url = format!("https://api.line.me/v2/bot/profile/{}", user_id);
    let profile: Profile = reqwest::Client::new()
        .get(&url)
        .header("Authorization", format!("Bearer {}", access_token))
        .send()
        .await?
        .json()
        .await?;

    Ok(profile.display_name)
}
