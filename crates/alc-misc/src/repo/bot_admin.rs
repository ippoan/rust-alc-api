use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use alc_core::tenant::TenantConn;

pub use alc_core::repository::bot_admin::*;

pub struct PgBotAdminRepository {
    pool: PgPool,
}

impl PgBotAdminRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl BotAdminRepository for PgBotAdminRepository {
    async fn list_configs(&self, tenant_id: Uuid) -> Result<Vec<BotConfigRow>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, BotConfigRow>(
            r#"
            SELECT id, provider, name, client_id, service_account, bot_id, enabled, created_at, updated_at
            FROM bot_configs
            ORDER BY name
            "#,
        )
        .fetch_all(&mut *tc.conn)
        .await
    }

    async fn update_client_secret(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        encrypted: &str,
    ) -> Result<(), sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query("UPDATE bot_configs SET client_secret_encrypted = $1 WHERE id = $2")
            .bind(encrypted)
            .bind(id)
            .execute(&mut *tc.conn)
            .await?;
        Ok(())
    }

    async fn update_private_key(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        encrypted: &str,
    ) -> Result<(), sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query("UPDATE bot_configs SET private_key_encrypted = $1 WHERE id = $2")
            .bind(encrypted)
            .bind(id)
            .execute(&mut *tc.conn)
            .await?;
        Ok(())
    }

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
    ) -> Result<BotConfigRow, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, BotConfigRow>(
            r#"
            UPDATE bot_configs SET
                provider = $1, name = $2, client_id = $3, service_account = $4,
                bot_id = $5, enabled = $6, updated_at = NOW()
            WHERE id = $7
            RETURNING id, provider, name, client_id, service_account, bot_id, enabled, created_at, updated_at
            "#,
        )
        .bind(provider)
        .bind(name)
        .bind(client_id)
        .bind(service_account)
        .bind(bot_id)
        .bind(enabled)
        .bind(id)
        .fetch_one(&mut *tc.conn)
        .await
    }

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
    ) -> Result<BotConfigRow, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, BotConfigRow>(
            r#"
            INSERT INTO bot_configs (tenant_id, provider, name, client_id, client_secret_encrypted, service_account, private_key_encrypted, bot_id, enabled)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            RETURNING id, provider, name, client_id, service_account, bot_id, enabled, created_at, updated_at
            "#,
        )
        .bind(tenant_id)
        .bind(provider)
        .bind(name)
        .bind(client_id)
        .bind(client_secret_encrypted)
        .bind(service_account)
        .bind(private_key_encrypted)
        .bind(bot_id)
        .bind(enabled)
        .fetch_one(&mut *tc.conn)
        .await
    }

    async fn get_config_with_secrets(
        &self,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<Option<BotConfigWithSecrets>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, BotConfigWithSecrets>(
            r#"
            SELECT id, provider, name, client_id, client_secret_encrypted,
                   service_account, private_key_encrypted, bot_id, enabled
            FROM bot_configs WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&mut *tc.conn)
        .await
    }

    async fn delete_config(&self, tenant_id: Uuid, id: Uuid) -> Result<(), sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query("DELETE FROM bot_configs WHERE id = $1")
            .bind(id)
            .execute(&mut *tc.conn)
            .await?;
        Ok(())
    }
}
