/// Access Requests (テナント参加申請) REST API
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post},
    Extension, Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use alc_core::auth_middleware::AuthUser;
use alc_core::AppState;

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
struct AccessRequestRow {
    id: Uuid,
    tenant_id: Uuid,
    user_id: Uuid,
    status: String,
    role: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
struct AccessRequestResponse {
    id: Uuid,
    tenant_id: Uuid,
    user_id: Uuid,
    status: String,
    role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    org_name: Option<String>,
    created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
struct ListResponse {
    requests: Vec<AccessRequestResponse>,
}

#[derive(Debug, Deserialize)]
struct ListParams {
    #[serde(default)]
    status: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CreateRequest {
    org_slug: String,
}

#[derive(Debug, Deserialize)]
struct ApproveRequest {
    #[serde(default)]
    role: Option<String>,
}

#[derive(Debug, Serialize)]
struct TenantBySlugResponse {
    found: bool,
    id: Uuid,
    name: String,
    slug: String,
}

/// 公開ルート (テナント slug 検索 — 認証不要)
pub fn public_router() -> Router<AppState> {
    Router::new().route("/tenants/by-slug/{slug}", get(get_tenant_by_slug))
}

/// 保護ルート (JWT 必須)
pub fn protected_router() -> Router<AppState> {
    Router::new()
        .route("/access-requests", post(create_request))
        .route("/access-requests", get(list_requests))
        .route("/access-requests/{id}/approve", post(approve_request))
        .route("/access-requests/{id}/decline", post(decline_request))
}

async fn get_tenant_by_slug(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<Json<TenantBySlugResponse>, StatusCode> {
    let tenant = sqlx::query_as::<_, (Uuid, String, Option<String>)>(
        "SELECT id, name, slug FROM alc_api.tenants WHERE slug = $1",
    )
    .bind(&slug)
    .fetch_optional(state.pool())
    .await
    .map_err(|e| {
        tracing::error!("Failed to lookup tenant by slug: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?
    .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(TenantBySlugResponse {
        found: true,
        id: tenant.0,
        name: tenant.1,
        slug: tenant.2.unwrap_or_default(),
    }))
}

async fn create_request(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthUser>,
    Json(body): Json<CreateRequest>,
) -> Result<(StatusCode, Json<AccessRequestResponse>), StatusCode> {
    // slug でテナントを検索
    let tenant =
        sqlx::query_as::<_, (Uuid, String)>("SELECT id, name FROM alc_api.tenants WHERE slug = $1")
            .bind(&body.org_slug)
            .fetch_optional(state.pool())
            .await
            .map_err(|e| {
                tracing::error!("Failed to lookup tenant by slug: {e}");
                StatusCode::INTERNAL_SERVER_ERROR
            })?
            .ok_or(StatusCode::NOT_FOUND)?;

    // アクセスリクエストを作成
    let row = sqlx::query_as::<_, AccessRequestRow>(
        r#"INSERT INTO alc_api.access_requests (tenant_id, user_id)
           VALUES ($1, $2) RETURNING *"#,
    )
    .bind(tenant.0)
    .bind(auth_user.user_id)
    .fetch_one(state.pool())
    .await
    .map_err(|e| {
        tracing::error!("Failed to create access request: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok((
        StatusCode::CREATED,
        Json(AccessRequestResponse {
            id: row.id,
            tenant_id: row.tenant_id,
            user_id: row.user_id,
            status: row.status,
            role: row.role,
            org_name: Some(tenant.1),
            created_at: row.created_at,
        }),
    ))
}

async fn list_requests(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthUser>,
    Query(params): Query<ListParams>,
) -> Result<Json<ListResponse>, StatusCode> {
    if auth_user.role != "admin" {
        return Err(StatusCode::FORBIDDEN);
    }

    let rows = if let Some(status) = &params.status {
        sqlx::query_as::<_, AccessRequestRow>(
            "SELECT * FROM alc_api.access_requests WHERE tenant_id = $1 AND status = $2 ORDER BY created_at DESC",
        )
        .bind(auth_user.tenant_id)
        .bind(status)
        .fetch_all(state.pool())
        .await
    } else {
        sqlx::query_as::<_, AccessRequestRow>(
            "SELECT * FROM alc_api.access_requests WHERE tenant_id = $1 ORDER BY created_at DESC",
        )
        .bind(auth_user.tenant_id)
        .fetch_all(state.pool())
        .await
    }
    .map_err(|e| {
        tracing::error!("Failed to list access requests: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let requests = rows
        .into_iter()
        .map(|r| AccessRequestResponse {
            id: r.id,
            tenant_id: r.tenant_id,
            user_id: r.user_id,
            status: r.status,
            role: r.role,
            org_name: None,
            created_at: r.created_at,
        })
        .collect();

    Ok(Json(ListResponse { requests }))
}

async fn approve_request(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
    Json(body): Json<Option<ApproveRequest>>,
) -> Result<StatusCode, StatusCode> {
    if auth_user.role != "admin" {
        return Err(StatusCode::FORBIDDEN);
    }

    let role = body
        .and_then(|b| b.role)
        .unwrap_or_else(|| "viewer".to_string());

    // ステータスを approved に更新
    let result = sqlx::query(
        "UPDATE alc_api.access_requests SET status = 'approved', role = $1, updated_at = now() WHERE id = $2 AND tenant_id = $3 AND status = 'pending'",
    )
    .bind(&role)
    .bind(id)
    .bind(auth_user.tenant_id)
    .execute(state.pool())
    .await
    .map_err(|e| {
        tracing::error!("Failed to approve access request: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    if result.rows_affected() == 0 {
        return Err(StatusCode::NOT_FOUND);
    }

    Ok(StatusCode::NO_CONTENT)
}

async fn decline_request(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    if auth_user.role != "admin" {
        return Err(StatusCode::FORBIDDEN);
    }

    let result = sqlx::query(
        "UPDATE alc_api.access_requests SET status = 'declined', updated_at = now() WHERE id = $1 AND tenant_id = $2 AND status = 'pending'",
    )
    .bind(id)
    .bind(auth_user.tenant_id)
    .execute(state.pool())
    .await
    .map_err(|e| {
        tracing::error!("Failed to decline access request: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    if result.rows_affected() == 0 {
        return Err(StatusCode::NOT_FOUND);
    }

    Ok(StatusCode::NO_CONTENT)
}
