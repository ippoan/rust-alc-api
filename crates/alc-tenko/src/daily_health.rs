use axum::{
    extract::{Query, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use chrono::{NaiveDate, Utc};
use serde::{Deserialize, Serialize};

use alc_core::auth_middleware::TenantId;
use alc_core::repository::daily_health::DailyHealthRow;
use alc_core::AppState;

pub fn tenant_router() -> Router<AppState> {
    Router::new().route("/tenko/daily-health-status", get(daily_health_status))
}

#[derive(Debug, Deserialize)]
struct DailyHealthFilter {
    date: Option<NaiveDate>,
}

#[derive(Debug, Serialize)]
struct DailyHealthSummary {
    total_employees: i64,
    checked_count: i64,
    unchecked_count: i64,
    pass_count: i64,
    fail_count: i64,
}

#[derive(Debug, Serialize)]
struct DailyHealthResponse {
    date: NaiveDate,
    employees: Vec<DailyHealthRow>,
    summary: DailyHealthSummary,
}

async fn daily_health_status(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Query(filter): Query<DailyHealthFilter>,
) -> Result<Json<DailyHealthResponse>, StatusCode> {
    let tenant_id = tenant.0 .0;
    let date = filter.date.unwrap_or_else(|| {
        // JST (UTC+9) の今日
        (Utc::now() + chrono::Duration::hours(9)).date_naive()
    });

    let rows = state
        .daily_health
        .fetch_daily_health(tenant_id, date)
        .await
        .map_err(|e| {
            tracing::error!("daily_health_status query error: {:?}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let total = rows.len() as i64;
    let checked = rows.iter().filter(|r| r.session_id.is_some()).count() as i64;
    let pass = rows
        .iter()
        .filter(|r| {
            r.safety_judgment
                .as_ref()
                .and_then(|j| j.get("status"))
                .and_then(|s| s.as_str())
                == Some("pass")
        })
        .count() as i64;
    let fail = rows
        .iter()
        .filter(|r| {
            r.safety_judgment
                .as_ref()
                .and_then(|j| j.get("status"))
                .and_then(|s| s.as_str())
                == Some("fail")
        })
        .count() as i64;

    Ok(Json(DailyHealthResponse {
        date,
        employees: rows,
        summary: DailyHealthSummary {
            total_employees: total,
            checked_count: checked,
            unchecked_count: total - checked,
            pass_count: pass,
            fail_count: fail,
        },
    }))
}
