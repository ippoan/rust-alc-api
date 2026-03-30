use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use crate::db::models::{TenkoSchedule, WebhookConfig};

use super::TenantConn;

#[async_trait]
#[allow(clippy::too_many_arguments)]
pub trait WebhookRepository: Send + Sync {
    async fn find_config(
        &self,
        tenant_id: Uuid,
        event_type: &str,
    ) -> Result<Option<WebhookConfig>, sqlx::Error>;

    async fn record_delivery(
        &self,
        tenant_id: Uuid,
        config_id: Uuid,
        event_type: &str,
        payload: &serde_json::Value,
        status_code: Option<i32>,
        response_body: Option<&str>,
        attempt: i32,
        success: bool,
    ) -> Result<(), sqlx::Error>;

    async fn find_overdue_configs(&self) -> Result<Vec<WebhookConfig>, sqlx::Error>;

    async fn find_overdue_schedules(
        &self,
        tenant_id: Uuid,
        overdue_minutes: i64,
    ) -> Result<Vec<TenkoSchedule>, sqlx::Error>;

    async fn get_employee_name(&self, employee_id: Uuid) -> Result<Option<String>, sqlx::Error>;

    async fn mark_overdue_notified(&self, schedule_id: Uuid) -> Result<(), sqlx::Error>;
}

pub struct PgWebhookRepository {
    pool: PgPool,
}

impl PgWebhookRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl WebhookRepository for PgWebhookRepository {
    async fn find_config(
        &self,
        tenant_id: Uuid,
        event_type: &str,
    ) -> Result<Option<WebhookConfig>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, WebhookConfig>(
            "SELECT * FROM webhook_configs WHERE tenant_id = $1 AND event_type = $2 AND enabled = TRUE",
        )
        .bind(tenant_id)
        .bind(event_type)
        .fetch_optional(&mut *tc.conn)
        .await
    }

    async fn record_delivery(
        &self,
        tenant_id: Uuid,
        config_id: Uuid,
        event_type: &str,
        payload: &serde_json::Value,
        status_code: Option<i32>,
        response_body: Option<&str>,
        attempt: i32,
        success: bool,
    ) -> Result<(), sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query(
            r#"
            INSERT INTO webhook_deliveries (
                tenant_id, config_id, event_type, payload,
                status_code, response_body, attempt, delivered_at, success
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            "#,
        )
        .bind(tenant_id)
        .bind(config_id)
        .bind(event_type)
        .bind(payload)
        .bind(status_code)
        .bind(response_body)
        .bind(attempt)
        .bind(if success {
            Some(chrono::Utc::now())
        } else {
            None
        })
        .bind(success)
        .execute(&mut *tc.conn)
        .await?;
        Ok(())
    }

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
