use axum::{extract::State, http::StatusCode, routing::get, Json, Router};

use alc_core::auth_middleware::TenantId;
use alc_core::repository::dtako_drivers::Driver;
use alc_core::AppState;

pub fn tenant_router() -> Router<AppState> {
    Router::new().route("/drivers", get(list_drivers))
}

async fn list_drivers(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
) -> Result<Json<Vec<Driver>>, StatusCode> {
    let tenant_id = tenant.0 .0;

    let drivers = state
        .dtako_drivers
        .list(tenant_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(drivers))
}
