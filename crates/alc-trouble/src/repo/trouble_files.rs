use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use alc_core::models::TroubleFile;
use alc_core::tenant::TenantConn;

pub use alc_core::repository::trouble_files::*;

pub struct PgTroubleFilesRepository {
    pool: PgPool,
}

impl PgTroubleFilesRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl TroubleFilesRepository for PgTroubleFilesRepository {
    async fn create(
        &self,
        tenant_id: Uuid,
        ticket_id: Uuid,
        filename: &str,
        content_type: &str,
        size_bytes: i64,
        storage_key: &str,
    ) -> Result<TroubleFile, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, TroubleFile>(
            r#"INSERT INTO trouble_files (tenant_id, ticket_id, filename, content_type, size_bytes, storage_key)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING *"#,
        )
        .bind(tenant_id)
        .bind(ticket_id)
        .bind(filename)
        .bind(content_type)
        .bind(size_bytes)
        .bind(storage_key)
        .fetch_one(&mut *tc.conn)
        .await
    }

    async fn list_by_ticket(
        &self,
        tenant_id: Uuid,
        ticket_id: Uuid,
    ) -> Result<Vec<TroubleFile>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, TroubleFile>(
            "SELECT * FROM trouble_files WHERE ticket_id = $1 AND tenant_id = $2 ORDER BY created_at",
        )
        .bind(ticket_id)
        .bind(tenant_id)
        .fetch_all(&mut *tc.conn)
        .await
    }

    async fn get(&self, tenant_id: Uuid, id: Uuid) -> Result<Option<TroubleFile>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, TroubleFile>(
            "SELECT * FROM trouble_files WHERE id = $1 AND tenant_id = $2",
        )
        .bind(id)
        .bind(tenant_id)
        .fetch_optional(&mut *tc.conn)
        .await
    }

    async fn delete(&self, tenant_id: Uuid, id: Uuid) -> Result<bool, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let result = sqlx::query("DELETE FROM trouble_files WHERE id = $1 AND tenant_id = $2")
            .bind(id)
            .bind(tenant_id)
            .execute(&mut *tc.conn)
            .await?;
        Ok(result.rows_affected() > 0)
    }
}
