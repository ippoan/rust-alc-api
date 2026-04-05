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
    async fn list(&self, tenant_id: Uuid) -> Result<Vec<NotifyRecipient>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, NotifyRecipient>("SELECT * FROM notify_recipients ORDER BY name")
            .fetch_all(&mut *tc.conn)
            .await
    }

    async fn get(&self, tenant_id: Uuid, id: Uuid) -> Result<Option<NotifyRecipient>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, NotifyRecipient>("SELECT * FROM notify_recipients WHERE id = $1")
            .bind(id)
            .fetch_optional(&mut *tc.conn)
            .await
    }

    async fn create(
        &self,
        tenant_id: Uuid,
        input: &CreateNotifyRecipient,
    ) -> Result<NotifyRecipient, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, NotifyRecipient>(
            r#"
            INSERT INTO notify_recipients (tenant_id, name, provider, lineworks_user_id, line_user_id, phone_number, email)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            RETURNING *
            "#,
        )
        .bind(tenant_id)
        .bind(&input.name)
        .bind(&input.provider)
        .bind(&input.lineworks_user_id)
        .bind(&input.line_user_id)
        .bind(&input.phone_number)
        .bind(&input.email)
        .fetch_one(&mut *tc.conn)
        .await
    }

    async fn update(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        input: &UpdateNotifyRecipient,
    ) -> Result<NotifyRecipient, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, NotifyRecipient>(
            r#"
            UPDATE notify_recipients SET
                name = COALESCE($1, name),
                provider = COALESCE($2, provider),
                lineworks_user_id = COALESCE($3, lineworks_user_id),
                line_user_id = COALESCE($4, line_user_id),
                phone_number = COALESCE($5, phone_number),
                email = COALESCE($6, email),
                enabled = COALESCE($7, enabled),
                updated_at = NOW()
            WHERE id = $8
            RETURNING *
            "#,
        )
        .bind(&input.name)
        .bind(&input.provider)
        .bind(&input.lineworks_user_id)
        .bind(&input.line_user_id)
        .bind(&input.phone_number)
        .bind(&input.email)
        .bind(input.enabled)
        .bind(id)
        .fetch_one(&mut *tc.conn)
        .await
    }

    async fn delete(&self, tenant_id: Uuid, id: Uuid) -> Result<(), sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query("DELETE FROM notify_recipients WHERE id = $1")
            .bind(id)
            .execute(&mut *tc.conn)
            .await?;
        Ok(())
    }

    async fn list_enabled(&self, tenant_id: Uuid) -> Result<Vec<NotifyRecipient>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, NotifyRecipient>(
            "SELECT * FROM notify_recipients WHERE enabled = TRUE ORDER BY name",
        )
        .fetch_all(&mut *tc.conn)
        .await
    }

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
