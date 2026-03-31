use async_trait::async_trait;
use uuid::Uuid;

use crate::models::{CreateWebhookConfig, WebhookConfig, WebhookDelivery};

#[async_trait]
pub trait TenkoWebhooksRepository: Send + Sync {
    async fn upsert(
        &self,
        tenant_id: Uuid,
        input: &CreateWebhookConfig,
    ) -> Result<WebhookConfig, sqlx::Error>;

    async fn list(&self, tenant_id: Uuid) -> Result<Vec<WebhookConfig>, sqlx::Error>;

    async fn get(&self, tenant_id: Uuid, id: Uuid) -> Result<Option<WebhookConfig>, sqlx::Error>;

    async fn delete(&self, tenant_id: Uuid, id: Uuid) -> Result<bool, sqlx::Error>;

    async fn list_deliveries(
        &self,
        tenant_id: Uuid,
        config_id: Uuid,
    ) -> Result<Vec<WebhookDelivery>, sqlx::Error>;
}
