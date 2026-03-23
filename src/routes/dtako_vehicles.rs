use axum::{
    extract::State,
    http::StatusCode,
    routing::get,
    Json, Router,
};

use crate::db::models::DtakoVehicle;
use crate::db::tenant::set_current_tenant;
use crate::middleware::auth::TenantId;
use crate::AppState;

pub fn tenant_router() -> Router<AppState> {
    Router::new().route("/vehicles", get(list_vehicles))
}

async fn list_vehicles(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
) -> Result<Json<Vec<DtakoVehicle>>, StatusCode> {
    let tenant_id = tenant.0 .0;
    let mut conn = state.pool.acquire().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.to_string()).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let vehicles = sqlx::query_as::<_, DtakoVehicle>(
        "SELECT * FROM alc_api.dtako_vehicles ORDER BY vehicle_cd",
    )
    .fetch_all(&mut *conn)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(vehicles))
}
