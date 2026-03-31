use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use super::TenantConn;

pub use alc_core::repository::carins_files::*;

const FILE_SELECT: &str = r#"
    uuid::text, filename, type as file_type,
    to_char(created_at, 'YYYY-MM-DD"T"HH24:MI:SS"Z"') as created,
    to_char(deleted_at, 'YYYY-MM-DD"T"HH24:MI:SS"Z"') as deleted,
    NULL as blob, s3_key, storage_class,
    to_char(last_accessed_at, 'YYYY-MM-DD"T"HH24:MI:SS"Z"') as last_accessed_at,
    access_count_weekly, access_count_total,
    to_char(promoted_to_standard_at, 'YYYY-MM-DD"T"HH24:MI:SS"Z"') as promoted_to_standard_at
"#;

const FILE_SELECT_F: &str = r#"
    uuid::text, f.filename, f.type as file_type,
    to_char(f.created_at, 'YYYY-MM-DD"T"HH24:MI:SS"Z"') as created,
    to_char(f.deleted_at, 'YYYY-MM-DD"T"HH24:MI:SS"Z"') as deleted,
    NULL as blob, f.s3_key, f.storage_class,
    to_char(f.last_accessed_at, 'YYYY-MM-DD"T"HH24:MI:SS"Z"') as last_accessed_at,
    f.access_count_weekly, f.access_count_total,
    to_char(f.promoted_to_standard_at, 'YYYY-MM-DD"T"HH24:MI:SS"Z"') as promoted_to_standard_at
"#;

pub struct PgCarinsFilesRepository {
    pool: PgPool,
}

impl PgCarinsFilesRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl CarinsFilesRepository for PgCarinsFilesRepository {
    async fn list_files(
        &self,
        tenant_id: Uuid,
        type_filter: Option<&str>,
    ) -> Result<Vec<FileRow>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        if let Some(t) = type_filter {
            sqlx::query_as::<_, FileRow>(&format!(
                "SELECT {FILE_SELECT} FROM files WHERE deleted_at IS NULL AND type = $1 ORDER BY created_at DESC"
            ))
            .bind(t)
            .fetch_all(&mut *tc.conn)
            .await
        } else {
            sqlx::query_as::<_, FileRow>(&format!(
                "SELECT {FILE_SELECT} FROM files WHERE deleted_at IS NULL ORDER BY created_at DESC"
            ))
            .fetch_all(&mut *tc.conn)
            .await
        }
    }

    async fn list_recent(&self, tenant_id: Uuid) -> Result<Vec<FileRow>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, FileRow>(&format!(
            "SELECT {FILE_SELECT} FROM files WHERE deleted_at IS NULL ORDER BY created_at DESC LIMIT 50"
        ))
        .fetch_all(&mut *tc.conn)
        .await
    }

    async fn list_not_attached(&self, tenant_id: Uuid) -> Result<Vec<FileRow>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, FileRow>(&format!(
            r#"SELECT f.{FILE_SELECT_F}
            FROM files f
            LEFT JOIN car_inspection_files_a cif ON f.uuid = cif.uuid
            WHERE f.deleted_at IS NULL AND cif.uuid IS NULL
            ORDER BY f.created_at DESC"#
        ))
        .fetch_all(&mut *tc.conn)
        .await
    }

    async fn get_file(&self, tenant_id: Uuid, uuid: &str) -> Result<Option<FileRow>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, FileRow>(&format!(
            "SELECT {FILE_SELECT} FROM files WHERE uuid = $1::uuid"
        ))
        .bind(uuid)
        .fetch_optional(&mut *tc.conn)
        .await
    }

    async fn get_file_for_download(
        &self,
        tenant_id: Uuid,
        uuid: &str,
    ) -> Result<Option<FileRow>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, FileRow>(
            "SELECT uuid::text, filename, type as file_type, \
             to_char(created_at, 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') as created, \
             to_char(deleted_at, 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') as deleted, \
             blob, s3_key, storage_class, \
             to_char(last_accessed_at, 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') as last_accessed_at, \
             access_count_weekly, access_count_total, \
             to_char(promoted_to_standard_at, 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') as promoted_to_standard_at \
             FROM files WHERE uuid = $1::uuid",
        )
        .bind(uuid)
        .fetch_optional(&mut *tc.conn)
        .await
    }

    async fn create_file(
        &self,
        tenant_id: Uuid,
        file_uuid: Uuid,
        filename: &str,
        file_type: &str,
        gcs_key: &str,
        now: DateTime<Utc>,
    ) -> Result<FileRow, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, FileRow>(&format!(
            "INSERT INTO files (uuid, tenant_id, filename, type, created_at, s3_key, storage_class, last_accessed_at) \
             VALUES ($1::uuid, $2::uuid, $3, $4, $5, $6, 'STANDARD', $5) \
             RETURNING {FILE_SELECT}"
        ))
        .bind(file_uuid.to_string())
        .bind(tenant_id.to_string())
        .bind(filename)
        .bind(file_type)
        .bind(now)
        .bind(gcs_key)
        .fetch_one(&mut *tc.conn)
        .await
    }

    async fn delete_file(&self, tenant_id: Uuid, uuid: &str) -> Result<bool, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let now = chrono::Utc::now();
        let result = sqlx::query("UPDATE files SET deleted_at = $1 WHERE uuid = $2::uuid")
            .bind(now)
            .bind(uuid)
            .execute(&mut *tc.conn)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn restore_file(&self, tenant_id: Uuid, uuid: &str) -> Result<bool, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let result = sqlx::query("UPDATE files SET deleted_at = NULL WHERE uuid = $1::uuid")
            .bind(uuid)
            .execute(&mut *tc.conn)
            .await?;
        Ok(result.rows_affected() > 0)
    }
}
