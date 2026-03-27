use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use serde::Serialize;
use uuid::Uuid;

use crate::db::models::{
    CarryingItem, DtakoDailyWorkHours, Employee, EmployeeHealthBaseline, EquipmentFailure,
    TenkoRecord,
};
use crate::db::tenant::set_current_tenant;
use crate::middleware::auth::TenantId;
use crate::AppState;

pub fn tenant_router() -> Router<AppState> {
    Router::new().route("/tenko/driver-info/{employee_id}", get(get_driver_info))
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct InstructionSummary {
    session_id: Uuid,
    instruction: String,
    instruction_confirmed_at: Option<chrono::DateTime<chrono::Utc>>,
    recorded_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct DailyInspectionSummary {
    session_id: Uuid,
    daily_inspection: serde_json::Value,
    recorded_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct MeasurementSummary {
    id: Uuid,
    temperature: Option<f64>,
    systolic: Option<i32>,
    diastolic: Option<i32>,
    pulse: Option<i32>,
    measured_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Serialize)]
pub struct DriverInfo {
    // イ 健康状態
    pub health_baseline: Option<EmployeeHealthBaseline>,
    pub recent_measurements: Vec<MeasurementSummary>,

    // ロ 労働時間
    pub working_hours: Vec<DtakoDailyWorkHours>,

    // ハ 指導監督の記録
    pub past_instructions: Vec<InstructionSummary>,

    // ニ 携行品
    pub carrying_items: Vec<CarryingItem>,

    // ホ 乗務員台帳
    pub employee: Employee,

    // ヘ 過去の点呼記録
    pub past_tenko_records: Vec<TenkoRecord>,

    // ト 車両整備状況
    pub recent_daily_inspections: Vec<DailyInspectionSummary>,
    pub equipment_failures: Vec<EquipmentFailure>,
}

async fn get_driver_info(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Path(employee_id): Path<Uuid>,
) -> Result<Json<DriverInfo>, StatusCode> {
    let tenant_id = tenant.0 .0;
    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // ホ 乗務員台帳
    let employee = sqlx::query_as::<_, Employee>(
        "SELECT * FROM alc_api.employees WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(employee_id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;

    // イ 健康基準値
    let health_baseline = sqlx::query_as::<_, EmployeeHealthBaseline>(
        "SELECT * FROM alc_api.employee_health_baselines WHERE employee_id = $1",
    )
    .bind(employee_id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // イ 直近5件の測定値 (tenko_sessions から)
    let recent_measurements = sqlx::query_as::<_, MeasurementSummary>(
        r#"SELECT id, temperature, systolic, diastolic, pulse, medical_measured_at AS measured_at
           FROM alc_api.tenko_sessions
           WHERE employee_id = $1 AND medical_measured_at IS NOT NULL
           ORDER BY medical_measured_at DESC LIMIT 5"#,
    )
    .bind(employee_id)
    .fetch_all(&mut *conn)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // ロ 労働時間 (直近7日)
    let working_hours = sqlx::query_as::<_, DtakoDailyWorkHours>(
        r#"SELECT * FROM alc_api.dtako_daily_work_hours
           WHERE driver_id = $1
           ORDER BY work_date DESC LIMIT 7"#,
    )
    .bind(employee_id)
    .fetch_all(&mut *conn)
    .await
    .unwrap_or_default();

    // ハ 指導監督の記録 (直近10件)
    let past_instructions = sqlx::query_as::<_, InstructionSummary>(
        r#"SELECT session_id, instruction, instruction_confirmed_at, recorded_at
           FROM alc_api.tenko_records
           WHERE employee_id = $1 AND instruction IS NOT NULL AND instruction != ''
           ORDER BY recorded_at DESC LIMIT 10"#,
    )
    .bind(employee_id)
    .fetch_all(&mut *conn)
    .await
    .unwrap_or_default();

    // ニ 携行品マスタ
    let carrying_items = sqlx::query_as::<_, CarryingItem>(
        "SELECT * FROM alc_api.carrying_items ORDER BY sort_order, created_at",
    )
    .fetch_all(&mut *conn)
    .await
    .unwrap_or_default();

    // ヘ 過去の点呼記録 (直近10件)
    let past_tenko_records = sqlx::query_as::<_, TenkoRecord>(
        r#"SELECT * FROM alc_api.tenko_records
           WHERE employee_id = $1
           ORDER BY recorded_at DESC LIMIT 10"#,
    )
    .bind(employee_id)
    .fetch_all(&mut *conn)
    .await
    .unwrap_or_default();

    // ト 直近の日常点検結果 (tenko_records から)
    let recent_daily_inspections = sqlx::query_as::<_, DailyInspectionSummary>(
        r#"SELECT session_id, daily_inspection, recorded_at
           FROM alc_api.tenko_records
           WHERE employee_id = $1 AND daily_inspection IS NOT NULL
           ORDER BY recorded_at DESC LIMIT 5"#,
    )
    .bind(employee_id)
    .fetch_all(&mut *conn)
    .await
    .unwrap_or_default();

    // ト 未解決の機器故障
    let equipment_failures = sqlx::query_as::<_, EquipmentFailure>(
        r#"SELECT * FROM alc_api.equipment_failures
           WHERE resolved_at IS NULL
           ORDER BY reported_at DESC"#,
    )
    .fetch_all(&mut *conn)
    .await
    .unwrap_or_default();

    Ok(Json(DriverInfo {
        health_baseline,
        recent_measurements,
        working_hours,
        past_instructions,
        carrying_items,
        employee,
        past_tenko_records,
        recent_daily_inspections,
        equipment_failures,
    }))
}
