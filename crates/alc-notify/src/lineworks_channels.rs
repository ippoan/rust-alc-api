//! LINE WORKS Bot のチャネル (トークルーム) の登録。
//!
//! Bot 公式 API には「既存トークルームに Bot を追加する」エンドポイントが無いため、
//! ユーザーが LINE WORKS アプリ上で手動で Bot を招待 → join webhook で channel_id を保存
//! という運用にしている。本モジュールはその webhook の受け口だけを担う。
//! 登録済み channel の一覧・削除・試し送信と `/internal/lineworks/send` は Worker
//! (alc-notify / alc-lineworks) に移った (Refs #747)。

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};

use alc_core::api_error::{internal_error_msg, ApiError};
use alc_core::AppState;

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
) -> Result<Json<BotSecretEncryptedResponse>, ApiError> {
    let cfg = state
        .lineworks_channels
        .lookup_bot_config_for_webhook(&bot_id)
        .await
        .map_err(|e| {
            tracing::error!("lookup_bot_config_for_webhook (internal): {e}");
            internal_error_msg("lookup_failed")
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
            internal_error_msg("lookup_failed")
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
                    internal_error_msg("upsert_failed")
                })?;
        }
        "leave" | "left" => {
            state
                .lineworks_channels
                .mark_left(cfg.tenant_id, cfg.id, &channel_id)
                .await
                .map_err(|e| {
                    tracing::error!("mark_left (internal): {e}");
                    internal_error_msg("mark_left_failed")
                })?;
        }
        _ => {}
    }

    Ok(Json(WebhookResponse { ok: true }))
}
