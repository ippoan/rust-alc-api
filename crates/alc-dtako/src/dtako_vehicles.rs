use axum::{extract::State, http::StatusCode, routing::get, Json, Router};

use crate::DtakoState;
use alc_core::auth_middleware::TenantId;
use alc_core::models::DtakoVehicle;

pub fn tenant_router<S>() -> Router<S>
where
    DtakoState: axum::extract::FromRef<S>,
    S: Clone + Send + Sync + 'static,
{
    Router::new().route("/vehicles", get(list_vehicles))
}

async fn list_vehicles(
    State(state): State<DtakoState>,
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
