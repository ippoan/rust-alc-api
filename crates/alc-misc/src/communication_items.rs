use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use alc_core::auth_middleware::TenantId;
use alc_core::models::{CommunicationItem, CreateCommunicationItem, UpdateCommunicationItem};
use alc_core::repository::communication_items::CommunicationItemWithName;
use alc_core::AppState;

pub fn tenant_router() -> Router<AppState> {
    Router::new()
        .route("/communication-items", get(list_items).post(create_item))
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

async fn list_items(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Query(filter): Query<CommunicationFilter>,
) -> Result<Json<CommunicationItemsResponse>, StatusCode> {
    let tenant_id = tenant.0 .0;
    let page = filter.page.unwrap_or(1).max(1);
    let per_page = filter.per_page.unwrap_or(20).min(100);
    let offset = (page - 1) * per_page;

    let (items, total) = state
        .communication_items
        .list(
            tenant_id,
            filter.is_active,
            filter.target_employee_id,
            per_page,
            offset,
        )
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

    let items = state
        .communication_items
        .list_active(tenant_id, filter.target_employee_id)
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

    state
        .communication_items
        .get(tenant_id, id)
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

    let item = state
        .communication_items
        .create(tenant_id, &body)
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

    match state
        .communication_items
        .update(tenant_id, id, &body)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    {
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

    let deleted = state
        .communication_items
        .delete(tenant_id, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if !deleted {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(StatusCode::NO_CONTENT)
}
