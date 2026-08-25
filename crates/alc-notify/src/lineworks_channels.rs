//! LINE WORKS Bot のチャネル/グループ管理。
//!
//! Bot 公式 API には「既存トークルームに Bot を追加する」エンドポイントが無いため、
//! ユーザーが LINE WORKS アプリ上で手動で Bot を招待 → join webhook で channel_id を保存
//! という運用にしている。本モジュールはその webhook と、登録済み channel の CRUD を担う。

use std::sync::Arc;

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
use alc_core::repository::bot_admin::BotAdminRepository;
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
/// - `POST /api/internal/lineworks/send` — 登録済み channel へテキスト送信 (無人 worker 用)
pub fn internal_router() -> Router<AppState> {
    Router::new()
        .route(
            "/internal/lineworks/bot-secret/{bot_id}",
            get(get_bot_secret_internal),
        )
        .route("/internal/lineworks/event", post(receive_event_internal))
        .route("/internal/lineworks/send", post(send_text_internal))
}

fn internal_error(msg: &str) -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({"error": "internal_error", "message": msg})),
    )
}

fn encryption_key() -> Result<String, (StatusCode, Json<serde_json::Value>)> {
    std::env::var("SSO_ENCRYPTION_KEY").map_err(|_| {
        tracing::error!("SSO_ENCRYPTION_KEY not set");
        internal_error("encryption_key_missing")
    })
}

/// ハンドラの共通エラー形 (`{"error": ..., "message": ...}`)
type ApiError = (StatusCode, Json<serde_json::Value>);

fn channel_not_found() -> ApiError {
    (
        StatusCode::NOT_FOUND,
        Json(serde_json::json!({"error": "channel_not_found"})),
    )
}

// ---------- shared: 復号 + 送信 ----------

/// channel 行が指す bot_config を復号して LINE WORKS へテキストを送る。
///
/// tenant 経路 (`test_send_channel`) と internal 経路 (`send_text_internal`) の
/// 共通部。`tenant_id` は呼び出し側が解決したもの — tenant 経路は
/// `X-Tenant-ID`、internal 経路は channel 行 (`row.tenant_id`) 由来で、
/// **この関数は header を一切見ない**。
async fn send_text_via_channel(
    bot_admin: &Arc<dyn BotAdminRepository>,
    lw_client: &LineworksBotClient,
    tenant_id: Uuid,
    row: &LineworksChannel,
    text: &str,
) -> Result<(), ApiError> {
    let full = bot_admin
        .get_config_with_secrets(tenant_id, row.bot_config_id)
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

    lw_client
        .send_text_to_channel(full.id, &config, &row.channel_id, text)
        .await
        .map_err(|e| {
            tracing::error!("send_text_to_channel: {e}");
            (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({"error": "upstream_error", "message": e.to_string()})),
            )
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
) -> Result<Json<serde_json::Value>, ApiError> {
    let row = state
        .lineworks_channels
        .get(tenant.0, id)
        .await
        .map_err(|e| {
            tracing::error!("get lineworks_channel: {e}");
            internal_error("get_failed")
        })?
        .ok_or_else(channel_not_found)?;

    send_text_via_channel(
        &state.bot_admin,
        &LineworksBotClient::new(),
        tenant.0,
        &row,
        &body.text,
    )
    .await?;

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

// ---------- internal: POST send ----------

/// `POST /api/internal/lineworks/send` の body。
///
/// `channel_id` は **`lineworks_channels` の行 id (Uuid)** で、LINE WORKS 側の
/// channel 文字列ではない (tenant 経路 `test-send` の `Path(id)` と同じもの)。
#[derive(Debug, Deserialize)]
pub struct InternalSendBody {
    pub channel_id: Uuid,
    pub text: String,
}

/// auth-worker 経由の無人送信 (dtako-scraper-relay の netprint cron 等)。
///
/// internal 経路は `X-Tenant-ID` を honor しない (shared secret だけで tenant を
/// 詐称できてしまうため — Refs #434)。tenant は channel 行の RLS バイパス取得
/// (`get_for_send`) から解決する。
async fn send_text_internal(
    State(state): State<AppState>,
    Json(body): Json<InternalSendBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if body.text.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "text_required", "message": "text is empty"})),
        ));
    }

    let row = state
        .lineworks_channels
        .get_for_send(body.channel_id)
        .await
        .map_err(|e| {
            tracing::error!("get_for_send lineworks_channel: {e}");
            internal_error("get_failed")
        })?
        .ok_or_else(channel_not_found)?;

    send_text_via_channel(
        &state.bot_admin,
        &LineworksBotClient::new(),
        row.tenant_id,
        &row,
        &body.text,
    )
    .await?;

    Ok(Json(serde_json::json!({"ok": true})))
}

#[cfg(test)]
mod tests {
    use super::*;
    use alc_core::auth_lineworks::encrypt_secret;
    use alc_core::repository::bot_admin::{
        BotConfigExportRow, BotConfigRow, BotConfigWithSecrets, TenantInfoForExport,
    };
    use rsa::pkcs1::EncodeRsaPrivateKey;
    use rsa::rand_core::OsRng;
    use rsa::RsaPrivateKey;
    use std::sync::{Mutex, OnceLock};
    use wiremock::matchers::{body_string_contains, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const ENC_KEY: &str = "test-sso-encryption-key";

    /// 本 crate で `SSO_ENCRYPTION_KEY` を読むのは handler だけで、テストは
    /// どれも同じ値を入れるだけなので直列化は不要 (unset するテストは置かない)。
    fn set_encryption_key() {
        std::env::set_var("SSO_ENCRYPTION_KEY", ENC_KEY);
    }

    /// RSA 鍵生成は 2048bit で数百 ms かかるのでテスト間で使い回す。
    fn test_private_pem() -> &'static str {
        static PEM: OnceLock<String> = OnceLock::new();
        PEM.get_or_init(|| {
            RsaPrivateKey::new(&mut OsRng, 2048)
                .unwrap()
                .to_pkcs1_pem(rsa::pkcs1::LineEnding::LF)
                .unwrap()
                .to_string()
        })
    }

    /// `get_config_with_secrets` の戻り値だけを差し替えられる最小 stub。
    /// 他メソッドは本 module から呼ばれないので `unimplemented!()`。
    struct StubBotAdmin {
        config: Mutex<Option<BotConfigWithSecrets>>,
        fail: bool,
    }

    impl StubBotAdmin {
        fn with_config(config: BotConfigWithSecrets) -> Arc<dyn BotAdminRepository> {
            Arc::new(Self {
                config: Mutex::new(Some(config)),
                fail: false,
            })
        }
        fn missing() -> Arc<dyn BotAdminRepository> {
            Arc::new(Self {
                config: Mutex::new(None),
                fail: false,
            })
        }
        fn failing() -> Arc<dyn BotAdminRepository> {
            Arc::new(Self {
                config: Mutex::new(None),
                fail: true,
            })
        }
    }

    #[async_trait::async_trait]
    impl BotAdminRepository for StubBotAdmin {
        async fn get_config_with_secrets(
            &self,
            _tenant_id: Uuid,
            _id: Uuid,
        ) -> Result<Option<BotConfigWithSecrets>, sqlx::Error> {
            if self.fail {
                return Err(sqlx::Error::RowNotFound);
            }
            Ok(self.config.lock().unwrap().take())
        }
        async fn list_configs(&self, _t: Uuid) -> Result<Vec<BotConfigRow>, sqlx::Error> {
            unimplemented!()
        }
        async fn update_client_secret(
            &self,
            _t: Uuid,
            _i: Uuid,
            _e: &str,
        ) -> Result<(), sqlx::Error> {
            unimplemented!()
        }
        async fn update_private_key(
            &self,
            _t: Uuid,
            _i: Uuid,
            _e: &str,
        ) -> Result<(), sqlx::Error> {
            unimplemented!()
        }
        async fn update_bot_secret(&self, _t: Uuid, _i: Uuid, _e: &str) -> Result<(), sqlx::Error> {
            unimplemented!()
        }
        async fn update_config(
            &self,
            _t: Uuid,
            _i: Uuid,
            _p: &str,
            _n: &str,
            _c: &str,
            _s: &str,
            _b: &str,
            _e: bool,
        ) -> Result<BotConfigRow, sqlx::Error> {
            unimplemented!()
        }
        async fn create_config(
            &self,
            _t: Uuid,
            _p: &str,
            _n: &str,
            _c: &str,
            _cs: &str,
            _s: &str,
            _pk: &str,
            _b: &str,
            _e: bool,
        ) -> Result<BotConfigRow, sqlx::Error> {
            unimplemented!()
        }
        async fn delete_config(&self, _t: Uuid, _i: Uuid) -> Result<(), sqlx::Error> {
            unimplemented!()
        }
        async fn get_tenant_for_export(
            &self,
            _t: Uuid,
        ) -> Result<Option<TenantInfoForExport>, sqlx::Error> {
            unimplemented!()
        }
        async fn list_configs_for_export(
            &self,
            _t: Uuid,
        ) -> Result<Vec<BotConfigExportRow>, sqlx::Error> {
            unimplemented!()
        }
    }

    fn sample_config() -> BotConfigWithSecrets {
        BotConfigWithSecrets {
            id: Uuid::new_v4(),
            provider: "lineworks".into(),
            name: "test bot".into(),
            client_id: "test-client-id".into(),
            client_secret_encrypted: encrypt_secret("test-client-secret", ENC_KEY).unwrap(),
            service_account: "sa@example.com".into(),
            private_key_encrypted: encrypt_secret(test_private_pem(), ENC_KEY).unwrap(),
            bot_id: "bot-1".into(),
            enabled: true,
            bot_secret_encrypted: None,
        }
    }

    fn sample_channel() -> LineworksChannel {
        LineworksChannel {
            id: Uuid::new_v4(),
            tenant_id: Uuid::new_v4(),
            bot_config_id: Uuid::new_v4(),
            channel_id: "ch-1".into(),
            title: Some("テスト".into()),
            channel_type: Some("group".into()),
            joined_at: chrono::Utc::now(),
            active: true,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }

    /// token 発行 mock + メッセージ送信 mock を立て、送信側の status を差し替える。
    async fn mock_lineworks(send_status: u16) -> (MockServer, LineworksBotClient) {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "tok-1",
                "refresh_token": "refresh-1",
                "expires_in": 3600
            })))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/bots/bot-1/channels/ch-1/messages"))
            .and(body_string_contains("予約番号"))
            .respond_with(ResponseTemplate::new(send_status))
            .mount(&server)
            .await;
        let client = LineworksBotClient::with_endpoints(
            &format!("{}/bots/", server.uri()),
            &format!("{}/token", server.uri()),
        );
        (server, client)
    }

    #[tokio::test]
    async fn send_text_via_channel_posts_message_and_succeeds() {
        set_encryption_key();
        let (_server, client) = mock_lineworks(200).await;

        let res = send_text_via_channel(
            &StubBotAdmin::with_config(sample_config()),
            &client,
            Uuid::new_v4(),
            &sample_channel(),
            "予約番号は J5JZPEQJ です",
        )
        .await;

        assert!(res.is_ok(), "expected ok, got {:?}", res.err().map(|e| e.0));
    }

    #[tokio::test]
    async fn send_text_via_channel_maps_upstream_failure_to_502() {
        set_encryption_key();
        let (_server, client) = mock_lineworks(500).await;

        let (status, Json(body)) = send_text_via_channel(
            &StubBotAdmin::with_config(sample_config()),
            &client,
            Uuid::new_v4(),
            &sample_channel(),
            "予約番号は J5JZPEQJ です",
        )
        .await
        .unwrap_err();

        assert_eq!(status, StatusCode::BAD_GATEWAY);
        assert_eq!(body["error"], "upstream_error");
    }

    #[tokio::test]
    async fn send_text_via_channel_maps_missing_bot_config_to_500() {
        set_encryption_key();

        let (status, Json(body)) = send_text_via_channel(
            &StubBotAdmin::missing(),
            &LineworksBotClient::new(),
            Uuid::new_v4(),
            &sample_channel(),
            "hello",
        )
        .await
        .unwrap_err();

        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(body["message"], "bot_config_not_found");
    }

    #[tokio::test]
    async fn send_text_via_channel_maps_repo_error_to_500() {
        set_encryption_key();

        let (status, Json(body)) = send_text_via_channel(
            &StubBotAdmin::failing(),
            &LineworksBotClient::new(),
            Uuid::new_v4(),
            &sample_channel(),
            "hello",
        )
        .await
        .unwrap_err();

        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(body["message"], "get_bot_config_failed");
    }

    #[tokio::test]
    async fn send_text_via_channel_maps_decrypt_failure_to_500() {
        set_encryption_key();
        let mut cfg = sample_config();
        cfg.client_secret_encrypted = "not-a-valid-ciphertext".into();

        let (status, Json(body)) = send_text_via_channel(
            &StubBotAdmin::with_config(cfg),
            &LineworksBotClient::new(),
            Uuid::new_v4(),
            &sample_channel(),
            "hello",
        )
        .await
        .unwrap_err();

        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(body["message"], "decrypt_failed");
    }

    #[tokio::test]
    async fn send_text_via_channel_maps_private_key_decrypt_failure_to_500() {
        set_encryption_key();
        let mut cfg = sample_config();
        cfg.private_key_encrypted = "not-a-valid-ciphertext".into();

        let (status, Json(body)) = send_text_via_channel(
            &StubBotAdmin::with_config(cfg),
            &LineworksBotClient::new(),
            Uuid::new_v4(),
            &sample_channel(),
            "hello",
        )
        .await
        .unwrap_err();

        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(body["message"], "decrypt_failed");
    }

    #[test]
    fn channel_not_found_is_404() {
        let (status, Json(body)) = channel_not_found();
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["error"], "channel_not_found");
    }
}
