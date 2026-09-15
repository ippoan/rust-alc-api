use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Extension, Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::CarinsState;
use alc_core::auth_middleware::TenantId;
use alc_core::repository::car_inspections::normalize_carins_numbers;

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
        .route(
            "/car-inspections/by-car/{car_id}/history",
            get(list_history),
        )
        // 番号を URL (request log) に載せないよう POST。`/{id}` より前に置く
        .route("/car-inspections/lookup", post(lookup))
        .route("/car-inspections/{id}", get(get_by_id))
}

#[derive(Debug, Serialize, ts_rs::TS)]
#[ts(export, rename = "CarInspectionListResponse")]
struct ListResponse {
    #[serde(rename = "carInspections")]
    car_inspections: Vec<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct LookupRequest {
    #[serde(default)]
    cert_no: Option<String>,
    #[serde(default)]
    car_id: Option<String>,
}

/// 車検期限の照合の応答。所有者・住所・車台番号は返さない
#[derive(Debug, Serialize, ts_rs::TS)]
#[ts(export)]
struct CarInspectionLookupResponse {
    /// "YYYY-MM-DD"
    expires_on: Option<chrono::NaiveDate>,
    /// `cert_no` / `car_id` / `none`
    matched_by: String,
    /// 登録番号
    car_no: Option<String>,
}

/// 電子車検証の管理番号 / 車両 ID で車検期限を照合する (運行者端末の車両の段、
/// Refs ippoan/alc-app-s3#110)。番号は tracing に出さない。
async fn lookup(
    State(state): State<CarinsState>,
    Extension(tenant_id): Extension<TenantId>,
    Json(mut body): Json<LookupRequest>,
) -> Result<Json<CarInspectionLookupResponse>, StatusCode> {
    normalize_carins_numbers(&mut body.cert_no, &mut body.car_id)
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    if body.cert_no.is_none() && body.car_id.is_none() {
        return Err(StatusCode::BAD_REQUEST);
    }
    let found = state
        .car_inspections
        .lookup_expiry(tenant_id.0, body.cert_no.as_deref(), body.car_id.as_deref())
        .await
        .map_err(|e| {
            tracing::error!("lookup_expiry failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(CarInspectionLookupResponse {
        expires_on: found.expires_on,
        matched_by: found.matched_by.to_string(),
        car_no: found.car_no,
    }))
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

async fn list_history(
    State(state): State<CarinsState>,
    Extension(tenant_id): Extension<TenantId>,
    Path(car_id): Path<String>,
) -> Result<Json<ListResponse>, StatusCode> {
    let rows = state
        .car_inspections
        .list_by_car_id(tenant_id.0, &car_id)
        .await
        .map_err(|e| {
            tracing::error!("list_by_car_id failed: {e}");
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
