use axum::{
    extract::{Query, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};

use crate::DtakoState;
use alc_core::auth_middleware::TenantId;
use alc_core::repository::dtako_work_times::{WorkTimesFilter, WorkTimesResponse};

pub fn tenant_router<S>() -> Router<S>
where
    DtakoState: axum::extract::FromRef<S>,
    S: Clone + Send + Sync + 'static,
{
    Router::new().route("/work-times", get(list_work_times))
}

async fn list_work_times(
    State(state): State<DtakoState>,
    tenant: axum::Extension<TenantId>,
    Query(filter): Query<WorkTimesFilter>,
) -> Result<Json<WorkTimesResponse>, StatusCode> {
    let tenant_id = tenant.0 .0;
    let page = filter.page.unwrap_or(1).max(1);
    let per_page = filter.per_page.unwrap_or(50).min(200);
    let offset = (page - 1) * per_page;

    let total = state
        .dtako_work_times
        .count(
            tenant_id,
            filter.driver_id,
            filter.date_from,
            filter.date_to,
        )
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let items = state
        .dtako_work_times
        .list(
            tenant_id,
            filter.driver_id,
            filter.date_from,
            filter.date_to,
            per_page,
            offset,
        )
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(WorkTimesResponse {
        items,
        total,
        page,
        per_page,
    }))
}
