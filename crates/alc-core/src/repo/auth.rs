use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::models::{Tenant, TenantAllowedEmail, User};

pub use crate::repository::auth::*;

pub struct PgAuthRepository {
    pool: PgPool,
}

impl PgAuthRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl AuthRepository for PgAuthRepository {
    async fn find_user_by_google_sub(&self, google_sub: &str) -> Result<Option<User>, sqlx::Error> {
        sqlx::query_as::<_, User>("SELECT * FROM users WHERE google_sub = $1")
            .bind(google_sub)
            .fetch_optional(&self.pool)
            .await
    }

    async fn find_user_by_lineworks_id(
        &self,
        lineworks_id: &str,
    ) -> Result<Option<User>, sqlx::Error> {
        sqlx::query_as::<_, User>("SELECT * FROM users WHERE lineworks_id = $1")
            .bind(lineworks_id)
            .fetch_optional(&self.pool)
            .await
    }

    async fn find_user_by_refresh_token_hash(
        &self,
        token_hash: &str,
    ) -> Result<Option<User>, sqlx::Error> {
        sqlx::query_as::<_, User>(
            r#"
            SELECT * FROM users
            WHERE refresh_token_hash = $1
              AND refresh_token_expires_at > NOW()
            "#,
        )
        .bind(token_hash)
        .fetch_optional(&self.pool)
        .await
    }

    async fn find_invitation_by_email(
        &self,
        email: &str,
    ) -> Result<Option<TenantAllowedEmail>, sqlx::Error> {
        sqlx::query_as::<_, TenantAllowedEmail>(
            "SELECT * FROM tenant_allowed_emails WHERE email = $1",
        )
        .bind(email)
        .fetch_optional(&self.pool)
        .await
    }

    async fn delete_invitation(&self, id: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM tenant_allowed_emails WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn find_tenant_by_email_domain(
        &self,
        email_domain: &str,
    ) -> Result<Option<Tenant>, sqlx::Error> {
        sqlx::query_as::<_, Tenant>("SELECT * FROM tenants WHERE email_domain = $1")
            .bind(email_domain)
            .fetch_optional(&self.pool)
            .await
    }

    async fn create_tenant_with_domain(&self, email_domain: &str) -> Result<Tenant, sqlx::Error> {
        sqlx::query_as::<_, Tenant>(
            "INSERT INTO tenants (name, email_domain) VALUES ($1, $1) RETURNING *",
        )
        .bind(email_domain)
        .fetch_one(&self.pool)
        .await
    }

    async fn create_tenant_by_name(&self, name: &str) -> Result<Tenant, sqlx::Error> {
        sqlx::query_as::<_, Tenant>("INSERT INTO tenants (name) VALUES ($1) RETURNING *")
            .bind(name)
            .fetch_one(&self.pool)
            .await
    }

    async fn get_tenant_by_id(&self, id: Uuid) -> Result<Option<Tenant>, sqlx::Error> {
        sqlx::query_as::<_, Tenant>("SELECT * FROM tenants WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
    }

    async fn get_tenant_slug(&self, tenant_id: Uuid) -> Result<Option<String>, sqlx::Error> {
        sqlx::query_scalar::<_, Option<String>>("SELECT slug FROM tenants WHERE id = $1")
            .bind(tenant_id)
            .fetch_optional(&self.pool)
            .await
            .map(|opt| opt.flatten())
    }

    async fn create_user_google(
        &self,
        tenant_id: Uuid,
        google_sub: &str,
        email: &str,
        name: &str,
        role: &str,
    ) -> Result<User, sqlx::Error> {
        sqlx::query_as::<_, User>(
            r#"
            INSERT INTO users (tenant_id, google_sub, email, name, role)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING *
            "#,
        )
        .bind(tenant_id)
        .bind(google_sub)
        .bind(email)
        .bind(name)
        .bind(role)
        .fetch_one(&self.pool)
        .await
    }

    async fn create_user_lineworks(
        &self,
        tenant_id: Uuid,
        lineworks_id: &str,
        email: &str,
        name: &str,
    ) -> Result<User, sqlx::Error> {
        sqlx::query_as::<_, User>(
            r#"INSERT INTO users (tenant_id, lineworks_id, email, name, role)
               VALUES ($1, $2, $3, $4, 'admin') RETURNING *"#,
        )
        .bind(tenant_id)
        .bind(lineworks_id)
        .bind(email)
        .bind(name)
        .fetch_one(&self.pool)
        .await
    }

    // --- LINE Login ---

    async fn find_user_by_line_user_id(
        &self,
        line_user_id: &str,
    ) -> Result<Option<User>, sqlx::Error> {
        sqlx::query_as::<_, User>("SELECT * FROM users WHERE line_user_id = $1")
            .bind(line_user_id)
            .fetch_optional(&self.pool)
            .await
    }

    async fn find_recipient_by_line_user_id(
        &self,
        line_user_id: &str,
    ) -> Result<Option<(Uuid, String)>, sqlx::Error> {
        sqlx::query_as::<_, (Uuid, String)>(
            "SELECT tenant_id, recipient_name FROM find_recipient_by_line_user_id($1)",
        )
        .bind(line_user_id)
        .fetch_optional(&self.pool)
        .await
    }

    async fn create_user_line(
        &self,
        tenant_id: Uuid,
        line_user_id: &str,
        name: &str,
    ) -> Result<User, sqlx::Error> {
        sqlx::query_as::<_, User>(
            r#"INSERT INTO users (tenant_id, line_user_id, email, name, role)
               VALUES ($1, $2, $2, $3, 'viewer') RETURNING *"#,
        )
        .bind(tenant_id)
        .bind(line_user_id)
        .bind(name)
        .fetch_one(&self.pool)
        .await
    }

    async fn save_refresh_token(
        &self,
        user_id: Uuid,
        refresh_hash: &str,
        expires_at: DateTime<Utc>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE users SET refresh_token_hash = $1, refresh_token_expires_at = $2 WHERE id = $3",
        )
        .bind(refresh_hash)
        .bind(expires_at)
        .bind(user_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn clear_refresh_token(&self, user_id: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE users SET refresh_token_hash = NULL, refresh_token_expires_at = NULL WHERE id = $1",
        )
        .bind(user_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn resolve_sso_config(
        &self,
        provider: &str,
        domain: &str,
    ) -> Result<Option<SsoConfigRow>, sqlx::Error> {
        sqlx::query_as::<_, SsoConfigRow>("SELECT * FROM resolve_sso_config($1, $2)")
            .bind(provider)
            .bind(domain)
            .fetch_optional(&self.pool)
            .await
    }

    async fn resolve_sso_config_required(
        &self,
        provider: &str,
        domain: &str,
    ) -> Result<SsoConfigRow, sqlx::Error> {
        sqlx::query_as::<_, SsoConfigRow>("SELECT * FROM resolve_sso_config($1, $2)")
            .bind(provider)
            .bind(domain)
            .fetch_one(&self.pool)
            .await
    }
}
