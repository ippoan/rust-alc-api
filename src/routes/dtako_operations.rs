use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{delete, get},
    Json, Router,
};

use crate::db::models::{DtakoOperation, DtakoOperationFilter, DtakoOperationListItem, DtakoOperationsResponse};
use crate::db::tenant::set_current_tenant;
use crate::middleware::auth::TenantId;
use crate::AppState;

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

    let mut conn = state.pool.acquire().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.to_string()).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let rows = sqlx::query_as::<_, (chrono::NaiveDate, i64)>(
        r#"SELECT reading_date, COUNT(*)::BIGINT
           FROM alc_api.dtako_operations
           WHERE reading_date >= $1 AND reading_date <= $2
           GROUP BY reading_date
           ORDER BY reading_date"#,
    )
    .bind(date_from)
    .bind(date_to)
    .fetch_all(&mut *conn)
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
) -> Result<Json<DtakoOperationsResponse>, StatusCode> {
    let tenant_id = tenant.0 .0;
    let page = filter.page.unwrap_or(1).max(1);
    let per_page = filter.per_page.unwrap_or(50).min(200);
    let offset = (page - 1) * per_page;

    let mut conn = state.pool.acquire().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.to_string()).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let total: (i64,) = sqlx::query_as(
        r#"SELECT COUNT(*)::BIGINT FROM alc_api.dtako_operations o
           LEFT JOIN alc_api.employees d ON o.driver_id = d.id
           LEFT JOIN alc_api.dtako_vehicles v ON o.vehicle_id = v.id
           WHERE ($1::DATE IS NULL OR o.reading_date >= $1)
             AND ($2::DATE IS NULL OR o.reading_date <= $2)
             AND ($3::TEXT IS NULL OR d.driver_cd = $3)
             AND ($4::TEXT IS NULL OR v.vehicle_cd = $4)"#,
    )
    .bind(filter.date_from)
    .bind(filter.date_to)
    .bind(&filter.driver_cd)
    .bind(&filter.vehicle_cd)
    .fetch_one(&mut *conn)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let operations = sqlx::query_as::<_, DtakoOperationListItem>(
        r#"SELECT o.id, o.unko_no, o.crew_role, o.reading_date, o.operation_date,
                  d.name AS driver_name, v.vehicle_name,
                  o.total_distance, o.safety_score, o.economy_score, o.total_score,
                  o.has_kudgivt
           FROM alc_api.dtako_operations o
           LEFT JOIN alc_api.employees d ON o.driver_id = d.id
           LEFT JOIN alc_api.dtako_vehicles v ON o.vehicle_id = v.id
           WHERE ($1::DATE IS NULL OR o.reading_date >= $1)
             AND ($2::DATE IS NULL OR o.reading_date <= $2)
             AND ($3::TEXT IS NULL OR d.driver_cd = $3)
             AND ($4::TEXT IS NULL OR v.vehicle_cd = $4)
           ORDER BY o.reading_date DESC, o.unko_no
           LIMIT $5 OFFSET $6"#,
    )
    .bind(filter.date_from)
    .bind(filter.date_to)
    .bind(&filter.driver_cd)
    .bind(&filter.vehicle_cd)
    .bind(per_page)
    .bind(offset)
    .fetch_all(&mut *conn)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(DtakoOperationsResponse {
        operations,
        total: total.0,
        page,
        per_page,
    }))
}

async fn get_operation(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Path(unko_no): Path<String>,
) -> Result<Json<Vec<DtakoOperation>>, StatusCode> {
    let tenant_id = tenant.0 .0;
    let mut conn = state.pool.acquire().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.to_string()).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let ops = sqlx::query_as::<_, DtakoOperation>(
        "SELECT * FROM alc_api.dtako_operations WHERE unko_no = $1 ORDER BY crew_role",
    )
    .bind(&unko_no)
    .fetch_all(&mut *conn)
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
    let tenant_id = tenant.0 .0;
    let mut conn = state.pool.acquire().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.to_string()).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let result = sqlx::query("DELETE FROM alc_api.dtako_operations WHERE unko_no = $1")
        .bind(&unko_no)
        .execute(&mut *conn)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if result.rows_affected() == 0 {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(StatusCode::NO_CONTENT)
}
