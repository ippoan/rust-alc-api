use axum::{extract::State, http::StatusCode, routing::get, Extension, Json, Router};
use serde::Serialize;

use crate::CarinsState;
use alc_core::auth_middleware::TenantId;
use alc_core::repository::car_inspections::CarInspectionFile;

pub fn tenant_router<S>() -> Router<S>
where
    CarinsState: axum::extract::FromRef<S>,
    S: Clone + Send + Sync + 'static,
{
    Router::new().route("/car-inspection-files/current", get(list_current))
}

#[derive(Debug, Serialize)]
struct ListResponse {
    files: Vec<CarInspectionFile>,
}

async fn list_current(
    State(state): State<CarinsState>,
    Extension(tenant_id): Extension<TenantId>,
) -> Result<Json<ListResponse>, StatusCode> {
    let rows = state
        .car_inspections
        .list_current_files(tenant_id.0)
        .await
        .map_err(|e| {
            tracing::error!("list_current_car_inspection_files failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(ListResponse { files: rows }))
}
