//! nuxt-dtako-admin 向け `/api/members` 系エンドポイント。
//!
//! 既存 `/admin/users` (auth-worker admin UI 用) が返す
//! `{ users: [...] }` 形式とは別の、フラットな `TenantMember[]` を
//! frontend に返すための alias layer。
//! 既存の `TenantUsersRepository` を再利用する。

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, patch},
    Extension, Json, Router,
};
use serde::{Deserialize, Serialize};

use alc_core::auth_middleware::AuthUser;
use alc_core::AppState;

/// frontend (nuxt-dtako-admin) が期待する共通形。
/// 登録済みユーザーと未承諾の招待 (tenant_allowed_emails) を同じ形で返す。
#[derive(Debug, Serialize)]
struct TenantMember {
    email: String,
    role: String,
    created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize)]
struct InviteRequest {
    email: String,
    role: Option<String>,
}

#[derive(Debug, Deserialize)]
struct UpdateRoleRequest {
    role: String,
}

/// frontend から送られてくる role を受け付ける集合。
/// DB 側は text でゆるいので、`member` などの frontend 固有値もそのまま保存する。
fn is_allowed_role(role: &str) -> bool {
    matches!(role, "admin" | "viewer" | "member" | "payroll")
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/members", get(list_members).post(invite_member))
        .route(
            "/members/{email}",
            patch(update_member_role).delete(delete_member),
        )
}

/// GET /members — 登録済みユーザー + 未承諾の招待をマージしたフラット配列。
async fn list_members(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthUser>,
) -> Result<Json<Vec<TenantMember>>, StatusCode> {
    if auth_user.role != "admin" {
        return Err(StatusCode::FORBIDDEN);
    }

    let users = state
        .tenant_users
        .list_users(auth_user.tenant_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to list users: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let invitations = state
        .tenant_users
        .list_invitations(auth_user.tenant_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to list invitations: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let mut members: Vec<TenantMember> = users
        .into_iter()
        .map(|u| TenantMember {
            email: u.email,
            role: u.role,
            created_at: u.created_at,
        })
        .collect();

    // 既にユーザー登録済みの email は招待側から除外 (重複排除)
    let registered: std::collections::HashSet<String> =
        members.iter().map(|m| m.email.clone()).collect();

    for inv in invitations {
        if !registered.contains(&inv.email) {
            members.push(TenantMember {
                email: inv.email,
                role: inv.role,
                created_at: inv.created_at,
            });
        }
    }

    Ok(Json(members))
}

/// POST /members — 招待 (`tenant_allowed_emails` への upsert) を行い、
/// TenantMember 形で返す。
async fn invite_member(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthUser>,
    Json(body): Json<InviteRequest>,
) -> Result<Json<TenantMember>, StatusCode> {
    if auth_user.role != "admin" {
        return Err(StatusCode::FORBIDDEN);
    }

    let role = body.role.unwrap_or_else(|| "member".to_string());
    if !is_allowed_role(&role) {
        return Err(StatusCode::BAD_REQUEST);
    }

    let invitation = state
        .tenant_users
        .invite_user(auth_user.tenant_id, &body.email, &role)
        .await
        .map_err(|e| {
            tracing::error!("Failed to invite member: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(TenantMember {
        email: invitation.email,
        role: invitation.role,
        created_at: invitation.created_at,
    }))
}

/// PATCH /members/{email} — email 単位で role を更新する。
/// 登録済みユーザー / 招待どちらの行も更新対象になる。
async fn update_member_role(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthUser>,
    Path(email): Path<String>,
    Json(body): Json<UpdateRoleRequest>,
) -> Result<StatusCode, StatusCode> {
    if auth_user.role != "admin" {
        return Err(StatusCode::FORBIDDEN);
    }
    if !is_allowed_role(&body.role) {
        return Err(StatusCode::BAD_REQUEST);
    }

    let found = state
        .tenant_users
        .update_role_by_email(auth_user.tenant_id, &email, &body.role)
        .await
        .map_err(|e| {
            tracing::error!("Failed to update member role: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    if !found {
        return Err(StatusCode::NOT_FOUND);
    }

    Ok(StatusCode::NO_CONTENT)
}

/// DELETE /members/{email} — email 単位で削除する。
async fn delete_member(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthUser>,
    Path(email): Path<String>,
) -> Result<StatusCode, StatusCode> {
    if auth_user.role != "admin" {
        return Err(StatusCode::FORBIDDEN);
    }

    // 自分自身は削除不可
    if email == auth_user.email {
        return Err(StatusCode::BAD_REQUEST);
    }

    let found = state
        .tenant_users
        .delete_by_email(auth_user.tenant_id, &email)
        .await
        .map_err(|e| {
            tracing::error!("Failed to delete member: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    if !found {
        return Err(StatusCode::NOT_FOUND);
    }

    Ok(StatusCode::NO_CONTENT)
}
