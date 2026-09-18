//! 整備記録の添付ファイル API (`maintenance_files`)。Refs ippoan/rust-alc-api#651。
//!
//! `crates/alc-trouble/src/files.rs` が直接の手本。あちらは専用の `trouble_storage`
//! (R2) を使うが、こちらは新しい `*_R2_BUCKET` を足さず、`src/main.rs` で組み立てて
//! いる既定の共有 `storage: Arc<dyn StorageBackend>` に prefix
//! `{tenant_id}/maintenance/{record_id}/{file_uuid}.{ext}` で相乗りする。
//!
//! 署名 URL は使わない (この repo に署名 URL 方式は 1 つもない) — download は
//! サーバが storage から取って bytes をそのまま返す。
//!
//! サムネイル生成はしない — `content_type` をそのまま保存・返却するだけ
//! (フロントが原寸を縮小表示する)。
//!
//! ## テナント境界 (ここが要点)
//!
//! `record_id` / `file_id` のどちらも `tenant_id` を明示述語で確認してから storage
//! を引く (RLS 任せにしない)。`storage_key` にテナント ID が入っていても、それを
//! 認可の根拠にしない — キーを推測されたら終わりなので、DB 側で先にテナント一致を
//! 確認する。

use async_trait::async_trait;
use axum::{
    extract::{Multipart, Path, State},
    http::StatusCode,
    routing::{delete, get, post},
    Json, Router,
};
use sqlx::PgPool;
use uuid::Uuid;

use alc_core::auth_middleware::TenantId;
use alc_core::storage::StorageBackend;
use alc_core::tenant::TenantConn;

use crate::models::MaintenanceFile;
use crate::MaintenanceState;

/// `maintenance_files` への読み書きと、添付先 `maintenance_records` のテナント
/// 所属確認。テスト側は mock 実装に差し替える。
#[async_trait]
pub trait MaintenanceFilesRepository: Send + Sync {
    /// `record_id` が `tenant_id` の記録であること (かつソフト削除されていないこと)
    /// を確認する。添付 (`upload_file`) と一覧 (`list_files`) の両方が、storage を
    /// 引く前にこれを通す。
    async fn record_belongs_to_tenant(
        &self,
        tenant_id: Uuid,
        record_id: Uuid,
    ) -> Result<bool, sqlx::Error>;

    #[allow(clippy::too_many_arguments)]
    async fn create(
        &self,
        tenant_id: Uuid,
        record_id: Uuid,
        filename: &str,
        content_type: &str,
        size_bytes: i64,
        storage_key: &str,
    ) -> Result<MaintenanceFile, sqlx::Error>;

    async fn list_by_record(
        &self,
        tenant_id: Uuid,
        record_id: Uuid,
    ) -> Result<Vec<MaintenanceFile>, sqlx::Error>;

    async fn get(&self, tenant_id: Uuid, id: Uuid) -> Result<Option<MaintenanceFile>, sqlx::Error>;

    async fn soft_delete(&self, tenant_id: Uuid, id: Uuid) -> Result<bool, sqlx::Error>;
}

pub struct PgMaintenanceFilesRepository {
    pool: PgPool,
}

impl PgMaintenanceFilesRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl MaintenanceFilesRepository for PgMaintenanceFilesRepository {
    async fn record_belongs_to_tenant(
        &self,
        tenant_id: Uuid,
        record_id: Uuid,
    ) -> Result<bool, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM maintenance_records \
             WHERE id = $1 AND tenant_id = $2 AND deleted_at IS NULL)",
        )
        .bind(record_id)
        .bind(tenant_id)
        .fetch_one(&mut *tc.conn)
        .await
    }

    async fn create(
        &self,
        tenant_id: Uuid,
        record_id: Uuid,
        filename: &str,
        content_type: &str,
        size_bytes: i64,
        storage_key: &str,
    ) -> Result<MaintenanceFile, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, MaintenanceFile>(
            r#"INSERT INTO maintenance_files (tenant_id, record_id, filename, content_type, size_bytes, storage_key)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING *"#,
        )
        .bind(tenant_id)
        .bind(record_id)
        .bind(filename)
        .bind(content_type)
        .bind(size_bytes)
        .bind(storage_key)
        .fetch_one(&mut *tc.conn)
        .await
    }

    async fn list_by_record(
        &self,
        tenant_id: Uuid,
        record_id: Uuid,
    ) -> Result<Vec<MaintenanceFile>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, MaintenanceFile>(
            "SELECT * FROM maintenance_files \
             WHERE record_id = $1 AND tenant_id = $2 AND deleted_at IS NULL \
             ORDER BY created_at",
        )
        .bind(record_id)
        .bind(tenant_id)
        .fetch_all(&mut *tc.conn)
        .await
    }

    async fn get(&self, tenant_id: Uuid, id: Uuid) -> Result<Option<MaintenanceFile>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, MaintenanceFile>(
            "SELECT * FROM maintenance_files WHERE id = $1 AND tenant_id = $2 AND deleted_at IS NULL",
        )
        .bind(id)
        .bind(tenant_id)
        .fetch_optional(&mut *tc.conn)
        .await
    }

    async fn soft_delete(&self, tenant_id: Uuid, id: Uuid) -> Result<bool, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let result = sqlx::query(
            "UPDATE maintenance_files SET deleted_at = now() \
             WHERE id = $1 AND tenant_id = $2 AND deleted_at IS NULL",
        )
        .bind(id)
        .bind(tenant_id)
        .execute(&mut *tc.conn)
        .await?;
        Ok(result.rows_affected() > 0)
    }
}

pub fn tenant_router<S>() -> Router<S>
where
    MaintenanceState: axum::extract::FromRef<S>,
    S: Clone + Send + Sync + 'static,
{
    Router::new()
        .route(
            "/maintenance/records/{record_id}/files",
            post(upload_file).get(list_files),
        )
        .route("/maintenance/files/{file_id}/download", get(download_file))
        .route("/maintenance/files/{file_id}", delete(delete_file))
}

async fn upload_file(
    State(state): State<MaintenanceState>,
    tenant: axum::Extension<TenantId>,
    Path(record_id): Path<Uuid>,
    mut multipart: Multipart,
) -> Result<(StatusCode, Json<MaintenanceFile>), StatusCode> {
    let tenant_id = tenant.0 .0;

    // record_id が自テナントのものであることを確認してから添付する
    // (storage_key にテナント ID が入っていてもそれを認可の根拠にしない)。
    let belongs = state
        .files
        .record_belongs_to_tenant(tenant_id, record_id)
        .await
        .map_err(|e| {
            tracing::error!("record_belongs_to_tenant error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    if !belongs {
        return Err(StatusCode::NOT_FOUND);
    }

    let storage: &std::sync::Arc<dyn StorageBackend> = state
        .storage
        .as_ref()
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;

    let field = multipart
        .next_field()
        .await
        .map_err(|_| StatusCode::BAD_REQUEST)?
        .ok_or(StatusCode::BAD_REQUEST)?;

    let filename = field.file_name().unwrap_or("unknown").to_string();
    let content_type = field
        .content_type()
        .unwrap_or("application/octet-stream")
        .to_string();
    let data = field.bytes().await.map_err(|_| StatusCode::BAD_REQUEST)?;
    let size_bytes = data.len() as i64;

    let file_uuid = Uuid::new_v4();
    let ext = filename.rsplit('.').next().unwrap_or("bin");
    let storage_key = format!("{tenant_id}/maintenance/{record_id}/{file_uuid}.{ext}");

    storage
        .upload(&storage_key, &data, &content_type)
        .await
        .map_err(|e| {
            tracing::error!("storage upload error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let file = state
        .files
        .create(
            tenant_id,
            record_id,
            &filename,
            &content_type,
            size_bytes,
            &storage_key,
        )
        .await
        .map_err(|e| {
            tracing::error!("create maintenance_file DB error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok((StatusCode::CREATED, Json(file)))
}

async fn list_files(
    State(state): State<MaintenanceState>,
    tenant: axum::Extension<TenantId>,
    Path(record_id): Path<Uuid>,
) -> Result<Json<Vec<MaintenanceFile>>, StatusCode> {
    let tenant_id = tenant.0 .0;

    let belongs = state
        .files
        .record_belongs_to_tenant(tenant_id, record_id)
        .await
        .map_err(|e| {
            tracing::error!("record_belongs_to_tenant error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    if !belongs {
        return Err(StatusCode::NOT_FOUND);
    }

    let files = state
        .files
        .list_by_record(tenant_id, record_id)
        .await
        .map_err(|e| {
            tracing::error!("list_by_record error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(Json(files))
}

async fn download_file(
    State(state): State<MaintenanceState>,
    tenant: axum::Extension<TenantId>,
    Path(file_id): Path<Uuid>,
) -> Result<impl axum::response::IntoResponse, StatusCode> {
    let file = state
        .files
        .get(tenant.0 .0, file_id)
        .await
        .map_err(|e| {
            tracing::error!("get maintenance_file error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    let storage: &std::sync::Arc<dyn StorageBackend> = state
        .storage
        .as_ref()
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;

    let data = storage.download(&file.storage_key).await.map_err(|e| {
        tracing::error!("storage download error: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok((
        [
            (axum::http::header::CONTENT_TYPE, file.content_type.clone()),
            (
                axum::http::header::CONTENT_DISPOSITION,
                format!(
                    "attachment; filename=\"{}\"",
                    file.filename.replace('"', "_")
                ),
            ),
        ],
        data,
    ))
}

async fn delete_file(
    State(state): State<MaintenanceState>,
    tenant: axum::Extension<TenantId>,
    Path(file_id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let deleted = state
        .files
        .soft_delete(tenant.0 .0, file_id)
        .await
        .map_err(|e| {
            tracing::error!("soft_delete maintenance_file error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}
