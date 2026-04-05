use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use alc_core::repository::notify_deliveries::*;
use alc_core::tenant::TenantConn;

pub struct PgNotifyDeliveryRepository {
    pool: PgPool,
}

impl PgNotifyDeliveryRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl NotifyDeliveryRepository for PgNotifyDeliveryRepository {
    async fn create_batch(
        &self,
        tenant_id: Uuid,
        document_id: Uuid,
        recipients: &[(Uuid, String)],
    ) -> Result<Vec<NotifyDelivery>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let mut deliveries = Vec::with_capacity(recipients.len());
        for (recipient_id, provider) in recipients {
            let d = sqlx::query_as::<_, NotifyDelivery>(
                r#"
                INSERT INTO notify_deliveries (tenant_id, document_id, recipient_id, provider)
                VALUES ($1, $2, $3, $4)
                RETURNING *
                "#,
            )
            .bind(tenant_id)
            .bind(document_id)
            .bind(recipient_id)
            .bind(provider)
            .fetch_one(&mut *tc.conn)
            .await?;
            deliveries.push(d);
        }
        Ok(deliveries)
    }

    async fn update_status(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        status: &str,
        error: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query(
            r#"
            UPDATE notify_deliveries SET
                status = $1,
                error_message = $2,
                attempt = attempt + 1
            WHERE id = $3
            "#,
        )
        .bind(status)
        .bind(error)
        .bind(id)
        .execute(&mut *tc.conn)
        .await?;
        Ok(())
    }

    async fn mark_sent(&self, tenant_id: Uuid, id: Uuid) -> Result<(), sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query(
            "UPDATE notify_deliveries SET status = 'sent', sent_at = NOW(), attempt = attempt + 1 WHERE id = $1",
        )
        .bind(id)
        .execute(&mut *tc.conn)
        .await?;
        Ok(())
    }

    async fn mark_read(&self, read_token: Uuid) -> Result<Option<ReadResult>, sqlx::Error> {
        // SECURITY DEFINER 関数経由 — TenantConn 不要
        sqlx::query_as::<_, ReadResult>("SELECT * FROM alc_api.mark_delivery_read($1)")
            .bind(read_token)
            .fetch_optional(&self.pool)
            .await
    }

    async fn list_by_document(
        &self,
        tenant_id: Uuid,
        document_id: Uuid,
    ) -> Result<Vec<NotifyDeliveryWithRecipient>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, NotifyDeliveryWithRecipient>(
            r#"
            SELECT d.id, d.document_id, d.recipient_id, d.provider, d.status,
                   d.error_message, d.attempt, d.sent_at, d.read_at, d.read_token, d.created_at,
                   r.name AS recipient_name
            FROM notify_deliveries d
            JOIN notify_recipients r ON r.id = d.recipient_id
            WHERE d.document_id = $1
            ORDER BY r.name
            "#,
        )
        .bind(document_id)
        .fetch_all(&mut *tc.conn)
        .await
    }

    async fn list_pending(
        &self,
        tenant_id: Uuid,
        document_id: Uuid,
    ) -> Result<Vec<NotifyDelivery>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, NotifyDelivery>(
            "SELECT * FROM notify_deliveries WHERE document_id = $1 AND status = 'pending'",
        )
        .bind(document_id)
        .fetch_all(&mut *tc.conn)
        .await
    }
}
