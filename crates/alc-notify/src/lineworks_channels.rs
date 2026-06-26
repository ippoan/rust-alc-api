//! LINE WORKS Bot のチャネル/グループ管理。
//!
//! Bot 公式 API には「既存トークルームに Bot を追加する」エンドポイントが無いため、
//! ユーザーが LINE WORKS アプリ上で手動で Bot を招待 → join webhook で channel_id を保存
//! という運用にしている。本モジュールはその webhook と、登録済み channel の CRUD を担う。

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get, post},
    Extension, Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use alc_core::auth_lineworks::decrypt_secret;
use alc_core::auth_middleware::TenantId;
use alc_core::repository::lineworks_channels::LineworksChannel;
use alc_core::AppState;

use crate::clients::lineworks::{LineworksBotClient, LineworksBotConfig};

/// Admin (require_tenant_header) ルート群。
pub fn tenant_router() -> Router<AppState> {
    Router::new()
        .route("/notify/lineworks/channels", get(list_channels))
        .route("/notify/lineworks/channels/{id}", delete(delete_channel))
        .route(
            "/notify/lineworks/channels/{id}/test-send",
            post(test_send_channel),
        )
}

/// Internal (auth-worker 専用) ルート群。`require_internal_jwt` 配下に nest される想定。
///
/// auth-worker (Cloudflare Workers) が LINE WORKS webhook を edge で受け、
/// HMAC 検証 + 復号 + イベント抽出を済ませた後、本ルートに転送する。
///
/// - `GET  /api/internal/lineworks/bot-secret/{bot_id}` — bot_secret_encrypted を返す (復号は auth-worker)
/// - `POST /api/internal/lineworks/event` — 検証済みイベントを受け取って upsert/mark_left
pub fn internal_router() -> Router<AppState> {
    Router::new()
        .route(
            "/internal/lineworks/bot-secret/{bot_id}",
            get(get_bot_secret_internal),
        )
        .route("/internal/lineworks/event", post(receive_event_internal))
}

fn internal_error(msg: &str) -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({"error": "internal_error", "message": msg})),
    )
}

fn encryption_key() -> Result<String, (StatusCode, Json<serde_json::Value>)> {
    std::env::var("SSO_ENCRYPTION_KEY")
        .or_else(|_| std::env::var("JWT_SECRET"))
        .map_err(|_| {
            tracing::error!("SSO_ENCRYPTION_KEY or JWT_SECRET not set");
            internal_error("encryption_key_missing")
        })
}

// ---------- admin: list ----------

async fn list_channels(
    State(state): State<AppState>,
    Extension(tenant): Extension<TenantId>,
) -> Result<Json<Vec<LineworksChannel>>, (StatusCode, Json<serde_json::Value>)> {
    let rows = state
        .lineworks_channels
        .list_active(tenant.0)
        .await
        .map_err(|e| {
            tracing::error!("list_active lineworks_channels: {e}");
            internal_error("list_failed")
        })?;
    Ok(Json(rows))
}

// ---------- admin: delete ----------

async fn delete_channel(
    State(state): State<AppState>,
    Extension(tenant): Extension<TenantId>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    state
        .lineworks_channels
        .delete(tenant.0, id)
        .await
        .map_err(|e| {
            tracing::error!("delete lineworks_channel: {e}");
            internal_error("delete_failed")
        })?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------- admin: test-send ----------

#[derive(Debug, Deserialize)]
pub struct TestSendBody {
    pub text: String,
}

async fn test_send_channel(
    State(state): State<AppState>,
    Extension(tenant): Extension<TenantId>,
    Path(id): Path<Uuid>,
    Json(body): Json<TestSendBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let row = state
        .lineworks_channels
        .get(tenant.0, id)
        .await
        .map_err(|e| {
            tracing::error!("get lineworks_channel: {e}");
            internal_error("get_failed")
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "channel_not_found"})),
            )
        })?;

    let full = state
        .bot_admin
        .get_config_with_secrets(tenant.0, row.bot_config_id)
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
    let private_key =
        alc_core::auth_lineworks::decrypt_pem_secret(&full.private_key_encrypted, &key).map_err(
            |e| {
                tracing::error!("decrypt private_key: {e}");
                internal_error("decrypt_failed")
            },
        )?;

    let config = LineworksBotConfig {
        client_id: full.client_id.clone(),
        client_secret,
        service_account: full.service_account.clone(),
        private_key,
        bot_id: full.bot_id.clone(),
    };

    let client = LineworksBotClient::new();
    client
        .send_text_to_channel(full.id, &config, &row.channel_id, &body.text)
        .await
        .map_err(|e| {
            tracing::error!("send_text_to_channel: {e}");
            (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({"error": "upstream_error", "message": e.to_string()})),
            )
        })?;

    Ok(Json(serde_json::json!({"ok": true})))
}

// ---------- shared response shape ----------

#[derive(Debug, Serialize)]
pub struct WebhookResponse {
    pub ok: bool,
}

// ---------- internal: GET bot-secret ----------

#[derive(Debug, Serialize)]
pub struct BotSecretEncryptedResponse {
    pub bot_secret_encrypted: String,
}

async fn get_bot_secret_internal(
    State(state): State<AppState>,
    Path(bot_id): Path<String>,
) -> Result<Json<BotSecretEncryptedResponse>, (StatusCode, Json<serde_json::Value>)> {
    let cfg = state
        .lineworks_channels
        .lookup_bot_config_for_webhook(&bot_id)
        .await
        .map_err(|e| {
            tracing::error!("lookup_bot_config_for_webhook (internal): {e}");
            internal_error("lookup_failed")
        })?
        .ok_or((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "bot_not_found"})),
        ))?;

    let bot_secret_encrypted = cfg.bot_secret_encrypted.ok_or((
        StatusCode::NOT_FOUND,
        Json(serde_json::json!({"error": "bot_secret_not_configured"})),
    ))?;

    Ok(Json(BotSecretEncryptedResponse {
        bot_secret_encrypted,
    }))
}

// ---------- internal: POST event ----------

#[derive(Debug, Deserialize)]
pub struct InternalEventBody {
    pub bot_id: String,
    pub event_type: String,
    pub channel_id: Option<String>,
    pub channel_type: Option<String>,
    pub title: Option<String>,
}

async fn receive_event_internal(
    State(state): State<AppState>,
    Json(body): Json<InternalEventBody>,
) -> Result<Json<WebhookResponse>, (StatusCode, Json<serde_json::Value>)> {
    process_internal_event(&state, body).await
}

/// Public testable core。`receive_event_internal` から委譲される。
pub async fn process_internal_event(
    state: &AppState,
    body: InternalEventBody,
) -> Result<Json<WebhookResponse>, (StatusCode, Json<serde_json::Value>)> {
    let cfg = state
        .lineworks_channels
        .lookup_bot_config_for_webhook(&body.bot_id)
        .await
        .map_err(|e| {
            tracing::error!("lookup_bot_config_for_webhook (event): {e}");
            internal_error("lookup_failed")
        })?
        .ok_or((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "bot_not_found"})),
        ))?;

    let channel_id = match body.channel_id {
        Some(c) => c,
        None => return Ok(Json(WebhookResponse { ok: true })),
    };

    match body.event_type.as_str() {
        "join" | "joined" => {
            state
                .lineworks_channels
                .upsert_joined(
                    cfg.tenant_id,
                    cfg.id,
                    &channel_id,
                    body.channel_type.as_deref(),
                    body.title.as_deref(),
                )
                .await
                .map_err(|e| {
                    tracing::error!("upsert_joined (internal): {e}");
                    internal_error("upsert_failed")
                })?;
        }
        "leave" | "left" => {
            state
                .lineworks_channels
                .mark_left(cfg.tenant_id, cfg.id, &channel_id)
                .await
                .map_err(|e| {
                    tracing::error!("mark_left (internal): {e}");
                    internal_error("mark_left_failed")
                })?;
        }
        _ => {}
    }

    Ok(Json(WebhookResponse { ok: true }))
}
