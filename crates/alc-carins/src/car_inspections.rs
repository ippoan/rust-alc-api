use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::get,
    Extension, Json, Router,
};
use serde::Serialize;

use crate::CarinsState;
use alc_core::auth_middleware::TenantId;

pub fn tenant_router<S>() -> Router<S>
where
    CarinsState: axum::extract::FromRef<S>,
    S: Clone + Send + Sync + 'static,
{
    Router::new()
        .route("/car-inspections/current", get(list_current))
        .route("/car-inspections/expired", get(list_expired))
        .route("/car-inspections/renew", get(list_renew))
        .route(
            "/car-inspections/vehicle-categories",
            get(vehicle_categories),
        )
        .route("/car-inspections/{id}", get(get_by_id))
}

#[derive(Debug, Serialize, ts_rs::TS)]
#[ts(export, rename = "CarInspectionListResponse")]
struct ListResponse {
    #[serde(rename = "carInspections")]
    car_inspections: Vec<serde_json::Value>,
}

async fn list_current(
    State(state): State<CarinsState>,
    Extension(tenant_id): Extension<TenantId>,
) -> Result<Json<ListResponse>, StatusCode> {
    let rows = state
        .car_inspections
        .list_current(tenant_id.0)
        .await
        .map_err(|e| {
            tracing::error!("list_current failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(ListResponse {
        car_inspections: rows,
    }))
}

async fn get_by_id(
    State(state): State<CarinsState>,
    Extension(tenant_id): Extension<TenantId>,
    Path(id): Path<i32>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let row = state
        .car_inspections
        .get_by_id(tenant_id.0, id)
        .await
        .map_err(|e| {
            tracing::error!("get_by_id failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(row))
}

async fn vehicle_categories(
    State(state): State<CarinsState>,
    Extension(tenant_id): Extension<TenantId>,
) -> Result<Json<alc_core::repository::car_inspections::VehicleCategories>, StatusCode> {
    let row = state
        .car_inspections
        .vehicle_categories(tenant_id.0)
        .await
        .map_err(|e| {
            tracing::error!("vehicle_categories failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(row))
}

async fn list_expired(
    State(state): State<CarinsState>,
    Extension(tenant_id): Extension<TenantId>,
) -> Result<Json<ListResponse>, StatusCode> {
    let rows = state
        .car_inspections
        .list_expired(tenant_id.0)
        .await
        .map_err(|e| {
            tracing::error!("list_expired failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(ListResponse {
        car_inspections: rows,
    }))
}

async fn list_renew(
    State(state): State<CarinsState>,
    Extension(tenant_id): Extension<TenantId>,
) -> Result<Json<ListResponse>, StatusCode> {
    let rows = state
        .car_inspections
        .list_renew(tenant_id.0)
        .await
        .map_err(|e| {
            tracing::error!("list_renew failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(ListResponse {
        car_inspections: rows,
    }))
}
