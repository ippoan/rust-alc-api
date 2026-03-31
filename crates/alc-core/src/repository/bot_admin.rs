use async_trait::async_trait;
use uuid::Uuid;

/// bot_configs 行 (secret 列を除外した公開用)
#[derive(Debug, sqlx::FromRow)]
pub struct BotConfigRow {
    pub id: Uuid,
    pub provider: String,
    pub name: String,
    pub client_id: String,
    pub service_account: String,
    pub bot_id: String,
    pub enabled: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[allow(clippy::too_many_arguments)]
#[async_trait]
pub trait BotAdminRepository: Send + Sync {
    /// bot_configs 一覧取得
    async fn list_configs(&self, tenant_id: Uuid) -> Result<Vec<BotConfigRow>, sqlx::Error>;

    /// client_secret_encrypted 更新
    async fn update_client_secret(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        encrypted: &str,
    ) -> Result<(), sqlx::Error>;

    /// private_key_encrypted 更新
    async fn update_private_key(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        encrypted: &str,
    ) -> Result<(), sqlx::Error>;

    /// 既存 bot_config を更新
    async fn update_config(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        provider: &str,
        name: &str,
        client_id: &str,
        service_account: &str,
        bot_id: &str,
        enabled: bool,
    ) -> Result<BotConfigRow, sqlx::Error>;

    /// 新規 bot_config を作成
    async fn create_config(
        &self,
        tenant_id: Uuid,
        provider: &str,
        name: &str,
        client_id: &str,
        client_secret_encrypted: &str,
        service_account: &str,
        private_key_encrypted: &str,
        bot_id: &str,
        enabled: bool,
    ) -> Result<BotConfigRow, sqlx::Error>;

    /// bot_config を削除
    async fn delete_config(&self, tenant_id: Uuid, id: Uuid) -> Result<(), sqlx::Error>;
}
