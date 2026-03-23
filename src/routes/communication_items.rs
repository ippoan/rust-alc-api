use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::db::models::{CommunicationItem, CreateCommunicationItem, UpdateCommunicationItem};
use crate::db::tenant::set_current_tenant;
use crate::middleware::auth::TenantId;
use crate::AppState;

pub fn tenant_router() -> Router<AppState> {
    Router::new()
        .route(
            "/communication-items",
            get(list_items).post(create_item),
        )
        .route(
            "/communication-items/{id}",
            get(get_item).put(update_item).delete(delete_item),
        )
        .route("/communication-items/active", get(list_active_items))
}

#[derive(Debug, Deserialize)]
struct CommunicationFilter {
    is_active: Option<bool>,
    target_employee_id: Option<Uuid>,
    page: Option<i64>,
    per_page: Option<i64>,
}

#[derive(Debug, Serialize)]
struct CommunicationItemsResponse {
    items: Vec<CommunicationItemWithName>,
    total: i64,
    page: i64,
    per_page: i64,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
struct CommunicationItemWithName {
    id: Uuid,
    tenant_id: Uuid,
    title: String,
    content: String,
    priority: String,
    target_employee_id: Option<Uuid>,
    target_employee_name: Option<String>,
    is_active: bool,
    effective_from: Option<chrono::DateTime<chrono::Utc>>,
    effective_until: Option<chrono::DateTime<chrono::Utc>>,
    created_by: Option<String>,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

async fn list_items(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Query(filter): Query<CommunicationFilter>,
) -> Result<Json<CommunicationItemsResponse>, StatusCode> {
    let tenant_id = tenant.0 .0;
    let page = filter.page.unwrap_or(1).max(1);
    let per_page = filter.per_page.unwrap_or(20).min(100);
    let offset = (page - 1) * per_page;

    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let total: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM alc_api.communication_items c
           WHERE ($1::BOOLEAN IS NULL OR c.is_active = $1)
             AND ($2::UUID IS NULL OR c.target_employee_id = $2 OR c.target_employee_id IS NULL)"#,
    )
    .bind(filter.is_active)
    .bind(filter.target_employee_id)
    .fetch_one(&mut *conn)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let items = sqlx::query_as::<_, CommunicationItemWithName>(
        r#"SELECT c.*, e.name AS target_employee_name
           FROM alc_api.communication_items c
           LEFT JOIN alc_api.employees e ON e.id = c.target_employee_id
           WHERE ($1::BOOLEAN IS NULL OR c.is_active = $1)
             AND ($2::UUID IS NULL OR c.target_employee_id = $2 OR c.target_employee_id IS NULL)
           ORDER BY
             CASE c.priority WHEN 'urgent' THEN 0 WHEN 'normal' THEN 1 ELSE 2 END,
             c.created_at DESC
           LIMIT $3 OFFSET $4"#,
    )
    .bind(filter.is_active)
    .bind(filter.target_employee_id)
    .bind(per_page)
    .bind(offset)
    .fetch_all(&mut *conn)
    .await
    .map_err(|e| {
        tracing::error!("communication_items list error: {:?}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(CommunicationItemsResponse {
        items,
        total,
        page,
        per_page,
    }))
}

/// 有効期間内のアクティブな伝達事項のみ返す (遠隔点呼UI用)
async fn list_active_items(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Query(filter): Query<CommunicationFilter>,
) -> Result<Json<Vec<CommunicationItemWithName>>, StatusCode> {
    let tenant_id = tenant.0 .0;
    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let items = sqlx::query_as::<_, CommunicationItemWithName>(
        r#"SELECT c.*, e.name AS target_employee_name
           FROM alc_api.communication_items c
           LEFT JOIN alc_api.employees e ON e.id = c.target_employee_id
           WHERE c.is_active = true
             AND (c.effective_from IS NULL OR c.effective_from <= now())
             AND (c.effective_until IS NULL OR c.effective_until >= now())
             AND ($1::UUID IS NULL OR c.target_employee_id = $1 OR c.target_employee_id IS NULL)
           ORDER BY
             CASE c.priority WHEN 'urgent' THEN 0 WHEN 'normal' THEN 1 ELSE 2 END,
             c.created_at DESC"#,
    )
    .bind(filter.target_employee_id)
    .fetch_all(&mut *conn)
    .await
    .map_err(|e| {
        tracing::error!("communication_items active error: {:?}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(items))
}

async fn get_item(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Path(id): Path<Uuid>,
) -> Result<Json<CommunicationItem>, StatusCode> {
    let tenant_id = tenant.0 .0;
    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    sqlx::query_as::<_, CommunicationItem>(
        "SELECT * FROM alc_api.communication_items WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .map(Json)
    .ok_or(StatusCode::NOT_FOUND)
}

async fn create_item(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Json(body): Json<CreateCommunicationItem>,
) -> Result<(StatusCode, Json<CommunicationItem>), StatusCode> {
    let tenant_id = tenant.0 .0;
    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let item = sqlx::query_as::<_, CommunicationItem>(
        r#"INSERT INTO alc_api.communication_items
               (tenant_id, title, content, priority, target_employee_id, effective_from, effective_until, created_by)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
           RETURNING *"#,
    )
    .bind(tenant_id)
    .bind(&body.title)
    .bind(body.content.as_deref().unwrap_or(""))
    .bind(body.priority.as_deref().unwrap_or("normal"))
    .bind(body.target_employee_id)
    .bind(body.effective_from)
    .bind(body.effective_until)
    .bind(&body.created_by)
    .fetch_one(&mut *conn)
    .await
    .map_err(|e| {
        tracing::error!("communication_items create error: {:?}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok((StatusCode::CREATED, Json(item)))
}

async fn update_item(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateCommunicationItem>,
) -> Result<Json<CommunicationItem>, StatusCode> {
    let tenant_id = tenant.0 .0;
    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let item = sqlx::query_as::<_, CommunicationItem>(
        r#"UPDATE alc_api.communication_items SET
               title = COALESCE($1, title),
               content = COALESCE($2, content),
               priority = COALESCE($3, priority),
               target_employee_id = CASE WHEN $5 THEN $4 ELSE target_employee_id END,
               is_active = COALESCE($6, is_active),
               effective_from = CASE WHEN $8 THEN $7 ELSE effective_from END,
               effective_until = CASE WHEN $10 THEN $9 ELSE effective_until END,
               updated_at = now()
           WHERE id = $11
           RETURNING *"#,
    )
    .bind(&body.title)
    .bind(&body.content)
    .bind(&body.priority)
    .bind(body.target_employee_id)
    .bind(body.target_employee_id.is_some())
    .bind(body.is_active)
    .bind(body.effective_from)
    .bind(body.effective_from.is_some())
    .bind(body.effective_until)
    .bind(body.effective_until.is_some())
    .bind(id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    match item {
        Some(i) => Ok(Json(i)),
        None => Err(StatusCode::NOT_FOUND),
    }
}

async fn delete_item(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let tenant_id = tenant.0 .0;
    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let result = sqlx::query("DELETE FROM alc_api.communication_items WHERE id = $1")
        .bind(id)
        .execute(&mut *conn)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if result.rows_affected() == 0 {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(StatusCode::NO_CONTENT)
}
