use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use alc_core::repository::notify_line_config::*;
use alc_core::tenant::TenantConn;

pub struct PgNotifyLineConfigRepository {
    pool: PgPool,
}

impl PgNotifyLineConfigRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl NotifyLineConfigRepository for PgNotifyLineConfigRepository {
    async fn get(&self, tenant_id: Uuid) -> Result<Option<NotifyLineConfig>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, NotifyLineConfig>(
            "SELECT id, tenant_id, name, channel_id, bot_basic_id, public_key_jwk, login_channel_id, login_key_id, enabled, created_at, updated_at FROM notify_line_configs LIMIT 1",
        )
        .fetch_optional(&mut *tc.conn)
        .await
    }

    async fn get_full(&self, tenant_id: Uuid) -> Result<Option<NotifyLineConfigFull>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, NotifyLineConfigFull>(
            "SELECT id, tenant_id, channel_id, channel_secret_encrypted, channel_access_token_encrypted, key_id, private_key_encrypted, login_channel_id, login_key_id FROM notify_line_configs LIMIT 1",
        )
        .fetch_optional(&mut *tc.conn)
        .await
    }

    async fn upsert(
        &self,
        tenant_id: Uuid,
        name: &str,
        channel_id: &str,
        channel_secret_encrypted: &str,
        key_id: &str,
        private_key_encrypted: &str,
        bot_basic_id: Option<&str>,
        public_key_jwk: Option<&str>,
        login_channel_id: Option<&str>,
        login_key_id: Option<&str>,
    ) -> Result<NotifyLineConfig, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, NotifyLineConfig>(
            r#"
            INSERT INTO notify_line_configs (tenant_id, name, channel_id, channel_secret_encrypted, key_id, private_key_encrypted, bot_basic_id, public_key_jwk, login_channel_id, login_key_id)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            ON CONFLICT (tenant_id) DO UPDATE SET
                name = EXCLUDED.name,
                channel_id = EXCLUDED.channel_id,
                channel_secret_encrypted = EXCLUDED.channel_secret_encrypted,
                key_id = EXCLUDED.key_id,
                private_key_encrypted = EXCLUDED.private_key_encrypted,
                bot_basic_id = EXCLUDED.bot_basic_id,
                public_key_jwk = EXCLUDED.public_key_jwk,
                login_channel_id = EXCLUDED.login_channel_id,
                login_key_id = EXCLUDED.login_key_id,
                updated_at = NOW()
            RETURNING id, tenant_id, name, channel_id, bot_basic_id, public_key_jwk, login_channel_id, login_key_id, enabled, created_at, updated_at
            "#,
        )
        .bind(tenant_id)
        .bind(name)
        .bind(channel_id)
        .bind(channel_secret_encrypted)
        .bind(key_id)
        .bind(private_key_encrypted)
        .bind(bot_basic_id)
        .bind(public_key_jwk)
        .bind(login_channel_id)
        .bind(login_key_id)
        .fetch_one(&mut *tc.conn)
        .await
    }

    async fn delete(&self, tenant_id: Uuid) -> Result<(), sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query("DELETE FROM notify_line_configs")
            .execute(&mut *tc.conn)
            .await?;
        Ok(())
    }

    async fn lookup_by_channel(
        &self,
        channel_id: &str,
    ) -> Result<Option<NotifyLineConfigFull>, sqlx::Error> {
        sqlx::query_as::<_, NotifyLineConfigFull>(
            "SELECT * FROM alc_api.lookup_line_config_by_channel($1)",
        )
        .bind(channel_id)
        .fetch_optional(&self.pool)
        .await
    }
}
