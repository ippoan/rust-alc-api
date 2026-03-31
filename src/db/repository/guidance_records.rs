use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use crate::db::models::{
    CreateGuidanceRecord, GuidanceRecord, GuidanceRecordAttachment, UpdateGuidanceRecord,
};

use super::TenantConn;

pub use alc_core::repository::guidance_records::*;

pub struct PgGuidanceRecordsRepository {
    pool: PgPool,
}

impl PgGuidanceRecordsRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl GuidanceRecordsRepository for PgGuidanceRecordsRepository {
    async fn count_top_level(
        &self,
        tenant_id: Uuid,
        employee_id: Option<Uuid>,
        guidance_type: Option<&str>,
        date_from: Option<&str>,
        date_to: Option<&str>,
    ) -> Result<i64, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_scalar(
            r#"SELECT COUNT(*) FROM alc_api.guidance_records g
               WHERE g.parent_id IS NULL
                 AND ($1::UUID IS NULL OR g.employee_id = $1)
                 AND ($2::TEXT IS NULL OR g.guidance_type = $2)
                 AND ($3::TEXT IS NULL OR g.guided_at >= $3::TIMESTAMPTZ)
                 AND ($4::TEXT IS NULL OR g.guided_at < ($4::DATE + INTERVAL '1 day'))"#,
        )
        .bind(employee_id)
        .bind(guidance_type)
        .bind(date_from)
        .bind(date_to)
        .fetch_one(&mut *tc.conn)
        .await
    }

    async fn list_tree(
        &self,
        tenant_id: Uuid,
        employee_id: Option<Uuid>,
        guidance_type: Option<&str>,
        date_from: Option<&str>,
        date_to: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<GuidanceRecordWithName>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, GuidanceRecordWithName>(
            r#"WITH RECURSIVE top AS (
                SELECT g.id FROM alc_api.guidance_records g
                WHERE g.parent_id IS NULL
                  AND ($1::UUID IS NULL OR g.employee_id = $1)
                  AND ($2::TEXT IS NULL OR g.guidance_type = $2)
                  AND ($3::TEXT IS NULL OR g.guided_at >= $3::TIMESTAMPTZ)
                  AND ($4::TEXT IS NULL OR g.guided_at < ($4::DATE + INTERVAL '1 day'))
                ORDER BY g.guided_at DESC
                LIMIT $5 OFFSET $6
            ), tree AS (
                SELECT g.* FROM alc_api.guidance_records g WHERE g.id IN (SELECT id FROM top)
                UNION ALL
                SELECT g.* FROM alc_api.guidance_records g JOIN tree t ON g.parent_id = t.id WHERE g.depth < 3
            )
            SELECT t.*, e.name AS employee_name
            FROM tree t
            LEFT JOIN alc_api.employees e ON e.id = t.employee_id
            ORDER BY t.depth, t.guided_at DESC"#,
        )
        .bind(employee_id)
        .bind(guidance_type)
        .bind(date_from)
        .bind(date_to)
        .bind(limit)
        .bind(offset)
        .fetch_all(&mut *tc.conn)
        .await
    }

    async fn list_attachments_by_record_ids(
        &self,
        tenant_id: Uuid,
        record_ids: &[Uuid],
    ) -> Result<Vec<GuidanceRecordAttachment>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, GuidanceRecordAttachment>(
            "SELECT * FROM alc_api.guidance_record_attachments WHERE record_id = ANY($1) ORDER BY created_at",
        )
        .bind(record_ids)
        .fetch_all(&mut *tc.conn)
        .await
    }

    async fn get(&self, tenant_id: Uuid, id: Uuid) -> Result<Option<GuidanceRecord>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, GuidanceRecord>("SELECT * FROM alc_api.guidance_records WHERE id = $1")
            .bind(id)
            .fetch_optional(&mut *tc.conn)
            .await
    }

    async fn get_parent_depth(
        &self,
        tenant_id: Uuid,
        parent_id: Uuid,
    ) -> Result<Option<i32>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_scalar("SELECT depth FROM alc_api.guidance_records WHERE id = $1")
            .bind(parent_id)
            .fetch_optional(&mut *tc.conn)
            .await
    }

    async fn create(
        &self,
        tenant_id: Uuid,
        input: &CreateGuidanceRecord,
        depth: i32,
    ) -> Result<GuidanceRecord, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, GuidanceRecord>(
            r#"INSERT INTO alc_api.guidance_records
                   (tenant_id, employee_id, guidance_type, title, content, guided_by, guided_at, parent_id, depth)
               VALUES ($1, $2, $3, $4, $5, $6, COALESCE($7, now()), $8, $9)
               RETURNING *"#,
        )
        .bind(tenant_id)
        .bind(input.employee_id)
        .bind(input.guidance_type.as_deref().unwrap_or("general"))
        .bind(&input.title)
        .bind(input.content.as_deref().unwrap_or(""))
        .bind(&input.guided_by)
        .bind(input.guided_at)
        .bind(input.parent_id)
        .bind(depth)
        .fetch_one(&mut *tc.conn)
        .await
    }

    async fn update(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        input: &UpdateGuidanceRecord,
    ) -> Result<Option<GuidanceRecord>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, GuidanceRecord>(
            r#"UPDATE alc_api.guidance_records SET
                   guidance_type = COALESCE($1, guidance_type),
                   title = COALESCE($2, title),
                   content = COALESCE($3, content),
                   guided_by = COALESCE($4, guided_by),
                   guided_at = COALESCE($5, guided_at),
                   updated_at = now()
               WHERE id = $6
               RETURNING *"#,
        )
        .bind(&input.guidance_type)
        .bind(&input.title)
        .bind(&input.content)
        .bind(&input.guided_by)
        .bind(input.guided_at)
        .bind(id)
        .fetch_optional(&mut *tc.conn)
        .await
    }

    async fn delete_recursive(&self, tenant_id: Uuid, id: Uuid) -> Result<u64, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let result = sqlx::query(
            r#"WITH RECURSIVE descendants AS (
                SELECT id FROM alc_api.guidance_records WHERE id = $1
                UNION ALL
                SELECT g.id FROM alc_api.guidance_records g JOIN descendants d ON g.parent_id = d.id
            )
            DELETE FROM alc_api.guidance_records WHERE id IN (SELECT id FROM descendants)"#,
        )
        .bind(id)
        .execute(&mut *tc.conn)
        .await?;
        Ok(result.rows_affected())
    }

    async fn list_attachments(
        &self,
        tenant_id: Uuid,
        record_id: Uuid,
    ) -> Result<Vec<GuidanceRecordAttachment>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, GuidanceRecordAttachment>(
            "SELECT * FROM alc_api.guidance_record_attachments WHERE record_id = $1 ORDER BY created_at",
        )
        .bind(record_id)
        .fetch_all(&mut *tc.conn)
        .await
    }

    async fn create_attachment(
        &self,
        tenant_id: Uuid,
        record_id: Uuid,
        file_name: &str,
        file_type: &str,
        file_size: i32,
        storage_url: &str,
    ) -> Result<GuidanceRecordAttachment, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, GuidanceRecordAttachment>(
            r#"INSERT INTO alc_api.guidance_record_attachments (record_id, file_name, file_type, file_size, storage_url)
               VALUES ($1, $2, $3, $4, $5)
               RETURNING *"#,
        )
        .bind(record_id)
        .bind(file_name)
        .bind(file_type)
        .bind(file_size)
        .bind(storage_url)
        .fetch_one(&mut *tc.conn)
        .await
    }

    async fn get_attachment(
        &self,
        tenant_id: Uuid,
        record_id: Uuid,
        att_id: Uuid,
    ) -> Result<Option<GuidanceRecordAttachment>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, GuidanceRecordAttachment>(
            "SELECT * FROM alc_api.guidance_record_attachments WHERE id = $1 AND record_id = $2",
        )
        .bind(att_id)
        .bind(record_id)
        .fetch_optional(&mut *tc.conn)
        .await
    }

    async fn delete_attachment(
        &self,
        tenant_id: Uuid,
        record_id: Uuid,
        att_id: Uuid,
    ) -> Result<u64, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let result = sqlx::query(
            "DELETE FROM alc_api.guidance_record_attachments WHERE id = $1 AND record_id = $2",
        )
        .bind(att_id)
        .bind(record_id)
        .execute(&mut *tc.conn)
        .await?;
        Ok(result.rows_affected())
    }
}
