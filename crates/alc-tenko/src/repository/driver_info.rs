use async_trait::async_trait;
use serde::Serialize;
use uuid::Uuid;

use alc_core::models::{CarryingItem, DtakoDailyWorkHours, Employee};

use crate::models::{EmployeeHealthBaseline, EquipmentFailure, TenkoRecord};

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct InstructionSummary {
    pub session_id: Uuid,
    pub instruction: String,
    pub instruction_confirmed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub recorded_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct DailyInspectionSummary {
    pub session_id: Uuid,
    pub daily_inspection: serde_json::Value,
    pub recorded_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct MeasurementSummary {
    pub id: Uuid,
    pub temperature: Option<f64>,
    pub systolic: Option<i32>,
    pub diastolic: Option<i32>,
    pub pulse: Option<i32>,
    pub measured_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// 整備記録 1 件 (`alc_api.maintenance_records`)。ト「運行に使用する事業用自動車の
/// 整備状況」の表示用に、車両テーブルへ JOIN せず必要な列だけを持つ (Refs #651)。
/// `alc-maintenance` の `MaintenanceRecord` とは別物 — 兄弟 crate に依存しないため
/// `alc-tenko` 側で独自に持つ。`cost` は `NUMERIC(12,2)` を `::text` キャストして
/// 文字列で保持する (`alc-maintenance::models::MaintenanceRecord` と同じ作法)。
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct MaintenanceRecordSummary {
    pub id: Uuid,
    pub vehicle_id: Uuid,
    pub category_name: String,
    pub performed_on: chrono::NaiveDate,
    pub odometer_km: Option<i32>,
    pub vendor: Option<String>,
    pub description: Option<String>,
    pub cost: Option<String>,
    pub next_due_on: Option<chrono::NaiveDate>,
}

#[async_trait]
pub trait DriverInfoRepository: Send + Sync {
    async fn get_employee(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
    ) -> Result<Option<Employee>, sqlx::Error>;

    async fn get_health_baseline(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
    ) -> Result<Option<EmployeeHealthBaseline>, sqlx::Error>;

    async fn get_recent_measurements(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
    ) -> Result<Vec<MeasurementSummary>, sqlx::Error>;

    async fn get_working_hours(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
    ) -> Result<Vec<DtakoDailyWorkHours>, sqlx::Error>;

    async fn get_past_instructions(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
    ) -> Result<Vec<InstructionSummary>, sqlx::Error>;

    async fn get_carrying_items(&self, tenant_id: Uuid) -> Result<Vec<CarryingItem>, sqlx::Error>;

    async fn get_past_tenko_records(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
    ) -> Result<Vec<TenkoRecord>, sqlx::Error>;

    async fn get_recent_daily_inspections(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
    ) -> Result<Vec<DailyInspectionSummary>, sqlx::Error>;

    async fn get_equipment_failures(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<EquipmentFailure>, sqlx::Error>;

    /// ト 直近の車両整備記録。`employee_id` の直近の点呼セッションが持つ
    /// `carins_vehicle_id` から `maintenance_vehicles` → `maintenance_records` を
    /// 解決する (Refs #651)。carins を紐づけていないテナント/車両では解決しないのが
    /// 仕様で、その場合は空配列を返す (呼び出し側は `unwrap_or_default()` の作法)。
    async fn get_recent_maintenance_records(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
    ) -> Result<Vec<MaintenanceRecordSummary>, sqlx::Error>;
}
