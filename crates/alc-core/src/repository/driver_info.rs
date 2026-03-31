use async_trait::async_trait;
use serde::Serialize;
use uuid::Uuid;

use crate::models::{
    CarryingItem, DtakoDailyWorkHours, Employee, EmployeeHealthBaseline, EquipmentFailure,
    TenkoRecord,
};

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
}
