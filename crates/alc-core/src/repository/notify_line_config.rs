use async_trait::async_trait;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct NotifyLineConfig {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub name: String,
    pub channel_id: String,
    pub enabled: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// channel_secret / access_token を含む完全版 (内部利用のみ)
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct NotifyLineConfigFull {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub channel_id: String,
    pub channel_secret_encrypted: String,
    pub channel_access_token_encrypted: String,
}

#[async_trait]
pub trait NotifyLineConfigRepository: Send + Sync {
    async fn get(&self, tenant_id: Uuid) -> Result<Option<NotifyLineConfig>, sqlx::Error>;

    async fn get_full(&self, tenant_id: Uuid) -> Result<Option<NotifyLineConfigFull>, sqlx::Error>;

    async fn upsert(
        &self,
        tenant_id: Uuid,
        name: &str,
        channel_id: &str,
        channel_secret_encrypted: &str,
        channel_access_token_encrypted: &str,
    ) -> Result<NotifyLineConfig, sqlx::Error>;

    async fn delete(&self, tenant_id: Uuid) -> Result<(), sqlx::Error>;

    /// LINE webhook: channel_id からテナント特定 (SECURITY DEFINER, テナント不要)
    async fn lookup_by_channel(
        &self,
        channel_id: &str,
    ) -> Result<Option<NotifyLineConfigFull>, sqlx::Error>;
}
