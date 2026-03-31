use async_trait::async_trait;
use chrono::NaiveDate;
use sqlx::PgPool;
use uuid::Uuid;

use super::TenantConn;

pub use alc_core::repository::daily_health::*;

pub struct PgDailyHealthRepository {
    pool: PgPool,
}

impl PgDailyHealthRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl DailyHealthRepository for PgDailyHealthRepository {
    async fn fetch_daily_health(
        &self,
        tenant_id: Uuid,
        date: NaiveDate,
    ) -> Result<Vec<DailyHealthRow>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, DailyHealthRow>(
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
        .fetch_all(&mut *tc.conn)
        .await
    }
}
