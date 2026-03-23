use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use chrono::NaiveDate;
use uuid::Uuid;

use crate::db::models::{DtakoDailyHoursFilter, DtakoDailyHoursResponse, DtakoDailyWorkHours, DtakoDailyWorkSegment, DtakoSegmentsResponse};
use crate::db::tenant::set_current_tenant;
use crate::middleware::auth::TenantId;
use crate::AppState;

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

    let mut conn = state.pool.acquire().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.to_string()).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let total: (i64,) = sqlx::query_as(
        r#"SELECT COUNT(*)::BIGINT FROM alc_api.dtako_daily_work_hours
           WHERE ($1::UUID IS NULL OR driver_id = $1)
             AND ($2::DATE IS NULL OR work_date >= $2)
             AND ($3::DATE IS NULL OR work_date <= $3)"#,
    )
    .bind(filter.driver_id)
    .bind(filter.date_from)
    .bind(filter.date_to)
    .fetch_one(&mut *conn)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let items = sqlx::query_as::<_, DtakoDailyWorkHours>(
        r#"SELECT * FROM alc_api.dtako_daily_work_hours
           WHERE ($1::UUID IS NULL OR driver_id = $1)
             AND ($2::DATE IS NULL OR work_date >= $2)
             AND ($3::DATE IS NULL OR work_date <= $3)
           ORDER BY work_date ASC
           LIMIT $4 OFFSET $5"#,
    )
    .bind(filter.driver_id)
    .bind(filter.date_from)
    .bind(filter.date_to)
    .bind(per_page)
    .bind(offset)
    .fetch_all(&mut *conn)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(DtakoDailyHoursResponse {
        items,
        total: total.0,
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
    let mut conn = state.pool.acquire().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.to_string()).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let segments = sqlx::query_as::<_, DtakoDailyWorkSegment>(
        r#"SELECT * FROM alc_api.dtako_daily_work_segments
           WHERE driver_id = $1 AND work_date = $2
           ORDER BY start_at"#,
    )
    .bind(driver_id)
    .bind(date)
    .fetch_all(&mut *conn)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(DtakoSegmentsResponse { segments }))
}
