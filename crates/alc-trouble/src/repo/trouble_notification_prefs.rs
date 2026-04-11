use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use alc_core::models::{TroubleNotificationPref, UpsertNotificationPref};
use alc_core::tenant::TenantConn;

pub use alc_core::repository::trouble_notification_prefs::*;

pub struct PgTroubleNotificationPrefsRepository {
    pool: PgPool,
}

impl PgTroubleNotificationPrefsRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl TroubleNotificationPrefsRepository for PgTroubleNotificationPrefsRepository {
    async fn upsert(
        &self,
        tenant_id: Uuid,
        input: &UpsertNotificationPref,
    ) -> Result<TroubleNotificationPref, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, TroubleNotificationPref>(
            r#"
            INSERT INTO trouble_notification_prefs
                (tenant_id, event_type, notify_channel, enabled, recipient_ids, notify_admins, lineworks_user_ids)
            VALUES ($1, $2, $3, COALESCE($4, TRUE), COALESCE($5, '{}'), COALESCE($6, FALSE), COALESCE($7, '{}'))
            ON CONFLICT (tenant_id, event_type, notify_channel)
            DO UPDATE SET
                enabled = COALESCE($4, trouble_notification_prefs.enabled),
                recipient_ids = COALESCE($5, trouble_notification_prefs.recipient_ids),
                notify_admins = COALESCE($6, trouble_notification_prefs.notify_admins),
                lineworks_user_ids = COALESCE($7, trouble_notification_prefs.lineworks_user_ids),
                updated_at = NOW()
            RETURNING *
            "#,
        )
        .bind(tenant_id)
        .bind(&input.event_type)
        .bind(&input.notify_channel)
        .bind(input.enabled)
        .bind(&input.recipient_ids)
        .bind(input.notify_admins)
        .bind(&input.lineworks_user_ids)
        .fetch_one(&mut *tc.conn)
        .await
    }

    async fn list(&self, tenant_id: Uuid) -> Result<Vec<TroubleNotificationPref>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, TroubleNotificationPref>(
            "SELECT * FROM trouble_notification_prefs WHERE tenant_id = $1 ORDER BY event_type, notify_channel",
        )
        .bind(tenant_id)
        .fetch_all(&mut *tc.conn)
        .await
    }

    async fn delete(&self, tenant_id: Uuid, id: Uuid) -> Result<bool, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let result =
            sqlx::query("DELETE FROM trouble_notification_prefs WHERE id = $1 AND tenant_id = $2")
                .bind(id)
                .bind(tenant_id)
                .execute(&mut *tc.conn)
                .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn find_enabled(
        &self,
        tenant_id: Uuid,
        event_type: &str,
        channel: &str,
    ) -> Result<Option<TroubleNotificationPref>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, TroubleNotificationPref>(
            r#"
            SELECT * FROM trouble_notification_prefs
            WHERE tenant_id = $1 AND event_type = $2 AND notify_channel = $3 AND enabled = TRUE
            "#,
        )
        .bind(tenant_id)
        .bind(event_type)
        .bind(channel)
        .fetch_optional(&mut *tc.conn)
        .await
    }
}
