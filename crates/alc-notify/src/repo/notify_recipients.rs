use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use alc_core::repository::notify_recipients::*;
use alc_core::tenant::TenantConn;

pub struct PgNotifyRecipientRepository {
    pool: PgPool,
}

impl PgNotifyRecipientRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl NotifyRecipientRepository for PgNotifyRecipientRepository {
    async fn upsert_by_line_user_id(
        &self,
        tenant_id: Uuid,
        line_user_id: &str,
        name: &str,
    ) -> Result<NotifyRecipient, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, NotifyRecipient>(
            r#"
            INSERT INTO notify_recipients (tenant_id, name, provider, line_user_id)
            VALUES ($1, $2, 'line', $3)
            ON CONFLICT (tenant_id, line_user_id) WHERE line_user_id IS NOT NULL
            DO UPDATE SET name = EXCLUDED.name, updated_at = NOW()
            RETURNING *
            "#,
        )
        .bind(tenant_id)
        .bind(name)
        .bind(line_user_id)
        .fetch_one(&mut *tc.conn)
        .await
    }
}
