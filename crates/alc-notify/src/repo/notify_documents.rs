use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use alc_core::repository::notify_documents::*;
use alc_core::tenant::TenantConn;

pub struct PgNotifyDocumentRepository {
    pool: PgPool,
}

impl PgNotifyDocumentRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl NotifyDocumentRepository for PgNotifyDocumentRepository {
    async fn create(
        &self,
        tenant_id: Uuid,
        input: &CreateNotifyDocument,
    ) -> Result<NotifyDocument, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, NotifyDocument>(
            r#"
            INSERT INTO notify_documents (tenant_id, source_type, source_sender, source_subject, r2_key, file_name, file_size_bytes)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            RETURNING *
            "#,
        )
        .bind(tenant_id)
        .bind(&input.source_type)
        .bind(&input.source_sender)
        .bind(&input.source_subject)
        .bind(&input.r2_key)
        .bind(&input.file_name)
        .bind(input.file_size_bytes)
        .fetch_one(&mut *tc.conn)
        .await
    }

    async fn get(&self, tenant_id: Uuid, id: Uuid) -> Result<Option<NotifyDocument>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, NotifyDocument>("SELECT * FROM notify_documents WHERE id = $1")
            .bind(id)
            .fetch_optional(&mut *tc.conn)
            .await
    }

    async fn list(
        &self,
        tenant_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<NotifyDocument>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, NotifyDocument>(
            "SELECT * FROM notify_documents ORDER BY created_at DESC LIMIT $1 OFFSET $2",
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&mut *tc.conn)
        .await
    }

    async fn search(
        &self,
        tenant_id: Uuid,
        query: &str,
    ) -> Result<Vec<NotifyDocument>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let pattern = format!("%{}%", query);
        sqlx::query_as::<_, NotifyDocument>(
            r#"
            SELECT * FROM notify_documents
            WHERE extracted_title ILIKE $1
               OR extracted_summary ILIKE $1
               OR source_subject ILIKE $1
               OR $2 = ANY(extracted_phone_numbers)
            ORDER BY created_at DESC
            LIMIT 100
            "#,
        )
        .bind(&pattern)
        .bind(query)
        .fetch_all(&mut *tc.conn)
        .await
    }

    async fn update_extraction(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        result: &ExtractionResult,
    ) -> Result<(), sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query(
            r#"
            UPDATE notify_documents SET
                extracted_title = $1,
                extracted_date = $2,
                extracted_summary = $3,
                extracted_phone_numbers = $4,
                extracted_data = $5,
                extraction_status = 'completed',
                updated_at = NOW()
            WHERE id = $6
            "#,
        )
        .bind(&result.title)
        .bind(result.date)
        .bind(&result.summary)
        .bind(&result.phone_numbers)
        .bind(&result.data)
        .bind(id)
        .execute(&mut *tc.conn)
        .await?;
        Ok(())
    }

    async fn update_extraction_error(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        error: &str,
    ) -> Result<(), sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query(
            r#"
            UPDATE notify_documents SET
                extraction_status = 'failed',
                extraction_error = $1,
                updated_at = NOW()
            WHERE id = $2
            "#,
        )
        .bind(error)
        .bind(id)
        .execute(&mut *tc.conn)
        .await?;
        Ok(())
    }

    async fn update_distribution_status(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        status: &str,
    ) -> Result<(), sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let distributed_at = if status == "completed" {
            Some(chrono::Utc::now())
        } else {
            None
        };
        sqlx::query(
            r#"
            UPDATE notify_documents SET
                distribution_status = $1,
                distributed_at = COALESCE($2, distributed_at),
                updated_at = NOW()
            WHERE id = $3
            "#,
        )
        .bind(status)
        .bind(distributed_at)
        .bind(id)
        .execute(&mut *tc.conn)
        .await?;
        Ok(())
    }
}
