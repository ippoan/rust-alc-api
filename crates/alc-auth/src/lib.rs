use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post},
    Extension, Json, Router,
};
use serde::Serialize;
use uuid::Uuid;

use alc_core::auth_middleware::AuthUser;
use alc_core::AppState;

// 認証 JWT の発行・検証と OAuth オーケストレーション (Google / LINE / LINE WORKS /
// WOFF / password login / refresh / switch-org) は auth-worker に完全移管した
// (Refs #479 PR-3、旧 public_router ごと撤去)。auth-worker が叩くログインの内部口
// (`/internal/auth/*`) も alc-auth-worker へ移った (Refs #680)。rust 側に残るのは
// 下記 protected_router — 前段 proxy が注入した identity を返すだけの薄い endpoint。

/// 保護ルート (require_tenant_header の後ろに配置。前段 proxy が注入する identity を信頼)
pub fn protected_router() -> Router<AppState> {
    Router::new()
        .route("/auth/me", get(me))
        .route("/auth/logout", post(logout))
        .route("/my-orgs", post(my_orgs))
}

#[derive(Debug, Serialize)]
pub struct UserResponse {
    pub id: Uuid,
    pub email: String,
    pub name: String,
    pub tenant_id: Uuid,
    /// 廃止予定。`tenants.slug` (NULL のことが多い)。
    /// 新しいフロントは `tenant_short_id` を使う。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tenant_slug: Option<String>,
    /// `tenants.short_id` (8 文字 hex、UNIQUE、NOT NULL)。
    /// メール ingest や URL 生成で使う。
    pub tenant_short_id: String,
    pub role: String,
}

// --- Me ---

async fn me(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthUser>,
) -> Result<Json<UserResponse>, StatusCode> {
    // tenant_short_id は注入 identity ヘッダーに含めていないので毎回 DB から引く
    // (NOT NULL なので必ずある。Lookup 1 回 = ms 単位で済むので /auth/me には
    // 無視できるコスト)。
    let short_id = state
        .auth
        .get_tenant_short_id(auth_user.tenant_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(UserResponse {
        id: auth_user.user_id,
        email: auth_user.email,
        name: auth_user.name,
        tenant_id: auth_user.tenant_id,
        tenant_slug: auth_user.tenant_slug,
        tenant_short_id: short_id,
        role: auth_user.role,
    }))
}

// --- Logout ---

async fn logout(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthUser>,
) -> Result<StatusCode, StatusCode> {
    state
        .auth
        .clear_refresh_token(auth_user.tenant_id, auth_user.user_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(StatusCode::NO_CONTENT)
}

// --- My orgs ---

#[derive(Debug, Serialize)]
struct MyOrgsResponse {
    organizations: Vec<OrgItem>,
}

#[derive(Debug, Serialize)]
struct OrgItem {
    id: Uuid,
    name: String,
    slug: String,
    role: String,
}

/// ユーザーが所属するテナント一覧を返す
async fn my_orgs(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthUser>,
) -> Result<Json<MyOrgsResponse>, StatusCode> {
    let tenant = state
        .auth
        .get_tenant_by_id(auth_user.tenant_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let orgs = match tenant {
        Some(t) => vec![OrgItem {
            id: t.id,
            name: t.name,
            slug: t.slug.unwrap_or_default(),
            role: auth_user.role,
        }],
        None => vec![],
    };

    Ok(Json(MyOrgsResponse {
        organizations: orgs,
    }))
}
