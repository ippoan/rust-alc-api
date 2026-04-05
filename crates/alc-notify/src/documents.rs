use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json, Router,
};

use alc_core::auth_middleware::TenantId;
use alc_core::AppState;

pub fn tenant_router() -> Router<AppState> {
    Router::new()
        .route("/notify/documents", axum::routing::get(list))
        .route("/notify/documents/search", axum::routing::get(search))
        .route("/notify/documents/{id}", axum::routing::get(get))
}

#[derive(serde::Deserialize)]
struct ListQuery {
    limit: Option<i64>,
    offset: Option<i64>,
}

async fn list(
    State(state): State<AppState>,
    Extension(tenant): Extension<TenantId>,
    Query(q): Query<ListQuery>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let docs = state
        .notify_documents
        .list(tenant.0, q.limit.unwrap_or(50), q.offset.unwrap_or(0))
        .await
        .map_err(|e| {
            tracing::error!("list notify_documents: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(Json(serde_json::to_value(docs).unwrap()))
}

#[derive(serde::Deserialize)]
struct SearchQuery {
    q: String,
}

async fn search(
    State(state): State<AppState>,
    Extension(tenant): Extension<TenantId>,
    Query(sq): Query<SearchQuery>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let docs = state
        .notify_documents
        .search(tenant.0, &sq.q)
        .await
        .map_err(|e| {
            tracing::error!("search notify_documents: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(Json(serde_json::to_value(docs).unwrap()))
}

async fn get(
    State(state): State<AppState>,
    Extension(tenant): Extension<TenantId>,
    Path(id): Path<uuid::Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let doc = state
        .notify_documents
        .get(tenant.0, id)
        .await
        .map_err(|e| {
            tracing::error!("get notify_document: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    let deliveries = state
        .notify_deliveries
        .list_by_document(tenant.0, id)
        .await
        .map_err(|e| {
            tracing::error!("list deliveries for document: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(serde_json::json!({
        "document": doc,
        "deliveries": deliveries,
    })))
}
