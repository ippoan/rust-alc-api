/// テナント ユーザー管理 REST API
/// auth-worker の admin/users ページから呼ばれる
use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get, post},
    Extension, Json, Router,
};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use alc_core::auth_middleware::AuthUser;
use alc_core::models::TenantAllowedEmail;
use alc_core::repository::tenant_users::UserRow;
use alc_core::AppState;

#[derive(Debug, Serialize, TS)]
#[ts(export)]
struct UserResponse {
    id: Uuid,
    email: String,
    name: String,
    role: String,
    created_at: chrono::DateTime<chrono::Utc>,
}

impl From<UserRow> for UserResponse {
    fn from(row: UserRow) -> Self {
        Self {
            id: row.id,
            email: row.email,
            name: row.name,
            role: row.role,
            created_at: row.created_at,
        }
    }
}

#[derive(Debug, Serialize)]
struct UsersListResponse {
    users: Vec<UserResponse>,
}

#[derive(Debug, Serialize)]
struct InvitationsListResponse {
    invitations: Vec<TenantAllowedEmail>,
}

#[derive(Debug, Deserialize)]
struct InviteRequest {
    email: String,
    role: Option<String>,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/admin/users", get(list_users))
        .route("/admin/users/invitations", get(list_invitations))
        .route("/admin/users/invite", post(invite_user))
        .route("/admin/users/invite/{id}", delete(delete_invitation))
        .route("/admin/users/{id}", delete(delete_user))
}

/// GET /admin/users — テナント内ユーザー一覧
async fn list_users(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthUser>,
) -> Result<Json<UsersListResponse>, StatusCode> {
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

    Ok(Json(UsersListResponse {
        users: users.into_iter().map(UserResponse::from).collect(),
    }))
}

/// GET /admin/users/invitations — 招待一覧
async fn list_invitations(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthUser>,
) -> Result<Json<InvitationsListResponse>, StatusCode> {
    if auth_user.role != "admin" {
        return Err(StatusCode::FORBIDDEN);
    }

    let invitations = state
        .tenant_users
        .list_invitations(auth_user.tenant_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to list invitations: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(InvitationsListResponse { invitations }))
}

/// POST /admin/users/invite — ユーザー招待
async fn invite_user(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthUser>,
    Json(body): Json<InviteRequest>,
) -> Result<Json<TenantAllowedEmail>, StatusCode> {
    if auth_user.role != "admin" {
        return Err(StatusCode::FORBIDDEN);
    }

    let role = body.role.unwrap_or_else(|| "admin".to_string());
    if role != "admin" && role != "viewer" {
        return Err(StatusCode::BAD_REQUEST);
    }

    let invitation = state
        .tenant_users
        .invite_user(auth_user.tenant_id, &body.email, &role)
        .await
        .map_err(|e| {
            tracing::error!("Failed to invite user: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(invitation))
}

/// DELETE /admin/users/invite/{id} — 招待削除
async fn delete_invitation(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    if auth_user.role != "admin" {
        return Err(StatusCode::FORBIDDEN);
    }

    state
        .tenant_users
        .delete_invitation(auth_user.tenant_id, id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to delete invitation: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(StatusCode::NO_CONTENT)
}

/// DELETE /admin/users/{id} — ユーザー削除
async fn delete_user(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    if auth_user.role != "admin" {
        return Err(StatusCode::FORBIDDEN);
    }

    // 自分自身は削除不可
    if id == auth_user.user_id {
        return Err(StatusCode::BAD_REQUEST);
    }

    state
        .tenant_users
        .delete_user(auth_user.tenant_id, id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to delete user: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(StatusCode::NO_CONTENT)
}
