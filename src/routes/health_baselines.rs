use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use uuid::Uuid;

use crate::db::models::{CreateHealthBaseline, EmployeeHealthBaseline, UpdateHealthBaseline};
use crate::db::tenant::set_current_tenant;
use crate::middleware::auth::TenantId;
use crate::AppState;

/// テナント対応ルート (JWT or X-Tenant-ID)
pub fn tenant_router() -> Router<AppState> {
    Router::new()
        .route(
            "/tenko/health-baselines",
            post(upsert_baseline).get(list_baselines),
        )
        .route(
            "/tenko/health-baselines/{employee_id}",
            get(get_baseline).put(update_baseline).delete(delete_baseline),
        )
}

/// 基準値作成/更新 (UPSERT)
async fn upsert_baseline(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Json(body): Json<CreateHealthBaseline>,
) -> Result<(StatusCode, Json<EmployeeHealthBaseline>), StatusCode> {
    let tenant_id = tenant.0 .0;

    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let baseline = sqlx::query_as::<_, EmployeeHealthBaseline>(
        r#"
        INSERT INTO employee_health_baselines (
            tenant_id, employee_id,
            baseline_systolic, baseline_diastolic, baseline_temperature,
            systolic_tolerance, diastolic_tolerance, temperature_tolerance,
            measurement_validity_minutes
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        ON CONFLICT (tenant_id, employee_id)
        DO UPDATE SET
            baseline_systolic = EXCLUDED.baseline_systolic,
            baseline_diastolic = EXCLUDED.baseline_diastolic,
            baseline_temperature = EXCLUDED.baseline_temperature,
            systolic_tolerance = EXCLUDED.systolic_tolerance,
            diastolic_tolerance = EXCLUDED.diastolic_tolerance,
            temperature_tolerance = EXCLUDED.temperature_tolerance,
            measurement_validity_minutes = EXCLUDED.measurement_validity_minutes,
            updated_at = NOW()
        RETURNING *
        "#,
    )
    .bind(tenant_id)
    .bind(body.employee_id)
    .bind(body.baseline_systolic.unwrap_or(120))
    .bind(body.baseline_diastolic.unwrap_or(80))
    .bind(body.baseline_temperature.unwrap_or(36.5))
    .bind(body.systolic_tolerance.unwrap_or(10))
    .bind(body.diastolic_tolerance.unwrap_or(10))
    .bind(body.temperature_tolerance.unwrap_or(0.5))
    .bind(body.measurement_validity_minutes.unwrap_or(30))
    .fetch_one(&mut *conn)
    .await
    .map_err(|e| {
        tracing::error!("upsert_baseline DB error: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok((StatusCode::CREATED, Json(baseline)))
}

/// テナント内一覧
async fn list_baselines(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
) -> Result<Json<Vec<EmployeeHealthBaseline>>, StatusCode> {
    let tenant_id = tenant.0 .0;

    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let baselines = sqlx::query_as::<_, EmployeeHealthBaseline>(
        "SELECT * FROM employee_health_baselines WHERE tenant_id = $1 ORDER BY created_at DESC",
    )
    .bind(tenant_id)
    .fetch_all(&mut *conn)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(baselines))
}

/// 個別取得 (employee_id で検索)
async fn get_baseline(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Path(employee_id): Path<Uuid>,
) -> Result<Json<EmployeeHealthBaseline>, StatusCode> {
    let tenant_id = tenant.0 .0;

    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let baseline = sqlx::query_as::<_, EmployeeHealthBaseline>(
        "SELECT * FROM employee_health_baselines WHERE tenant_id = $1 AND employee_id = $2",
    )
    .bind(tenant_id)
    .bind(employee_id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(baseline))
}

/// 基準値更新
async fn update_baseline(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Path(employee_id): Path<Uuid>,
    Json(body): Json<UpdateHealthBaseline>,
) -> Result<Json<EmployeeHealthBaseline>, StatusCode> {
    let tenant_id = tenant.0 .0;

    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let baseline = sqlx::query_as::<_, EmployeeHealthBaseline>(
        r#"
        UPDATE employee_health_baselines SET
            baseline_systolic = COALESCE($3, baseline_systolic),
            baseline_diastolic = COALESCE($4, baseline_diastolic),
            baseline_temperature = COALESCE($5, baseline_temperature),
            systolic_tolerance = COALESCE($6, systolic_tolerance),
            diastolic_tolerance = COALESCE($7, diastolic_tolerance),
            temperature_tolerance = COALESCE($8, temperature_tolerance),
            measurement_validity_minutes = COALESCE($9, measurement_validity_minutes),
            updated_at = NOW()
        WHERE tenant_id = $1 AND employee_id = $2
        RETURNING *
        "#,
    )
    .bind(tenant_id)
    .bind(employee_id)
    .bind(body.baseline_systolic)
    .bind(body.baseline_diastolic)
    .bind(body.baseline_temperature)
    .bind(body.systolic_tolerance)
    .bind(body.diastolic_tolerance)
    .bind(body.temperature_tolerance)
    .bind(body.measurement_validity_minutes)
    .fetch_optional(&mut *conn)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(baseline))
}

/// 基準値削除
async fn delete_baseline(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Path(employee_id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let tenant_id = tenant.0 .0;

    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let result = sqlx::query(
        "DELETE FROM employee_health_baselines WHERE tenant_id = $1 AND employee_id = $2",
    )
    .bind(tenant_id)
    .bind(employee_id)
    .execute(&mut *conn)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if result.rows_affected() == 0 {
        return Err(StatusCode::NOT_FOUND);
    }

    Ok(StatusCode::NO_CONTENT)
}
