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
