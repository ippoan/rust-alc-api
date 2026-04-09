use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{delete, get},
    Extension, Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::CarinsState;
use alc_core::auth_middleware::TenantId;
use alc_core::models::NfcTag;

pub fn tenant_router<S>() -> Router<S>
where
    CarinsState: axum::extract::FromRef<S>,
    S: Clone + Send + Sync + 'static,
{
    Router::new()
        .route("/nfc-tags", get(list_tags).post(register_tag))
        .route("/nfc-tags/search", get(search_by_uuid))
        .route("/nfc-tags/{nfc_uuid}", delete(delete_tag))
}

fn normalize_nfc_uuid(uuid: &str) -> String {
    uuid.to_lowercase().replace(':', "")
}

#[derive(Debug, Deserialize)]
struct SearchQuery {
    uuid: String,
}

#[derive(Debug, Serialize, ts_rs::TS)]
#[ts(export, rename = "NfcTagSearchResponse")]
struct SearchResponse {
    nfc_tag: NfcTag,
    car_inspection: Option<serde_json::Value>,
}

async fn search_by_uuid(
    State(state): State<CarinsState>,
    Extension(tenant_id): Extension<TenantId>,
    Query(q): Query<SearchQuery>,
) -> Result<Json<SearchResponse>, StatusCode> {
    let nfc_uuid = normalize_nfc_uuid(&q.uuid);

    let tag = state
        .nfc_tags
        .search_by_uuid(tenant_id.0, &nfc_uuid)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    let ci = state
        .nfc_tags
        .get_car_inspection_json(tenant_id.0, tag.car_inspection_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(SearchResponse {
        nfc_tag: tag,
        car_inspection: ci,
    }))
}

#[derive(Debug, Deserialize)]
struct ListQuery {
    car_inspection_id: Option<i32>,
}

async fn list_tags(
    State(state): State<CarinsState>,
    Extension(tenant_id): Extension<TenantId>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Vec<NfcTag>>, StatusCode> {
    let rows = state
        .nfc_tags
        .list(tenant_id.0, q.car_inspection_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(rows))
}

#[derive(Debug, Deserialize)]
struct RegisterRequest {
    nfc_uuid: String,
    car_inspection_id: i32,
}

async fn register_tag(
    State(state): State<CarinsState>,
    Extension(tenant_id): Extension<TenantId>,
    Json(body): Json<RegisterRequest>,
) -> Result<(StatusCode, Json<NfcTag>), StatusCode> {
    let nfc_uuid = normalize_nfc_uuid(&body.nfc_uuid);

    let tag = state
        .nfc_tags
        .register(tenant_id.0, &nfc_uuid, body.car_inspection_id)
        .await
        .map_err(|e| {
            tracing::error!("register_tag failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok((StatusCode::CREATED, Json(tag)))
}

async fn delete_tag(
    State(state): State<CarinsState>,
    Extension(tenant_id): Extension<TenantId>,
    Path(nfc_uuid): Path<String>,
) -> Result<StatusCode, StatusCode> {
    let normalized = normalize_nfc_uuid(&nfc_uuid);

    let deleted = state
        .nfc_tags
        .delete(tenant_id.0, &normalized)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if !deleted {
        return Err(StatusCode::NOT_FOUND);
    }

    Ok(StatusCode::NO_CONTENT)
}
