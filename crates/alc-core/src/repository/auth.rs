use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::models::{Tenant, TenantAllowedEmail, User};

/// SSO プロバイダ設定 (resolve_sso_config 関数の戻り値)
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SsoConfigRow {
    pub tenant_id: Uuid,
    pub client_id: String,
    pub client_secret_encrypted: String,
    pub external_org_id: String,
    pub woff_id: Option<String>,
}

#[async_trait]
pub trait AuthRepository: Send + Sync {
    // --- User lookup ---

    async fn find_user_by_google_sub(&self, google_sub: &str) -> Result<Option<User>, sqlx::Error>;

    async fn find_user_by_lineworks_id(
        &self,
        lineworks_id: &str,
    ) -> Result<Option<User>, sqlx::Error>;

    async fn find_user_by_refresh_token_hash(
        &self,
        token_hash: &str,
    ) -> Result<Option<User>, sqlx::Error>;

    // --- Invitation ---

    async fn find_invitation_by_email(
        &self,
        email: &str,
    ) -> Result<Option<TenantAllowedEmail>, sqlx::Error>;

    async fn delete_invitation(&self, id: Uuid) -> Result<(), sqlx::Error>;

    // --- Tenant ---

    async fn find_tenant_by_email_domain(
        &self,
        email_domain: &str,
    ) -> Result<Option<Tenant>, sqlx::Error>;

    async fn create_tenant_with_domain(&self, email_domain: &str) -> Result<Tenant, sqlx::Error>;

    async fn create_tenant_by_name(&self, name: &str) -> Result<Tenant, sqlx::Error>;

    async fn get_tenant_by_id(&self, id: Uuid) -> Result<Option<Tenant>, sqlx::Error>;

    async fn get_tenant_slug(&self, tenant_id: Uuid) -> Result<Option<String>, sqlx::Error>;

    // --- User creation ---

    #[allow(clippy::too_many_arguments)]
    async fn create_user_google(
        &self,
        tenant_id: Uuid,
        google_sub: &str,
        email: &str,
        name: &str,
        role: &str,
    ) -> Result<User, sqlx::Error>;

    async fn create_user_lineworks(
        &self,
        tenant_id: Uuid,
        lineworks_id: &str,
        email: &str,
        name: &str,
    ) -> Result<User, sqlx::Error>;

    // --- Token management ---

    async fn save_refresh_token(
        &self,
        user_id: Uuid,
        refresh_hash: &str,
        expires_at: DateTime<Utc>,
    ) -> Result<(), sqlx::Error>;

    async fn clear_refresh_token(&self, user_id: Uuid) -> Result<(), sqlx::Error>;

    // --- LINE Login ---

    async fn find_user_by_line_user_id(
        &self,
        line_user_id: &str,
    ) -> Result<Option<User>, sqlx::Error>;

    /// notify_recipients から line_user_id で tenant_id を逆引き (SECURITY DEFINER)
    async fn find_recipient_by_line_user_id(
        &self,
        line_user_id: &str,
    ) -> Result<Option<(Uuid, String)>, sqlx::Error>;

    async fn create_user_line(
        &self,
        tenant_id: Uuid,
        line_user_id: &str,
        name: &str,
    ) -> Result<User, sqlx::Error>;

    // --- SSO config ---

    async fn resolve_sso_config(
        &self,
        provider: &str,
        domain: &str,
    ) -> Result<Option<SsoConfigRow>, sqlx::Error>;

    async fn resolve_sso_config_required(
        &self,
        provider: &str,
        domain: &str,
    ) -> Result<SsoConfigRow, sqlx::Error>;
}
