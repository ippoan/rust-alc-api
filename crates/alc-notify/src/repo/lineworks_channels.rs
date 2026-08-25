use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use alc_core::repository::lineworks_channels::*;
use alc_core::tenant::TenantConn;

pub struct PgLineworksChannelsRepository {
    pool: PgPool,
}

impl PgLineworksChannelsRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl LineworksChannelsRepository for PgLineworksChannelsRepository {
    async fn list_active(&self, tenant_id: Uuid) -> Result<Vec<LineworksChannel>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, LineworksChannel>(
            "SELECT * FROM lineworks_channels WHERE active = TRUE ORDER BY joined_at DESC",
        )
        .fetch_all(&mut *tc.conn)
        .await
    }

    async fn get(
        &self,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<Option<LineworksChannel>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, LineworksChannel>("SELECT * FROM lineworks_channels WHERE id = $1")
            .bind(id)
            .fetch_optional(&mut *tc.conn)
            .await
    }

    async fn get_for_send(&self, id: Uuid) -> Result<Option<LineworksChannel>, sqlx::Error> {
        // SECURITY DEFINER 関数経由で RLS バイパス (migration 129)。
        // internal 経路は X-Tenant-ID を honor しないため、tenant は行から解決する。
        sqlx::query_as::<_, LineworksChannel>(
            "SELECT * FROM alc_api.lookup_lineworks_channel_for_send($1)",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
    }

    async fn upsert_joined(
        &self,
        tenant_id: Uuid,
        bot_config_id: Uuid,
        channel_id: &str,
        channel_type: Option<&str>,
        title: Option<&str>,
    ) -> Result<LineworksChannel, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, LineworksChannel>(
            r#"
            INSERT INTO lineworks_channels
                (tenant_id, bot_config_id, channel_id, channel_type, title, active, joined_at)
            VALUES ($1, $2, $3, $4, $5, TRUE, NOW())
            ON CONFLICT (bot_config_id, channel_id)
            DO UPDATE SET
                active = TRUE,
                joined_at = NOW(),
                channel_type = COALESCE(EXCLUDED.channel_type, lineworks_channels.channel_type),
                title = COALESCE(EXCLUDED.title, lineworks_channels.title),
                updated_at = NOW()
            RETURNING *
            "#,
        )
        .bind(tenant_id)
        .bind(bot_config_id)
        .bind(channel_id)
        .bind(channel_type)
        .bind(title)
        .fetch_one(&mut *tc.conn)
        .await
    }

    async fn mark_left(
        &self,
        tenant_id: Uuid,
        bot_config_id: Uuid,
        channel_id: &str,
    ) -> Result<(), sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query(
            r#"
            UPDATE lineworks_channels
               SET active = FALSE, updated_at = NOW()
             WHERE bot_config_id = $1 AND channel_id = $2
            "#,
        )
        .bind(bot_config_id)
        .bind(channel_id)
        .execute(&mut *tc.conn)
        .await?;
        Ok(())
    }

    async fn delete(&self, tenant_id: Uuid, id: Uuid) -> Result<(), sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query("DELETE FROM lineworks_channels WHERE id = $1")
            .bind(id)
            .execute(&mut *tc.conn)
            .await?;
        Ok(())
    }

    async fn lookup_bot_config_for_webhook(
        &self,
        bot_id: &str,
    ) -> Result<Option<BotConfigForWebhook>, sqlx::Error> {
        sqlx::query_as::<_, BotConfigForWebhook>("SELECT * FROM lookup_bot_config_for_webhook($1)")
            .bind(bot_id)
            .fetch_optional(&self.pool)
            .await
    }
}
