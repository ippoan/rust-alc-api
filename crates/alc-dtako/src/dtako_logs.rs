use axum::{
    extract::{Query, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};

use alc_core::auth_middleware::TenantId;
use alc_core::models::{
    DtakologDateQuery, DtakologDateRangeQuery, DtakologSelectQuery, DtakologView,
};
use alc_core::AppState;

pub fn tenant_router() -> Router<AppState> {
    Router::new()
        .route("/current", get(current_list_all))
        .route("/by-date", get(get_by_date))
        .route("/current/select", get(current_list_select))
        .route("/by-date-range", get(get_by_date_range))
}

async fn current_list_all(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
) -> Result<Json<Vec<DtakologView>>, StatusCode> {
    let rows = state
        .dtako_logs
        .current_list_all(tenant.0 .0)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(rows.into_iter().map(DtakologView::from).collect()))
}

async fn get_by_date(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Query(q): Query<DtakologDateQuery>,
) -> Result<Json<Vec<DtakologView>>, StatusCode> {
    let rows = state
        .dtako_logs
        .get_date(tenant.0 .0, &q.date_time, q.vehicle_cd)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(rows.into_iter().map(DtakologView::from).collect()))
}

async fn current_list_select(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Query(q): Query<DtakologSelectQuery>,
) -> Result<Json<Vec<DtakologView>>, StatusCode> {
    let vehicle_cds: Vec<i32> = q
        .vehicle_cds
        .as_deref()
        .unwrap_or("")
        .split(',')
        .filter_map(|s| s.trim().parse().ok())
        .collect();
    let rows = state
        .dtako_logs
        .current_list_select(
            tenant.0 .0,
            q.address_disp_p.as_deref(),
            q.branch_cd,
            &vehicle_cds,
        )
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(rows.into_iter().map(DtakologView::from).collect()))
}

async fn get_by_date_range(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Query(q): Query<DtakologDateRangeQuery>,
) -> Result<Json<Vec<DtakologView>>, StatusCode> {
    let rows = state
        .dtako_logs
        .get_date_range(
            tenant.0 .0,
            &q.start_date_time,
            &q.end_date_time,
            q.vehicle_cd,
        )
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(rows.into_iter().map(DtakologView::from).collect()))
}
