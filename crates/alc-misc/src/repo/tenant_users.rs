use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use alc_core::models::TenantAllowedEmail;

use alc_core::tenant::TenantConn;

pub use alc_core::repository::tenant_users::*;

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
            "SELECT id, email, name, role, created_at FROM users WHERE tenant_id = $1 ORDER BY created_at",
        )
        .bind(tenant_id)
        .fetch_all(&mut *tc.conn)
        .await
    }

    async fn list_invitations(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<TenantAllowedEmail>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, TenantAllowedEmail>(
            "SELECT * FROM tenant_allowed_emails WHERE tenant_id = $1 ORDER BY created_at",
        )
        .bind(tenant_id)
        .fetch_all(&mut *tc.conn)
        .await
    }

    async fn invite_user(
        &self,
        tenant_id: Uuid,
        email: &str,
        role: &str,
    ) -> Result<TenantAllowedEmail, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
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
        .fetch_one(&mut *tc.conn)
        .await
    }

    async fn delete_invitation(&self, tenant_id: Uuid, id: Uuid) -> Result<(), sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query("DELETE FROM tenant_allowed_emails WHERE id = $1 AND tenant_id = $2")
            .bind(id)
            .bind(tenant_id)
            .execute(&mut *tc.conn)
            .await?;
        Ok(())
    }

    async fn delete_user(&self, tenant_id: Uuid, id: Uuid) -> Result<(), sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query("DELETE FROM users WHERE id = $1 AND tenant_id = $2")
            .bind(id)
            .bind(tenant_id)
            .execute(&mut *tc.conn)
            .await?;
        Ok(())
    }

    async fn update_role_by_email(
        &self,
        tenant_id: Uuid,
        email: &str,
        role: &str,
    ) -> Result<bool, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let users_affected =
            sqlx::query("UPDATE users SET role = $1 WHERE email = $2 AND tenant_id = $3")
                .bind(role)
                .bind(email)
                .bind(tenant_id)
                .execute(&mut *tc.conn)
                .await?
                .rows_affected();
        let invites_affected = sqlx::query(
            "UPDATE tenant_allowed_emails SET role = $1 WHERE email = $2 AND tenant_id = $3",
        )
        .bind(role)
        .bind(email)
        .bind(tenant_id)
        .execute(&mut *tc.conn)
        .await?
        .rows_affected();
        Ok(users_affected + invites_affected > 0)
    }

    async fn delete_by_email(&self, tenant_id: Uuid, email: &str) -> Result<bool, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let users_affected = sqlx::query("DELETE FROM users WHERE email = $1 AND tenant_id = $2")
            .bind(email)
            .bind(tenant_id)
            .execute(&mut *tc.conn)
            .await?
            .rows_affected();
        let invites_affected =
            sqlx::query("DELETE FROM tenant_allowed_emails WHERE email = $1 AND tenant_id = $2")
                .bind(email)
                .bind(tenant_id)
                .execute(&mut *tc.conn)
                .await?
                .rows_affected();
        Ok(users_affected + invites_affected > 0)
    }
}
