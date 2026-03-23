use axum::{
    extract::{Query, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use chrono::{NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::db::tenant::set_current_tenant;
use crate::middleware::auth::TenantId;
use crate::AppState;

pub fn tenant_router() -> Router<AppState> {
    Router::new().route("/tenko/daily-health-status", get(daily_health_status))
}

#[derive(Debug, Deserialize)]
struct DailyHealthFilter {
    date: Option<NaiveDate>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
struct DailyHealthRow {
    employee_id: Uuid,
    employee_name: String,
    employee_code: Option<String>,
    // Session
    session_id: Option<Uuid>,
    tenko_type: Option<String>,
    completed_at: Option<chrono::DateTime<Utc>>,
    // Medical
    temperature: Option<f64>,
    systolic: Option<i32>,
    diastolic: Option<i32>,
    pulse: Option<i32>,
    medical_measured_at: Option<chrono::DateTime<Utc>>,
    medical_manual_input: Option<bool>,
    // Alcohol
    alcohol_result: Option<String>,
    alcohol_value: Option<f64>,
    // JSONB
    self_declaration: Option<serde_json::Value>,
    safety_judgment: Option<serde_json::Value>,
    // Baseline
    has_baseline: Option<bool>,
    baseline_systolic: Option<i32>,
    baseline_diastolic: Option<i32>,
    baseline_temperature: Option<f64>,
    systolic_tolerance: Option<i32>,
    diastolic_tolerance: Option<i32>,
    temperature_tolerance: Option<f64>,
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

    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let rows = sqlx::query_as::<_, DailyHealthRow>(
        r#"
        SELECT
            e.id AS employee_id,
            e.name AS employee_name,
            e.code AS employee_code,
            s.id AS session_id,
            s.tenko_type,
            s.completed_at,
            s.temperature,
            s.systolic,
            s.diastolic,
            s.pulse,
            s.medical_measured_at,
            s.medical_manual_input,
            s.alcohol_result,
            s.alcohol_value,
            s.self_declaration,
            s.safety_judgment,
            (b.id IS NOT NULL) AS has_baseline,
            b.baseline_systolic,
            b.baseline_diastolic,
            b.baseline_temperature,
            b.systolic_tolerance,
            b.diastolic_tolerance,
            b.temperature_tolerance
        FROM alc_api.employees e
        LEFT JOIN LATERAL (
            SELECT *
            FROM alc_api.tenko_sessions ts
            WHERE ts.employee_id = e.id
              AND ts.tenant_id = $1
              AND ts.status = 'completed'
              AND ts.completed_at >= ($2::date - INTERVAL '9 hours')
              AND ts.completed_at < ($2::date + INTERVAL '15 hours')
            ORDER BY ts.completed_at DESC
            LIMIT 1
        ) s ON true
        LEFT JOIN alc_api.employee_health_baselines b
            ON b.employee_id = e.id AND b.tenant_id = $1
        WHERE e.tenant_id = $1
          AND e.deleted_at IS NULL
        ORDER BY
            CASE
                WHEN s.id IS NOT NULL AND (s.safety_judgment->>'status') = 'fail' THEN 0
                WHEN s.id IS NULL THEN 1
                ELSE 2
            END,
            e.name
        "#,
    )
    .bind(tenant_id)
    .bind(date)
    .fetch_all(&mut *conn)
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
