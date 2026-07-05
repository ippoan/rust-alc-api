use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use alc_core::models::WebhookConfig;
use alc_core::tenant::TenantConn;

use crate::models::TenkoSchedule;

pub use crate::overdue::TenkoOverdueRepository;

/// overdue 検出クエリの Pg 実装 (旧 alc-misc PgWebhookRepository から分離、Refs #513)
pub struct PgTenkoOverdueRepository {
    pool: PgPool,
}

impl PgTenkoOverdueRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl TenkoOverdueRepository for PgTenkoOverdueRepository {
    async fn find_overdue_configs(&self) -> Result<Vec<WebhookConfig>, sqlx::Error> {
        sqlx::query_as::<_, WebhookConfig>(
            "SELECT * FROM webhook_configs WHERE event_type = 'tenko_overdue' AND enabled = TRUE",
        )
        .fetch_all(&self.pool)
        .await
    }

    async fn find_overdue_schedules(
        &self,
        tenant_id: Uuid,
        overdue_minutes: i64,
    ) -> Result<Vec<TenkoSchedule>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, TenkoSchedule>(
            r#"
            SELECT s.* FROM tenko_schedules s
            WHERE s.tenant_id = $1
              AND s.consumed = FALSE
              AND s.overdue_notified_at IS NULL
              AND s.scheduled_at + ($2 || ' minutes')::INTERVAL < NOW()
            "#,
        )
        .bind(tenant_id)
        .bind(overdue_minutes.to_string())
        .fetch_all(&mut *tc.conn)
        .await
    }

    async fn get_employee_name(&self, employee_id: Uuid) -> Result<Option<String>, sqlx::Error> {
        sqlx::query_scalar("SELECT name FROM employees WHERE id = $1")
            .bind(employee_id)
            .fetch_optional(&self.pool)
            .await
    }

    async fn mark_overdue_notified(&self, schedule_id: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE tenko_schedules SET overdue_notified_at = NOW() WHERE id = $1")
            .bind(schedule_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}
