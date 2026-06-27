//! 内部認証データ API (Refs ippoan/rust-alc-api#434)。
//!
//! 認証オーケストレーション (OAuth code 交換 / JWT 発行) を auth-worker に移管する
//! ための、DB プリミティブを薄く公開する internal endpoint 群。`require_internal_jwt`
//! 配下に nest され、**auth-worker (= `aud=alc-api-internal` を mint できる唯一の
//! caller) だけ**が呼べる。
//!
//! JWT 発行は呼び出し側 (auth-worker) が `create_access_token` 相当で行うため、本
//! endpoint は user / recipient / sso-config の read / upsert のみを担い、token は
//! 発行しない。auth-worker が JWT 組み立てに必要な user フィールド + tenant slug を
//! 1 度に返す。
//!
//! lockdown (`allUsers` 削除) 後は `require_internal_jwt` が OIDC custom audience
//! (`alc-api-internal`) 検証に置換されるが、本ルート定義は不変。`/alc-proxy` は
//! service-URL audience でしか OIDC を mint しないため、consumer が `/alc-proxy`
//! 経由で本 internal route に到達しても `aud` 不一致で弾かれる (confused-deputy 防止)。

use axum::{
    extract::{Query, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use alc_core::models::User;
use alc_core::AppState;

type ErrorResponse = (StatusCode, Json<serde_json::Value>);
type ApiResult<T> = Result<Json<T>, ErrorResponse>;

/// internal レスポンス用に `User` から秘匿フィールド (`password_hash` /
/// `refresh_token_*`) を除いた DTO。auth-worker は本 DTO + `slug` から access JWT を
/// 組み立てる。
#[derive(Debug, Serialize)]
pub struct InternalUser {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub email: String,
    pub name: String,
    pub role: String,
    pub google_sub: Option<String>,
    pub lineworks_id: Option<String>,
    pub line_user_id: Option<String>,
}

impl From<User> for InternalUser {
    fn from(u: User) -> Self {
        Self {
            id: u.id,
            tenant_id: u.tenant_id,
            email: u.email,
            name: u.name,
            role: u.role,
            google_sub: u.google_sub,
            lineworks_id: u.lineworks_id,
            line_user_id: u.line_user_id,
        }
    }
}

/// JWT 発行に必要な user + tenant slug を 1 度に返す。
#[derive(Debug, Serialize)]
pub struct InternalUserWithSlug {
    #[serde(flatten)]
    pub user: InternalUser,
    pub slug: Option<String>,
}

/// SSO 設定 (auth-worker が code 交換に使う)。`client_secret_encrypted` は暗号化
/// 済みのまま返し、復号は auth-worker 側で行う (lineworks bot-secret と同方針 =
/// rust は平文 secret を response に echo しない)。
#[derive(Debug, Serialize)]
pub struct InternalSsoConfig {
    pub tenant_id: Uuid,
    pub client_id: String,
    pub client_secret_encrypted: String,
    pub external_org_id: String,
    pub woff_id: Option<String>,
}

/// recipient 逆引き 1 件 (tenant_id, name)。
#[derive(Debug, Serialize)]
pub struct RecipientTenant {
    pub tenant_id: Uuid,
    pub name: String,
}

fn internal_error(context: &str, err: impl std::fmt::Display) -> ErrorResponse {
    let detail = err.to_string();
    tracing::error!("internal auth endpoint error ({context}): {detail}");
    // staging (揮発 DB) でのみ DB エラー詳細を response に載せて診断を高速化する。
    // 本番は内部エラー文言を隠す (情報漏洩防止)。
    let body = if std::env::var("STAGING_MODE").as_deref() == Ok("true") {
        serde_json::json!({ "error": "internal_error", "context": context, "detail": detail })
    } else {
        serde_json::json!({ "error": "internal_error" })
    };
    (StatusCode::INTERNAL_SERVER_ERROR, Json(body))
}

fn not_found(error: &str) -> ErrorResponse {
    (
        StatusCode::NOT_FOUND,
        Json(serde_json::json!({ "error": error })),
    )
}

/// `require_internal_jwt` 配下に nest される internal 認証データ route 群。
pub fn internal_router() -> Router<AppState> {
    Router::new()
        .route("/internal/auth/sso-config", get(get_sso_config))
        .route(
            "/internal/auth/users/upsert-lineworks",
            post(upsert_lineworks_user),
        )
        .route("/internal/auth/users/upsert-line", post(upsert_line_user))
        .route("/internal/auth/users/by-line-id", get(user_by_line_id))
        .route(
            "/internal/auth/recipients/register-line",
            post(register_line_recipient),
        )
        .route(
            "/internal/auth/recipients/by-line-id",
            get(recipients_by_line_id),
        )
        .route("/internal/auth/refresh-token", post(save_refresh_token))
}

/// tenant slug を解決して `InternalUserWithSlug` を返す共通ヘルパ。
async fn with_slug(state: &AppState, user: User) -> ApiResult<InternalUserWithSlug> {
    let slug = state
        .auth
        .get_tenant_slug(user.tenant_id)
        .await
        .map_err(|e| internal_error("get_tenant_slug", e))?;
    Ok(Json(InternalUserWithSlug {
        user: user.into(),
        slug,
    }))
}

// ---------- GET /internal/auth/sso-config?provider=&domain= ----------

#[derive(Debug, Deserialize)]
struct SsoConfigQuery {
    provider: String,
    domain: String,
}

async fn get_sso_config(
    State(state): State<AppState>,
    Query(q): Query<SsoConfigQuery>,
) -> ApiResult<InternalSsoConfig> {
    let cfg = state
        .auth
        .resolve_sso_config(&q.provider, &q.domain)
        .await
        .map_err(|e| internal_error("resolve_sso_config", e))?;
    match cfg {
        Some(c) => Ok(Json(InternalSsoConfig {
            tenant_id: c.tenant_id,
            client_id: c.client_id,
            client_secret_encrypted: c.client_secret_encrypted,
            external_org_id: c.external_org_id,
            woff_id: c.woff_id,
        })),
        None => Err(not_found("sso_config_not_found")),
    }
}

// ---------- POST /internal/auth/users/upsert-lineworks ----------

#[derive(Debug, Deserialize)]
struct UpsertLineworksBody {
    tenant_id: Uuid,
    lineworks_id: String,
    email: String,
    name: String,
}

async fn upsert_lineworks_user(
    State(state): State<AppState>,
    Json(b): Json<UpsertLineworksBody>,
) -> ApiResult<InternalUserWithSlug> {
    let existing = state
        .auth
        .find_user_by_lineworks_id(&b.lineworks_id)
        .await
        .map_err(|e| internal_error("find_user_by_lineworks_id", e))?;
    let user = match existing {
        Some(u) => u,
        None => state
            .auth
            .create_user_lineworks(b.tenant_id, &b.lineworks_id, &b.email, &b.name)
            .await
            .map_err(|e| internal_error("create_user_lineworks", e))?,
    };
    with_slug(&state, user).await
}

// ---------- POST /internal/auth/users/upsert-line ----------

#[derive(Debug, Deserialize)]
struct UpsertLineBody {
    tenant_id: Uuid,
    line_user_id: String,
    name: String,
}

async fn upsert_line_user(
    State(state): State<AppState>,
    Json(b): Json<UpsertLineBody>,
) -> ApiResult<InternalUserWithSlug> {
    let existing = state
        .auth
        .find_user_by_line_user_id(&b.line_user_id)
        .await
        .map_err(|e| internal_error("find_user_by_line_user_id", e))?;
    let user = match existing {
        Some(u) => u,
        None => state
            .auth
            .create_user_line(b.tenant_id, &b.line_user_id, &b.name)
            .await
            .map_err(|e| internal_error("create_user_line", e))?,
    };
    with_slug(&state, user).await
}

// ---------- GET /internal/auth/users/by-line-id?line_user_id= ----------

#[derive(Debug, Deserialize)]
struct ByLineIdQuery {
    line_user_id: String,
}

async fn user_by_line_id(
    State(state): State<AppState>,
    Query(q): Query<ByLineIdQuery>,
) -> ApiResult<Option<InternalUserWithSlug>> {
    let user = state
        .auth
        .find_user_by_line_user_id(&q.line_user_id)
        .await
        .map_err(|e| internal_error("find_user_by_line_user_id", e))?;
    match user {
        Some(u) => {
            let slug = state
                .auth
                .get_tenant_slug(u.tenant_id)
                .await
                .map_err(|e| internal_error("get_tenant_slug", e))?;
            Ok(Json(Some(InternalUserWithSlug {
                user: u.into(),
                slug,
            })))
        }
        None => Ok(Json(None)),
    }
}

// ---------- POST /internal/auth/recipients/register-line ----------

#[derive(Debug, Deserialize)]
struct RegisterRecipientBody {
    tenant_id: Uuid,
    name: String,
    line_user_id: String,
}

async fn register_line_recipient(
    State(state): State<AppState>,
    Json(b): Json<RegisterRecipientBody>,
) -> Result<StatusCode, ErrorResponse> {
    state
        .auth
        .register_line_recipient(b.tenant_id, &b.name, &b.line_user_id)
        .await
        .map_err(|e| internal_error("register_line_recipient", e))?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------- POST /internal/auth/refresh-token ----------

#[derive(Debug, Deserialize)]
struct SaveRefreshTokenBody {
    user_id: Uuid,
    /// hex(sha256(raw)) — auth-worker 側で生成済みの hash (raw は載せない)。
    refresh_hash: String,
    expires_at: DateTime<Utc>,
}

/// auth-worker が発行した refresh token の hash を保存する。raw token は
/// auth-worker が browser に返すのみで、ここには hash しか渡さない (rust は raw を持たない)。
async fn save_refresh_token(
    State(state): State<AppState>,
    Json(b): Json<SaveRefreshTokenBody>,
) -> Result<StatusCode, ErrorResponse> {
    state
        .auth
        .save_refresh_token(b.user_id, &b.refresh_hash, b.expires_at)
        .await
        .map_err(|e| internal_error("save_refresh_token", e))?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------- GET /internal/auth/recipients/by-line-id?line_user_id= ----------

async fn recipients_by_line_id(
    State(state): State<AppState>,
    Query(q): Query<ByLineIdQuery>,
) -> ApiResult<Vec<RecipientTenant>> {
    let rows = state
        .auth
        .find_recipients_by_line_user_id(&q.line_user_id)
        .await
        .map_err(|e| internal_error("find_recipients_by_line_user_id", e))?;
    Ok(Json(
        rows.into_iter()
            .map(|(tenant_id, name)| RecipientTenant { tenant_id, name })
            .collect(),
    ))
}
