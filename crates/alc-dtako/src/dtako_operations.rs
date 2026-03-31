use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{delete, get},
    Json, Router,
};

use alc_core::auth_middleware::TenantId;
use alc_core::models::DtakoOperationFilter;
use alc_core::AppState;

pub fn tenant_router() -> Router<AppState> {
    Router::new()
        .route("/operations", get(list_operations))
        .route("/operations/calendar", get(calendar_dates))
        .route("/operations/{unko_no}", get(get_operation))
        .route("/operations/{unko_no}", delete(delete_operation))
}

#[derive(serde::Deserialize)]
struct CalendarQuery {
    year: i32,
    month: i32,
}

#[derive(serde::Serialize)]
struct CalendarResponse {
    year: i32,
    month: u32,
    dates: Vec<CalendarDateEntry>,
}

#[derive(serde::Serialize)]
struct CalendarDateEntry {
    date: chrono::NaiveDate,
    count: i64,
}

async fn calendar_dates(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Query(q): Query<CalendarQuery>,
) -> Result<Json<CalendarResponse>, StatusCode> {
    let tenant_id = tenant.0 .0;
    let month = q.month as u32;
    let date_from =
        chrono::NaiveDate::from_ymd_opt(q.year, month, 1).ok_or(StatusCode::BAD_REQUEST)?;
    let date_to = if month == 12 {
        chrono::NaiveDate::from_ymd_opt(q.year + 1, 1, 1)
    } else {
        chrono::NaiveDate::from_ymd_opt(q.year, month + 1, 1)
    }
    .ok_or(StatusCode::BAD_REQUEST)?
    .pred_opt()
    .ok_or(StatusCode::BAD_REQUEST)?;

    let rows = state
        .dtako_operations
        .calendar_dates(tenant_id, date_from, date_to)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let dates: Vec<CalendarDateEntry> = rows
        .into_iter()
        .map(|(date, count)| CalendarDateEntry { date, count })
        .collect();

    Ok(Json(CalendarResponse {
        year: q.year,
        month,
        dates,
    }))
}

async fn list_operations(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Query(filter): Query<DtakoOperationFilter>,
) -> Result<Json<alc_core::models::DtakoOperationsResponse>, StatusCode> {
    let response = state
        .dtako_operations
        .list(tenant.0 .0, &filter)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(response))
}

async fn get_operation(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Path(unko_no): Path<String>,
) -> Result<Json<Vec<alc_core::models::DtakoOperation>>, StatusCode> {
    let ops = state
        .dtako_operations
        .get_by_unko_no(tenant.0 .0, &unko_no)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if ops.is_empty() {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(Json(ops))
}

async fn delete_operation(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Path(unko_no): Path<String>,
) -> Result<StatusCode, StatusCode> {
    let rows_affected = state
        .dtako_operations
        .delete_by_unko_no(tenant.0 .0, &unko_no)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if rows_affected == 0 {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(StatusCode::NO_CONTENT)
}
