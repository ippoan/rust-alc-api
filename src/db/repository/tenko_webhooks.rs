use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use crate::db::models::{CreateWebhookConfig, WebhookConfig, WebhookDelivery};

use super::TenantConn;

pub use alc_core::repository::tenko_webhooks::*;

pub struct PgTenkoWebhooksRepository {
    pool: PgPool,
}

impl PgTenkoWebhooksRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl TenkoWebhooksRepository for PgTenkoWebhooksRepository {
    async fn upsert(
        &self,
        tenant_id: Uuid,
        input: &CreateWebhookConfig,
    ) -> Result<WebhookConfig, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, WebhookConfig>(
            r#"
            INSERT INTO webhook_configs (tenant_id, event_type, url, secret, enabled)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (tenant_id, event_type)
            DO UPDATE SET url = $3, secret = $4, enabled = $5, updated_at = NOW()
            RETURNING *
            "#,
        )
        .bind(tenant_id)
        .bind(&input.event_type)
        .bind(&input.url)
        .bind(&input.secret)
        .bind(input.enabled)
        .fetch_one(&mut *tc.conn)
        .await
    }

    async fn list(&self, tenant_id: Uuid) -> Result<Vec<WebhookConfig>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, WebhookConfig>(
            "SELECT * FROM webhook_configs WHERE tenant_id = $1 ORDER BY event_type",
        )
        .bind(tenant_id)
        .fetch_all(&mut *tc.conn)
        .await
    }

    async fn get(&self, tenant_id: Uuid, id: Uuid) -> Result<Option<WebhookConfig>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, WebhookConfig>(
            "SELECT * FROM webhook_configs WHERE id = $1 AND tenant_id = $2",
        )
        .bind(id)
        .bind(tenant_id)
        .fetch_optional(&mut *tc.conn)
        .await
    }

    async fn delete(&self, tenant_id: Uuid, id: Uuid) -> Result<bool, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let result = sqlx::query("DELETE FROM webhook_configs WHERE id = $1 AND tenant_id = $2")
            .bind(id)
            .bind(tenant_id)
            .execute(&mut *tc.conn)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn list_deliveries(
        &self,
        tenant_id: Uuid,
        config_id: Uuid,
    ) -> Result<Vec<WebhookDelivery>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, WebhookDelivery>(
            r#"
            SELECT * FROM webhook_deliveries
            WHERE config_id = $1 AND tenant_id = $2
            ORDER BY created_at DESC
            LIMIT 100
            "#,
        )
        .bind(config_id)
        .bind(tenant_id)
        .fetch_all(&mut *tc.conn)
        .await
    }
}
