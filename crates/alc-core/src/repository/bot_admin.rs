use async_trait::async_trait;
use uuid::Uuid;

/// テナント情報 (Bot Config export 用、staging import と互換のシェイプ)
#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct TenantInfoForExport {
    pub id: Uuid,
    pub name: String,
    pub slug: Option<String>,
    pub email_domain: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// bot_configs 行 (export 用、暗号化のまま全フィールド)
#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct BotConfigExportRow {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub provider: String,
    pub name: String,
    pub client_id: String,
    pub client_secret_encrypted: String,
    pub service_account: String,
    pub bot_id: String,
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bot_secret_encrypted: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[async_trait]
pub trait BotAdminRepository: Send + Sync {
    /// 指定テナントの基本情報を取得 (Bot Config export 用、tenants は RLS なしで OK)
    async fn get_tenant_for_export(
        &self,
        tenant_id: Uuid,
    ) -> Result<Option<TenantInfoForExport>, sqlx::Error>;

    /// 指定テナントの全 bot_configs を export 用に取得 (暗号化フィールド含む)
    async fn list_configs_for_export(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<BotConfigExportRow>, sqlx::Error>;
}
