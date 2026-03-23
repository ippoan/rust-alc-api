use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get, post, put},
    Json, Router,
};
use uuid::Uuid;

use crate::db::models::{CarryingItem, CreateCarryingItem, UpdateCarryingItem};
use crate::db::tenant::set_current_tenant;
use crate::middleware::auth::TenantId;
use crate::AppState;

pub fn tenant_router() -> Router<AppState> {
    Router::new()
        .route("/carrying-items", get(list_items).post(create_item))
        .route("/carrying-items/{id}", put(update_item).delete(delete_item))
}

async fn list_items(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
) -> Result<Json<Vec<CarryingItem>>, StatusCode> {
    let tenant_id = tenant.0 .0;
    let mut conn = state.pool.acquire().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.to_string()).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let items = sqlx::query_as::<_, CarryingItem>(
        "SELECT * FROM alc_api.carrying_items ORDER BY sort_order, created_at",
    )
    .fetch_all(&mut *conn)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(items))
}

async fn create_item(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Json(body): Json<CreateCarryingItem>,
) -> Result<(StatusCode, Json<CarryingItem>), StatusCode> {
    let tenant_id = tenant.0 .0;
    let mut conn = state.pool.acquire().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.to_string()).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let item = sqlx::query_as::<_, CarryingItem>(
        r#"INSERT INTO alc_api.carrying_items (tenant_id, item_name, is_required, sort_order)
           VALUES ($1, $2, $3, $4)
           RETURNING *"#,
    )
    .bind(tenant_id)
    .bind(&body.item_name)
    .bind(body.is_required.unwrap_or(true))
    .bind(body.sort_order.unwrap_or(0))
    .fetch_one(&mut *conn)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok((StatusCode::CREATED, Json(item)))
}

async fn update_item(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateCarryingItem>,
) -> Result<Json<CarryingItem>, StatusCode> {
    let tenant_id = tenant.0 .0;
    let mut conn = state.pool.acquire().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.to_string()).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let item = sqlx::query_as::<_, CarryingItem>(
        r#"UPDATE alc_api.carrying_items SET
               item_name = COALESCE($1, item_name),
               is_required = COALESCE($2, is_required),
               sort_order = COALESCE($3, sort_order)
           WHERE id = $4
           RETURNING *"#,
    )
    .bind(&body.item_name)
    .bind(body.is_required)
    .bind(body.sort_order)
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
    let mut conn = state.pool.acquire().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.to_string()).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let result = sqlx::query("DELETE FROM alc_api.carrying_items WHERE id = $1")
        .bind(id)
        .execute(&mut *conn)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if result.rows_affected() == 0 {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(StatusCode::NO_CONTENT)
}
