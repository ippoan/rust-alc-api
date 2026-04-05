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

use alc_core::AppState;

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
    // X-Line-Signature ヘッダー取得
    let signature = headers
        .get("x-line-signature")
        .and_then(|v| v.to_str().ok())
        .ok_or(StatusCode::BAD_REQUEST)?;

    // body をパース
    let webhook_body: WebhookBody =
        serde_json::from_slice(&body).map_err(|_| StatusCode::BAD_REQUEST)?;

    // destination (Bot の user ID) から channel_id を特定して config を取得
    // LINE webhook の destination は Bot userId だが、lookup は channel_id ベース
    // 全テナントの LINE config を探す必要がある → destination は使わず、
    // 署名検証で正しい config を特定する
    let config = find_config_by_signature(&state, &body, signature).await?;

    // follow イベントを処理
    for event in &webhook_body.events {
        if event.event_type == "follow" {
            if let Some(source) = &event.source {
                if let Some(user_id) = &source.user_id {
                    tracing::info!(
                        "LINE follow event: user_id={}, tenant_id={}",
                        user_id,
                        config.tenant_id
                    );

                    // LINE ユーザープロフィールを取得して名前を使う
                    let name = get_user_display_name(&config.channel_access_token, user_id)
                        .await
                        .unwrap_or_else(|_| format!("LINE User {}", &user_id[..8]));

                    if let Err(e) = state
                        .notify_recipients
                        .upsert_by_line_user_id(config.tenant_id, user_id, &name)
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
    channel_access_token: String,
}

/// 署名検証で正しい config を特定
async fn find_config_by_signature(
    state: &AppState,
    body: &[u8],
    signature: &str,
) -> Result<ResolvedConfig, StatusCode> {
    // 現状はテナント数が少ないので、pool から全 config を取得して検証
    // 将来的にはキャッシュや destination マッピングで最適化
    let pool = state.pool();
    let configs: Vec<(uuid::Uuid, String, String)> = sqlx::query_as(
        "SELECT tenant_id, channel_secret_encrypted, channel_access_token_encrypted FROM alc_api.notify_line_configs WHERE enabled = TRUE",
    )
    .fetch_all(pool)
    .await
    .map_err(|e| {
        tracing::error!("fetch line configs: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let key = std::env::var("SSO_ENCRYPTION_KEY")
        .or_else(|_| std::env::var("JWT_SECRET"))
        .map_err(|_| {
            tracing::error!("SSO_ENCRYPTION_KEY or JWT_SECRET not set");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    for (tenant_id, channel_secret_enc, channel_access_token_enc) in &configs {
        let Ok(channel_secret) = alc_core::auth_lineworks::decrypt_secret(channel_secret_enc, &key)
        else {
            continue;
        };
        if verify_signature(body, &channel_secret, signature) {
            let channel_access_token =
                alc_core::auth_lineworks::decrypt_secret(channel_access_token_enc, &key).map_err(
                    |e| {
                        tracing::error!("decrypt access_token: {e}");
                        StatusCode::INTERNAL_SERVER_ERROR
                    },
                )?;
            return Ok(ResolvedConfig {
                tenant_id: *tenant_id,
                channel_access_token,
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
