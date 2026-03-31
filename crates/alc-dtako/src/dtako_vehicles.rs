use axum::{extract::State, http::StatusCode, routing::get, Json, Router};

use alc_core::auth_middleware::TenantId;
use alc_core::models::DtakoVehicle;
use alc_core::AppState;

pub fn tenant_router() -> Router<AppState> {
    Router::new().route("/vehicles", get(list_vehicles))
}

async fn list_vehicles(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
) -> Result<Json<Vec<DtakoVehicle>>, StatusCode> {
    let tenant_id = tenant.0 .0;

    let vehicles = state
        .dtako_vehicles
        .list(tenant_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(vehicles))
}
