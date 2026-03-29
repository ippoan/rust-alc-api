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
use crate::db::repository::driver_info::{DriverInfoRepository, PgDriverInfoRepository};
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
    let repo = PgDriverInfoRepository::new(state.pool.clone());

    // ホ 乗務員台帳
    let employee = repo
        .get_employee(tenant_id, employee_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    // イ 健康基準値
    let health_baseline = repo
        .get_health_baseline(tenant_id, employee_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // イ 直近5件の測定値 (tenko_sessions から)
    let recent_measurements = repo
        .get_recent_measurements(tenant_id, employee_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // ロ 労働時間 (直近7日)
    let working_hours = repo
        .get_working_hours(tenant_id, employee_id)
        .await
        .unwrap_or_default();

    // ハ 指導監督の記録 (直近10件)
    let past_instructions = repo
        .get_past_instructions(tenant_id, employee_id)
        .await
        .unwrap_or_default();

    // ニ 携行品マスタ
    let carrying_items = repo.get_carrying_items(tenant_id).await.unwrap_or_default();

    // ヘ 過去の点呼記録 (直近10件)
    let past_tenko_records = repo
        .get_past_tenko_records(tenant_id, employee_id)
        .await
        .unwrap_or_default();

    // ト 直近の日常点検結果 (tenko_records から)
    let recent_daily_inspections = repo
        .get_recent_daily_inspections(tenant_id, employee_id)
        .await
        .unwrap_or_default();

    // ト 未解決の機器故障
    let equipment_failures = repo
        .get_equipment_failures(tenant_id)
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
