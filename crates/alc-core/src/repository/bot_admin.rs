use async_trait::async_trait;
use uuid::Uuid;

/// bot_configs 行 (暗号化フィールド付き、メッセージ送信用)
#[derive(Debug, sqlx::FromRow)]
pub struct BotConfigWithSecrets {
    pub id: Uuid,
    pub provider: String,
    pub name: String,
    pub client_id: String,
    pub client_secret_encrypted: String,
    pub service_account: String,
    pub private_key_encrypted: String,
    pub bot_id: String,
    pub enabled: bool,
    /// LINE WORKS Bot webhook 署名検証用 (X-WORKS-Signature HMAC key)。
    /// migration 102 で追加。未設定時は webhook が常に 401 を返す。
    pub bot_secret_encrypted: Option<String>,
}

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
    pub private_key_encrypted: String,
    pub bot_id: String,
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bot_secret_encrypted: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// bot_configs 行 (secret 列を除外した公開用)
#[derive(Debug, Clone, sqlx::FromRow)]
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

    /// bot_config を暗号化フィールド付きで取得 (メッセージ送信用)
    async fn get_config_with_secrets(
        &self,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<Option<BotConfigWithSecrets>, sqlx::Error>;

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

    /// bot_secret_encrypted 更新 (LINE WORKS Bot webhook 署名検証用)
    async fn update_bot_secret(
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
