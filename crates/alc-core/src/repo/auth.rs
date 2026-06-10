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
        // short_id 衝突は DB の UNIQUE 制約で弾かれるので、UNIQUE 違反だけ
        // 補足して再試行する。DEFAULT 式が gen_random_uuid() なので、
        // 同じ短縮ID が連続して降ってくる確率は実質ゼロ (4B 中 1)。
        for _ in 0..16 {
            let result = sqlx::query_as::<_, Tenant>(
                "INSERT INTO tenants (name, email_domain) VALUES ($1, $1) RETURNING *",
            )
            .bind(email_domain)
            .fetch_one(&self.pool)
            .await;
            match result {
                Ok(t) => return Ok(t),
                Err(sqlx::Error::Database(e)) if e.is_unique_violation() => continue,
                Err(e) => return Err(e),
            }
        }
        Err(sqlx::Error::Configuration(
            "could not insert tenant: 16 short_id collisions in a row".into(),
        ))
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

    async fn get_tenant_short_id(&self, tenant_id: Uuid) -> Result<Option<String>, sqlx::Error> {
        sqlx::query_scalar::<_, String>("SELECT short_id FROM tenants WHERE id = $1")
            .bind(tenant_id)
            .fetch_optional(&self.pool)
            .await
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

    // --- Switch org ---

    async fn find_user_in_tenant(
        &self,
        target_tenant_id: Uuid,
        current_user_id: Uuid,
    ) -> Result<Option<User>, sqlx::Error> {
        // email 一致のみで別テナントの user row にマッチさせない。同一 email が
        // 複数テナントに存在すると意図しない role での JWT 発行につながるため、
        // verified identity (google_sub / lineworks_id / line_user_id) の一致を
        // 要求する (Refs #393 M-4)。verified ID を持たないユーザーは switch 不可。
        sqlx::query_as::<_, User>(
            r#"SELECT t.* FROM users t
               JOIN users c ON c.id = $2
               WHERE t.tenant_id = $1
               AND (
                 (t.google_sub IS NOT NULL AND t.google_sub = c.google_sub)
                 OR (t.lineworks_id IS NOT NULL AND t.lineworks_id = c.lineworks_id)
                 OR (t.line_user_id IS NOT NULL AND t.line_user_id = c.line_user_id)
               )
               LIMIT 1"#,
        )
        .bind(target_tenant_id)
        .bind(current_user_id)
        .fetch_optional(&self.pool)
        .await
    }

    // --- Password login ---

    async fn find_user_by_username(
        &self,
        tenant_id: Uuid,
        username: &str,
    ) -> Result<Option<User>, sqlx::Error> {
        sqlx::query_as::<_, User>("SELECT * FROM users WHERE tenant_id = $1 AND username = $2")
            .bind(tenant_id)
            .bind(username)
            .fetch_optional(&self.pool)
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

    async fn find_recipients_by_line_user_id(
        &self,
        line_user_id: &str,
    ) -> Result<Vec<(Uuid, String)>, sqlx::Error> {
        sqlx::query_as::<_, (Uuid, String)>(
            "SELECT tenant_id, recipient_name FROM find_recipient_by_line_user_id($1)",
        )
        .bind(line_user_id)
        .fetch_all(&self.pool)
        .await
    }

    async fn register_line_recipient(
        &self,
        tenant_id: Uuid,
        name: &str,
        line_user_id: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO alc_api.notify_recipients (tenant_id, name, provider, line_user_id) \
             VALUES ($1, $2, 'line', $3) \
             ON CONFLICT (tenant_id, line_user_id) WHERE line_user_id IS NOT NULL DO NOTHING",
        )
        .bind(tenant_id)
        .bind(name)
        .bind(line_user_id)
        .execute(&self.pool)
        .await?;
        Ok(())
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
