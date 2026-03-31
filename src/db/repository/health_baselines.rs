use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use crate::db::models::{CreateHealthBaseline, EmployeeHealthBaseline, UpdateHealthBaseline};

use super::TenantConn;

pub use alc_core::repository::health_baselines::*;

pub struct PgHealthBaselinesRepository {
    pool: PgPool,
}

impl PgHealthBaselinesRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl HealthBaselinesRepository for PgHealthBaselinesRepository {
    async fn upsert(
        &self,
        tenant_id: Uuid,
        body: &CreateHealthBaseline,
    ) -> Result<EmployeeHealthBaseline, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, EmployeeHealthBaseline>(
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
        .fetch_one(&mut *tc.conn)
        .await
    }

    async fn list(&self, tenant_id: Uuid) -> Result<Vec<EmployeeHealthBaseline>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, EmployeeHealthBaseline>(
            "SELECT * FROM employee_health_baselines WHERE tenant_id = $1 ORDER BY created_at DESC",
        )
        .bind(tenant_id)
        .fetch_all(&mut *tc.conn)
        .await
    }

    async fn get(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
    ) -> Result<Option<EmployeeHealthBaseline>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, EmployeeHealthBaseline>(
            "SELECT * FROM employee_health_baselines WHERE tenant_id = $1 AND employee_id = $2",
        )
        .bind(tenant_id)
        .bind(employee_id)
        .fetch_optional(&mut *tc.conn)
        .await
    }

    async fn update(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
        body: &UpdateHealthBaseline,
    ) -> Result<Option<EmployeeHealthBaseline>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, EmployeeHealthBaseline>(
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
        .fetch_optional(&mut *tc.conn)
        .await
    }

    async fn delete(&self, tenant_id: Uuid, employee_id: Uuid) -> Result<bool, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let result = sqlx::query(
            "DELETE FROM employee_health_baselines WHERE tenant_id = $1 AND employee_id = $2",
        )
        .bind(tenant_id)
        .bind(employee_id)
        .execute(&mut *tc.conn)
        .await?;
        Ok(result.rows_affected() > 0)
    }
}
