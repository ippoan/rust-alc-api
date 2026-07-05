use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use alc_core::models::WebhookConfig;

use alc_core::tenant::TenantConn;

pub use alc_core::repository::webhook::*;

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
}
