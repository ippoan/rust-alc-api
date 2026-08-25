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
use alc_core::repository::notify_recipients::NotifyRecipient;
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
/// - `POST /api/internal/lineworks/send` — 登録済み channel / recipient へテキスト送信 (無人 worker 用)
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

fn recipient_not_found() -> ApiError {
    (
        StatusCode::NOT_FOUND,
        Json(serde_json::json!({"error": "recipient_not_found"})),
    )
}

fn bad_request(error: &str, message: &str) -> ApiError {
    (
        StatusCode::BAD_REQUEST,
        Json(serde_json::json!({"error": error, "message": message})),
    )
}

fn upstream_error(e: impl std::fmt::Display) -> ApiError {
    (
        StatusCode::BAD_GATEWAY,
        Json(serde_json::json!({"error": "upstream_error", "message": e.to_string()})),
    )
}

// ---------- shared: 復号 + 送信 ----------

/// bot_config を取得して秘密値を復号し、送信に使える設定にする。
///
/// channel 宛 (`send_text_via_channel`) と recipient 宛 (`send_text_to_lineworks_user`)
/// の共通部。戻り値の `Uuid` は bot_config の id で、クライアント側の
/// アクセストークンキャッシュのキーになる。
///
/// `tenant_id` は呼び出し側が解決したもの — tenant 経路は `X-Tenant-ID`、
/// internal 経路は取得した行 (`row.tenant_id`) 由来で、**この関数は header を
/// 一切見ない**。
async fn resolve_bot_config(
    bot_admin: &Arc<dyn BotAdminRepository>,
    tenant_id: Uuid,
    bot_config_id: Uuid,
) -> Result<(Uuid, LineworksBotConfig), ApiError> {
    let full = bot_admin
        .get_config_with_secrets(tenant_id, bot_config_id)
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

/// channel 行が指す bot_config を復号して LINE WORKS のトークルームへテキストを送る。
///
/// tenant 経路 (`test_send_channel`) と internal 経路 (`send_text_internal` の
/// `channel_id` 指定) の共通部。
async fn send_text_via_channel(
    bot_admin: &Arc<dyn BotAdminRepository>,
    lw_client: &LineworksBotClient,
    tenant_id: Uuid,
    row: &LineworksChannel,
    text: &str,
) -> Result<(), ApiError> {
    let (config_id, config) = resolve_bot_config(bot_admin, tenant_id, row.bot_config_id).await?;

    lw_client
        .send_text_to_channel(config_id, &config, &row.channel_id, text)
        .await
        .map_err(|e| {
            tracing::error!("send_text_to_channel: {e}");
            upstream_error(e)
        })
}

/// recipient 行が LINE WORKS の個人宛先として使えるかを検証し、`lineworks_user_id` を返す。
///
/// `provider` が `lineworks` でない (= `lineworks_user_id` が NULL) 行は 400 で弾く。
/// LINE 宛 (`provider = "line"`) は別の Messaging API 経路なのでここでは扱わない —
/// 黙って何もしないと、呼び出し側は「送れた」と誤認したまま通知が消える。
///
/// `enabled = false` も **400 で弾く** (404 ではなく)。行自体は存在するので 404 に
/// すると呼び出し側が「id が違う」と切り分けられなくなる。無効化は「この宛先には
/// もう送るな」という運用の意思表示で、tenant 経路の配信 (`distribute` が
/// `list_enabled` で引く) も無効な宛先には配らないため、internal 経路だけが
/// 送ってしまうのを避ける。
fn validate_lineworks_recipient(row: &NotifyRecipient) -> Result<&str, ApiError> {
    if row.provider != "lineworks" {
        return Err(bad_request(
            "recipient_not_lineworks",
            &format!("recipient provider is {}", row.provider),
        ));
    }
    let user_id = row.lineworks_user_id.as_deref().ok_or_else(|| {
        bad_request(
            "recipient_not_lineworks",
            "recipient has no lineworks_user_id",
        )
    })?;
    if !row.enabled {
        return Err(bad_request("recipient_disabled", "recipient is disabled"));
    }
    Ok(user_id)
}

/// tenant で有効な LINE WORKS bot config の id を選ぶ。
///
/// `notify_recipients` には `lineworks_channels.bot_config_id` に相当する列が無い
/// (宛先は個人であって Bot に紐づかない) ため、配信オーケストレーター
/// (`distribute::resolve_lineworks_config`) と同じく tenant で 1 つに決める。
///
/// 件数ごとの挙動 (distribute と完全に同じ選び方):
/// - **0 件** (`lineworks` が無い / あるが全て `enabled = false`) → 500
///   `bot_config_not_found`。channel 経路が bot_config を引けなかったときと同じ
///   エラーに倒す (呼び出し側から見ればどちらも「Bot 設定が無い」)。
/// - **複数件** → `list_configs` が返す順 (repo 側 `ORDER BY name`) の最初の 1 件。
///   tenant が LINE WORKS Bot を複数持つ運用は想定していないが、持ったときに
///   internal 経路と `distribute` が別の Bot から送るとログの追跡が壊れるため、
///   **選び方を揃えること自体が契約**。宛先を Bot ごとに分けたくなったら
///   `notify_recipients` に bot_config_id を足す (ここで別ルールを足さない)。
async fn pick_lineworks_bot_config_id(
    bot_admin: &Arc<dyn BotAdminRepository>,
    tenant_id: Uuid,
) -> Result<Uuid, ApiError> {
    let configs = bot_admin.list_configs(tenant_id).await.map_err(|e| {
        tracing::error!("list_configs: {e}");
        internal_error("list_bot_configs_failed")
    })?;

    configs
        .iter()
        .find(|c| c.provider == "lineworks" && c.enabled)
        .map(|c| c.id)
        .ok_or_else(|| internal_error("bot_config_not_found"))
}

/// recipient (個人) 宛に LINE WORKS のダイレクトメッセージを送る。
///
/// `tenant_id` は recipient 行由来 (internal 経路は header を見ない)。
async fn send_text_to_lineworks_user(
    bot_admin: &Arc<dyn BotAdminRepository>,
    lw_client: &LineworksBotClient,
    tenant_id: Uuid,
    lineworks_user_id: &str,
    text: &str,
) -> Result<(), ApiError> {
    let bot_config_id = pick_lineworks_bot_config_id(bot_admin, tenant_id).await?;
    let (config_id, config) = resolve_bot_config(bot_admin, tenant_id, bot_config_id).await?;

    lw_client
        .send_text_to_user(config_id, &config, lineworks_user_id, text)
        .await
        .map_err(|e| {
            tracing::error!("send_text_to_user: {e}");
            upstream_error(e)
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
/// `recipient_id` は **`notify_recipients` の行 id (Uuid)** で、個人宛の
/// ダイレクトメッセージになる。
///
/// **どちらか一方が必須** — 両方指定・両方省略はどちらも 400 で弾く。宛先の
/// 取り違えを黙って起こさないため (片方を優先する実装にすると、呼び出し側の
/// 設定ミスが「意図しない相手に届いた」として現れる)。
#[derive(Debug, Deserialize)]
pub struct InternalSendBody {
    #[serde(default)]
    pub channel_id: Option<Uuid>,
    #[serde(default)]
    pub recipient_id: Option<Uuid>,
    pub text: String,
}

/// `InternalSendBody` が指す宛先。
#[derive(Debug, PartialEq, Eq)]
enum SendTarget {
    /// `lineworks_channels` の行 id (トークルーム宛)
    Channel(Uuid),
    /// `notify_recipients` の行 id (個人宛)
    Recipient(Uuid),
}

/// body の宛先 2 択を検証する (pure)。
fn resolve_send_target(
    channel_id: Option<Uuid>,
    recipient_id: Option<Uuid>,
) -> Result<SendTarget, ApiError> {
    match (channel_id, recipient_id) {
        (Some(c), None) => Ok(SendTarget::Channel(c)),
        (None, Some(r)) => Ok(SendTarget::Recipient(r)),
        (Some(_), Some(_)) => Err(bad_request(
            "target_ambiguous",
            "specify exactly one of channel_id / recipient_id",
        )),
        (None, None) => Err(bad_request(
            "target_required",
            "channel_id or recipient_id is required",
        )),
    }
}

/// auth-worker 経由の無人送信 (dtako-scraper-relay の netprint cron 等)。
///
/// internal 経路は `X-Tenant-ID` を honor しない (shared secret だけで tenant を
/// 詐称できてしまうため — Refs #434)。tenant は channel 行 / recipient 行の
/// RLS バイパス取得 (`get_for_send`) から解決する。
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

    let lw_client = LineworksBotClient::new();

    match resolve_send_target(body.channel_id, body.recipient_id)? {
        SendTarget::Channel(id) => {
            let row = state
                .lineworks_channels
                .get_for_send(id)
                .await
                .map_err(|e| {
                    tracing::error!("get_for_send lineworks_channel: {e}");
                    internal_error("get_failed")
                })?
                .ok_or_else(channel_not_found)?;

            send_text_via_channel(
                &state.bot_admin,
                &lw_client,
                row.tenant_id,
                &row,
                &body.text,
            )
            .await?;
        }
        SendTarget::Recipient(id) => {
            let row = state
                .notify_recipients
                .get_for_send(id)
                .await
                .map_err(|e| {
                    tracing::error!("get_for_send notify_recipient: {e}");
                    internal_error("get_failed")
                })?
                .ok_or_else(recipient_not_found)?;

            let user_id = validate_lineworks_recipient(&row)?;

            send_text_to_lineworks_user(
                &state.bot_admin,
                &lw_client,
                row.tenant_id,
                user_id,
                &body.text,
            )
            .await?;
        }
    }

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

    /// `get_config_with_secrets` / `list_configs` の戻り値だけを差し替えられる
    /// 最小 stub。他メソッドは本 module から呼ばれないので `unimplemented!()`。
    struct StubBotAdmin {
        config: Mutex<Option<BotConfigWithSecrets>>,
        /// `list_configs` (recipient 経路の bot 選択) が返す一覧
        listed: Vec<BotConfigRow>,
        fail: bool,
    }

    impl StubBotAdmin {
        fn with_config(config: BotConfigWithSecrets) -> Arc<dyn BotAdminRepository> {
            Arc::new(Self {
                listed: vec![listed_row(&config, "lineworks", true)],
                config: Mutex::new(Some(config)),
                fail: false,
            })
        }
        /// `list_configs` だけを差し替える (recipient 経路の bot 選択テスト用)
        fn with_listed(
            config: BotConfigWithSecrets,
            listed: Vec<BotConfigRow>,
        ) -> Arc<dyn BotAdminRepository> {
            Arc::new(Self {
                listed,
                config: Mutex::new(Some(config)),
                fail: false,
            })
        }
        fn missing() -> Arc<dyn BotAdminRepository> {
            Arc::new(Self {
                config: Mutex::new(None),
                listed: vec![],
                fail: false,
            })
        }
        fn failing() -> Arc<dyn BotAdminRepository> {
            Arc::new(Self {
                config: Mutex::new(None),
                listed: vec![],
                fail: true,
            })
        }
    }

    fn listed_row(cfg: &BotConfigWithSecrets, provider: &str, enabled: bool) -> BotConfigRow {
        BotConfigRow {
            id: cfg.id,
            provider: provider.into(),
            name: cfg.name.clone(),
            client_id: cfg.client_id.clone(),
            service_account: cfg.service_account.clone(),
            bot_id: cfg.bot_id.clone(),
            enabled,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
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
            if self.fail {
                return Err(sqlx::Error::RowNotFound);
            }
            Ok(self.listed.clone())
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
        // recipient (個人) 宛は channels ではなく users パス
        Mock::given(method("POST"))
            .and(path("/bots/bot-1/users/lw-user-1/messages"))
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

    #[test]
    fn recipient_not_found_is_404() {
        let (status, Json(body)) = recipient_not_found();
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["error"], "recipient_not_found");
    }

    // ---------- 宛先 2 択の検証 ----------

    #[test]
    fn resolve_send_target_picks_the_single_specified_target() {
        let c = Uuid::new_v4();
        let r = Uuid::new_v4();
        assert_eq!(
            resolve_send_target(Some(c), None).unwrap(),
            SendTarget::Channel(c)
        );
        assert_eq!(
            resolve_send_target(None, Some(r)).unwrap(),
            SendTarget::Recipient(r)
        );
    }

    #[test]
    fn resolve_send_target_rejects_both_specified() {
        let (status, Json(body)) =
            resolve_send_target(Some(Uuid::new_v4()), Some(Uuid::new_v4())).unwrap_err();
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["error"], "target_ambiguous");
    }

    #[test]
    fn resolve_send_target_rejects_neither_specified() {
        let (status, Json(body)) = resolve_send_target(None, None).unwrap_err();
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["error"], "target_required");
    }

    // ---------- recipient 行の検証 ----------

    fn sample_recipient() -> NotifyRecipient {
        NotifyRecipient {
            id: Uuid::new_v4(),
            tenant_id: Uuid::new_v4(),
            name: "本多 優鷹".into(),
            provider: "lineworks".into(),
            lineworks_user_id: Some("lw-user-1".into()),
            line_user_id: None,
            phone_number: None,
            email: None,
            enabled: true,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn validate_lineworks_recipient_returns_user_id() {
        let row = sample_recipient();
        assert_eq!(validate_lineworks_recipient(&row).unwrap(), "lw-user-1");
    }

    #[test]
    fn validate_lineworks_recipient_rejects_line_provider() {
        let mut row = sample_recipient();
        row.provider = "line".into();
        row.lineworks_user_id = None;
        row.line_user_id = Some("U123".into());

        let (status, Json(body)) = validate_lineworks_recipient(&row).unwrap_err();
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["error"], "recipient_not_lineworks");
    }

    /// provider だけ lineworks で user_id が入っていない不整合行も同じ 400。
    #[test]
    fn validate_lineworks_recipient_rejects_missing_user_id() {
        let mut row = sample_recipient();
        row.lineworks_user_id = None;

        let (status, Json(body)) = validate_lineworks_recipient(&row).unwrap_err();
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["error"], "recipient_not_lineworks");
    }

    /// 無効化された宛先は 404 ではなく 400 — 行は在るので「id 違い」と混同させない。
    #[test]
    fn validate_lineworks_recipient_rejects_disabled() {
        let mut row = sample_recipient();
        row.enabled = false;

        let (status, Json(body)) = validate_lineworks_recipient(&row).unwrap_err();
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["error"], "recipient_disabled");
    }

    // ---------- recipient 宛の bot 選択 + 送信 ----------

    #[tokio::test]
    async fn send_text_to_lineworks_user_posts_to_users_path() {
        set_encryption_key();
        let (_server, client) = mock_lineworks(200).await;

        let res = send_text_to_lineworks_user(
            &StubBotAdmin::with_config(sample_config()),
            &client,
            Uuid::new_v4(),
            "lw-user-1",
            "予約番号は J5JZPEQJ です",
        )
        .await;

        assert!(res.is_ok(), "expected ok, got {:?}", res.err().map(|e| e.0));
    }

    #[tokio::test]
    async fn send_text_to_lineworks_user_maps_upstream_failure_to_502() {
        set_encryption_key();
        let (_server, client) = mock_lineworks(500).await;

        let (status, Json(body)) = send_text_to_lineworks_user(
            &StubBotAdmin::with_config(sample_config()),
            &client,
            Uuid::new_v4(),
            "lw-user-1",
            "予約番号は J5JZPEQJ です",
        )
        .await
        .unwrap_err();

        assert_eq!(status, StatusCode::BAD_GATEWAY);
        assert_eq!(body["error"], "upstream_error");
    }

    #[tokio::test]
    async fn pick_lineworks_bot_config_id_skips_disabled_and_other_providers() {
        let cfg = sample_config();
        let line_row = listed_row(&cfg, "line", true);
        let disabled_row = listed_row(&cfg, "lineworks", false);
        let mut wanted = listed_row(&cfg, "lineworks", true);
        wanted.id = Uuid::new_v4();

        let admin = StubBotAdmin::with_listed(cfg, vec![line_row, disabled_row, wanted.clone()]);
        assert_eq!(
            pick_lineworks_bot_config_id(&admin, Uuid::new_v4())
                .await
                .unwrap(),
            wanted.id
        );
    }

    #[tokio::test]
    async fn pick_lineworks_bot_config_id_maps_empty_list_to_500() {
        let (status, Json(body)) =
            pick_lineworks_bot_config_id(&StubBotAdmin::missing(), Uuid::new_v4())
                .await
                .unwrap_err();

        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(body["message"], "bot_config_not_found");
    }

    #[tokio::test]
    async fn pick_lineworks_bot_config_id_maps_repo_error_to_500() {
        let (status, Json(body)) =
            pick_lineworks_bot_config_id(&StubBotAdmin::failing(), Uuid::new_v4())
                .await
                .unwrap_err();

        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(body["message"], "list_bot_configs_failed");
    }

    // ---------- body の後方互換 ----------

    /// #596 が確定させた `{channel_id, text}` はそのまま deserialize できること
    /// (relay 側は当面この形のまま送ってくる)。
    #[test]
    fn internal_send_body_accepts_legacy_channel_only_shape() {
        let id = Uuid::new_v4();
        let body: InternalSendBody =
            serde_json::from_value(serde_json::json!({"channel_id": id, "text": "hi"})).unwrap();
        assert_eq!(body.channel_id, Some(id));
        assert_eq!(body.recipient_id, None);
    }

    #[test]
    fn internal_send_body_accepts_recipient_only_shape() {
        let id = Uuid::new_v4();
        let body: InternalSendBody =
            serde_json::from_value(serde_json::json!({"recipient_id": id, "text": "hi"})).unwrap();
        assert_eq!(body.recipient_id, Some(id));
        assert_eq!(body.channel_id, None);
    }

    /// **キー無しと明示 null は同じ「未指定」**。caller が
    /// `{channel_id: null, recipient_id: "…"}` と書いても 400 にしない
    /// (relay 側がどちらの書き方でも安全に通るように)。
    #[test]
    fn internal_send_body_treats_explicit_null_as_unspecified() {
        let id = Uuid::new_v4();
        let body: InternalSendBody = serde_json::from_value(
            serde_json::json!({"channel_id": null, "recipient_id": id, "text": "hi"}),
        )
        .unwrap();
        assert_eq!(body.channel_id, None);
        assert_eq!(
            resolve_send_target(body.channel_id, body.recipient_id).unwrap(),
            SendTarget::Recipient(id)
        );

        let body: InternalSendBody = serde_json::from_value(
            serde_json::json!({"channel_id": id, "recipient_id": null, "text": "hi"}),
        )
        .unwrap();
        assert_eq!(body.recipient_id, None);
        assert_eq!(
            resolve_send_target(body.channel_id, body.recipient_id).unwrap(),
            SendTarget::Channel(id)
        );
    }

    /// 両方 null は「両方省略」と同じ 400 `target_required`。
    #[test]
    fn internal_send_body_treats_both_null_as_target_required() {
        let body: InternalSendBody = serde_json::from_value(
            serde_json::json!({"channel_id": null, "recipient_id": null, "text": "hi"}),
        )
        .unwrap();
        let (status, Json(err)) =
            resolve_send_target(body.channel_id, body.recipient_id).unwrap_err();
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(err["error"], "target_required");
    }
}
