use async_trait::async_trait;
use ts_rs::TS;
use uuid::Uuid;

/// SSO Provider Config の DB 行 (client_secret_encrypted は除外)
#[derive(Debug, serde::Serialize, Clone, sqlx::FromRow, TS)]
#[ts(export)]
pub struct SsoConfigRow {
    pub provider: String,
    pub client_id: String,
    pub external_org_id: String,
    pub enabled: bool,
    pub woff_id: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[allow(clippy::too_many_arguments)]
#[async_trait]
pub trait SsoAdminRepository: Send + Sync {
    /// テナントの SSO 設定一覧
    async fn list_configs(&self, tenant_id: Uuid) -> Result<Vec<SsoConfigRow>, sqlx::Error>;

    /// SSO 設定の作成/更新 (client_secret_encrypted あり)
    async fn upsert_config_with_secret(
        &self,
        tenant_id: Uuid,
        provider: &str,
        client_id: &str,
        client_secret_encrypted: &str,
        external_org_id: &str,
        woff_id: Option<&str>,
        enabled: bool,
    ) -> Result<SsoConfigRow, sqlx::Error>;

    /// SSO 設定の作成/更新 (client_secret_encrypted なし)
    async fn upsert_config_without_secret(
        &self,
        tenant_id: Uuid,
        provider: &str,
        client_id: &str,
        external_org_id: &str,
        woff_id: Option<&str>,
        enabled: bool,
    ) -> Result<SsoConfigRow, sqlx::Error>;

    /// SSO 設定の削除
    async fn delete_config(&self, tenant_id: Uuid, provider: &str) -> Result<(), sqlx::Error>;
}
