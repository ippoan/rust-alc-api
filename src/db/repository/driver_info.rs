use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use crate::db::models::{
    CarryingItem, DtakoDailyWorkHours, Employee, EmployeeHealthBaseline, EquipmentFailure,
    TenkoRecord,
};

use super::TenantConn;

pub use alc_core::repository::driver_info::*;

pub struct PgDriverInfoRepository {
    pool: PgPool,
}

impl PgDriverInfoRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl DriverInfoRepository for PgDriverInfoRepository {
    async fn get_employee(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
    ) -> Result<Option<Employee>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, Employee>(
            "SELECT * FROM alc_api.employees WHERE id = $1 AND deleted_at IS NULL",
        )
        .bind(employee_id)
        .fetch_optional(&mut *tc.conn)
        .await
    }

    async fn get_health_baseline(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
    ) -> Result<Option<EmployeeHealthBaseline>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, EmployeeHealthBaseline>(
            "SELECT * FROM alc_api.employee_health_baselines WHERE employee_id = $1",
        )
        .bind(employee_id)
        .fetch_optional(&mut *tc.conn)
        .await
    }

    async fn get_recent_measurements(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
    ) -> Result<Vec<MeasurementSummary>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, MeasurementSummary>(
            r#"SELECT id, temperature, systolic, diastolic, pulse, medical_measured_at AS measured_at
               FROM alc_api.tenko_sessions
               WHERE employee_id = $1 AND medical_measured_at IS NOT NULL
               ORDER BY medical_measured_at DESC LIMIT 5"#,
        )
        .bind(employee_id)
        .fetch_all(&mut *tc.conn)
        .await
    }

    async fn get_working_hours(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
    ) -> Result<Vec<DtakoDailyWorkHours>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, DtakoDailyWorkHours>(
            r#"SELECT * FROM alc_api.dtako_daily_work_hours
               WHERE driver_id = $1
               ORDER BY work_date DESC LIMIT 7"#,
        )
        .bind(employee_id)
        .fetch_all(&mut *tc.conn)
        .await
    }

    async fn get_past_instructions(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
    ) -> Result<Vec<InstructionSummary>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, InstructionSummary>(
            r#"SELECT session_id, instruction, instruction_confirmed_at, recorded_at
               FROM alc_api.tenko_records
               WHERE employee_id = $1 AND instruction IS NOT NULL AND instruction != ''
               ORDER BY recorded_at DESC LIMIT 10"#,
        )
        .bind(employee_id)
        .fetch_all(&mut *tc.conn)
        .await
    }

    async fn get_carrying_items(&self, tenant_id: Uuid) -> Result<Vec<CarryingItem>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, CarryingItem>(
            "SELECT * FROM alc_api.carrying_items ORDER BY sort_order, created_at",
        )
        .fetch_all(&mut *tc.conn)
        .await
    }

    async fn get_past_tenko_records(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
    ) -> Result<Vec<TenkoRecord>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, TenkoRecord>(
            r#"SELECT * FROM alc_api.tenko_records
               WHERE employee_id = $1
               ORDER BY recorded_at DESC LIMIT 10"#,
        )
        .bind(employee_id)
        .fetch_all(&mut *tc.conn)
        .await
    }

    async fn get_recent_daily_inspections(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
    ) -> Result<Vec<DailyInspectionSummary>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, DailyInspectionSummary>(
            r#"SELECT session_id, daily_inspection, recorded_at
               FROM alc_api.tenko_records
               WHERE employee_id = $1 AND daily_inspection IS NOT NULL
               ORDER BY recorded_at DESC LIMIT 5"#,
        )
        .bind(employee_id)
        .fetch_all(&mut *tc.conn)
        .await
    }

    async fn get_equipment_failures(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<EquipmentFailure>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, EquipmentFailure>(
            r#"SELECT * FROM alc_api.equipment_failures
               WHERE resolved_at IS NULL
               ORDER BY reported_at DESC"#,
        )
        .fetch_all(&mut *tc.conn)
        .await
    }
}
