use axum::{extract::State, http::StatusCode, routing::get, Json, Router};

use crate::DtakoState;
use alc_core::auth_middleware::TenantId;
use alc_core::repository::dtako_drivers::Driver;

pub fn tenant_router<S>() -> Router<S>
where
    DtakoState: axum::extract::FromRef<S>,
    S: Clone + Send + Sync + 'static,
{
    Router::new().route("/drivers", get(list_drivers))
}

async fn list_drivers(
    State(state): State<DtakoState>,
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
