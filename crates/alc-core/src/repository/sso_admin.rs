use async_trait::async_trait;
use ts_rs::TS;
use uuid::Uuid;

pub use super::bot_admin::TenantInfoForExport;

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

/// sso_provider_configs 行 (export 用、暗号化のまま全フィールド)。
/// staging import の `StagingSsoProviderConfig` と同一シェイプで round-trip 可能。
#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct SsoConfigExportRow {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub provider: String,
    pub client_id: String,
    pub client_secret_encrypted: String,
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

    /// SSO 設定の更新 (client_secret_encrypted なし = UPDATE only)。
    /// 既存 config が無ければ RowNotFound (= 新規作成には secret 必須)。
    async fn upsert_config_without_secret(
        &self,
        tenant_id: Uuid,
        provider: &str,
        client_id: &str,
        external_org_id: &str,
        woff_id: Option<&str>,
        enabled: bool,
    ) -> Result<SsoConfigRow, sqlx::Error>;

    /// SSO 設定の削除。削除した行数を返す (0 = 該当なし → handler が 404)
    async fn delete_config(&self, tenant_id: Uuid, provider: &str) -> Result<u64, sqlx::Error>;

    /// 指定テナントの基本情報を取得 (export 用、tenants は RLS なしで OK)
    async fn get_tenant_for_export(
        &self,
        tenant_id: Uuid,
    ) -> Result<Option<TenantInfoForExport>, sqlx::Error>;

    /// 指定テナントの全 sso_provider_configs を export 用に取得 (暗号化フィールド含む)
    async fn list_configs_for_export(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<SsoConfigExportRow>, sqlx::Error>;
}
