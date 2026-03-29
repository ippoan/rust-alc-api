use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use crate::db::models::TenantAllowedEmail;

use super::TenantConn;

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

pub struct PgTenantUsersRepository {
    pool: PgPool,
}

impl PgTenantUsersRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl TenantUsersRepository for PgTenantUsersRepository {
    async fn list_users(&self, tenant_id: Uuid) -> Result<Vec<UserRow>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, UserRow>(
            "SELECT id, email, name, role, created_at FROM users ORDER BY created_at",
        )
        .fetch_all(&mut *tc.conn)
        .await
    }

    async fn list_invitations(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<TenantAllowedEmail>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, TenantAllowedEmail>(
            "SELECT * FROM tenant_allowed_emails ORDER BY created_at",
        )
        .fetch_all(&mut *tc.conn)
        .await
    }

    async fn invite_user(
        &self,
        tenant_id: Uuid,
        email: &str,
        role: &str,
    ) -> Result<TenantAllowedEmail, sqlx::Error> {
        sqlx::query_as::<_, TenantAllowedEmail>(
            r#"
            INSERT INTO tenant_allowed_emails (tenant_id, email, role)
            VALUES ($1, $2, $3)
            ON CONFLICT (email) DO UPDATE SET role = EXCLUDED.role
            RETURNING *
            "#,
        )
        .bind(tenant_id)
        .bind(email)
        .bind(role)
        .fetch_one(&self.pool)
        .await
    }

    async fn delete_invitation(&self, tenant_id: Uuid, id: Uuid) -> Result<(), sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query("DELETE FROM tenant_allowed_emails WHERE id = $1")
            .bind(id)
            .execute(&mut *tc.conn)
            .await?;
        Ok(())
    }

    async fn delete_user(&self, tenant_id: Uuid, id: Uuid) -> Result<(), sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(id)
            .execute(&mut *tc.conn)
            .await?;
        Ok(())
    }
}
