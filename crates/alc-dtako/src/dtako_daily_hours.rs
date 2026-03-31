use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use chrono::NaiveDate;
use uuid::Uuid;

use alc_core::auth_middleware::TenantId;
use alc_core::models::{DtakoDailyHoursFilter, DtakoDailyHoursResponse, DtakoSegmentsResponse};
use alc_core::AppState;

pub fn tenant_router() -> Router<AppState> {
    Router::new()
        .route("/daily-hours", get(list_daily_hours))
        .route(
            "/daily-hours/{driver_id}/{date}/segments",
            get(get_daily_segments),
        )
}

async fn list_daily_hours(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Query(filter): Query<DtakoDailyHoursFilter>,
) -> Result<Json<DtakoDailyHoursResponse>, StatusCode> {
    let tenant_id = tenant.0 .0;
    let page = filter.page.unwrap_or(1).max(1);
    let per_page = filter.per_page.unwrap_or(50).min(200);
    let offset = (page - 1) * per_page;

    let total = state
        .dtako_daily_hours
        .count(
            tenant_id,
            filter.driver_id,
            filter.date_from,
            filter.date_to,
        )
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let items = state
        .dtako_daily_hours
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

    Ok(Json(DtakoDailyHoursResponse {
        items,
        total,
        page,
        per_page,
    }))
}

async fn get_daily_segments(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Path((driver_id, date)): Path<(Uuid, NaiveDate)>,
) -> Result<Json<DtakoSegmentsResponse>, StatusCode> {
    let tenant_id = tenant.0 .0;

    let segments = state
        .dtako_daily_hours
        .get_segments(tenant_id, driver_id, date)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(DtakoSegmentsResponse { segments }))
}
