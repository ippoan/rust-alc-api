/// テナント ユーザー管理 REST API
/// auth-worker の admin/users ページから呼ばれる
use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get, post},
    Extension, Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::db::models::TenantAllowedEmail;
use crate::db::tenant::set_current_tenant;
use crate::middleware::auth::AuthUser;
use crate::AppState;

#[derive(Debug, Serialize, sqlx::FromRow)]
struct UserResponse {
    id: Uuid,
    email: String,
    name: String,
    role: String,
    created_at: chrono::DateTime<chrono::Utc>,
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

    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &auth_user.tenant_id.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let users = sqlx::query_as::<_, UserResponse>(
        "SELECT id, email, name, role, created_at FROM users ORDER BY created_at",
    )
    .fetch_all(&mut *conn)
    .await
    .map_err(|e| {
        tracing::error!("Failed to list users: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(UsersListResponse { users }))
}

/// GET /admin/users/invitations — 招待一覧
async fn list_invitations(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthUser>,
) -> Result<Json<InvitationsListResponse>, StatusCode> {
    if auth_user.role != "admin" {
        return Err(StatusCode::FORBIDDEN);
    }

    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &auth_user.tenant_id.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let invitations = sqlx::query_as::<_, TenantAllowedEmail>(
        "SELECT * FROM tenant_allowed_emails ORDER BY created_at",
    )
    .fetch_all(&mut *conn)
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

    let invitation = sqlx::query_as::<_, TenantAllowedEmail>(
        r#"
        INSERT INTO tenant_allowed_emails (tenant_id, email, role)
        VALUES ($1, $2, $3)
        ON CONFLICT (email) DO UPDATE SET role = EXCLUDED.role
        RETURNING *
        "#,
    )
    .bind(auth_user.tenant_id)
    .bind(&body.email)
    .bind(&role)
    .fetch_one(&state.pool)
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

    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &auth_user.tenant_id.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    sqlx::query("DELETE FROM tenant_allowed_emails WHERE id = $1")
        .bind(id)
        .execute(&mut *conn)
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

    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &auth_user.tenant_id.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(id)
        .execute(&mut *conn)
        .await
        .map_err(|e| {
            tracing::error!("Failed to delete user: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(StatusCode::NO_CONTENT)
}
