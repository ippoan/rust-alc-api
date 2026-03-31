use async_trait::async_trait;
use uuid::Uuid;

use crate::models::TenantAllowedEmail;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct UserRow {
    pub id: Uuid,
    pub email: String,
    pub name: String,
    pub role: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[async_trait]
pub trait TenantUsersRepository: Send + Sync {
    async fn list_users(&self, tenant_id: Uuid) -> Result<Vec<UserRow>, sqlx::Error>;

    async fn list_invitations(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<TenantAllowedEmail>, sqlx::Error>;

    async fn invite_user(
        &self,
        tenant_id: Uuid,
        email: &str,
        role: &str,
    ) -> Result<TenantAllowedEmail, sqlx::Error>;

    async fn delete_invitation(&self, tenant_id: Uuid, id: Uuid) -> Result<(), sqlx::Error>;

    async fn delete_user(&self, tenant_id: Uuid, id: Uuid) -> Result<(), sqlx::Error>;
}
