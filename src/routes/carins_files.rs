use axum::{
    extract::{Path, Query, State},
    http::{header, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Extension, Json, Router,
};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

use crate::db::tenant::set_current_tenant;
use crate::middleware::auth::TenantId;
use crate::AppState;

pub fn tenant_router() -> Router<AppState> {
    Router::new()
        .route("/files", get(list_files).post(create_file))
        .route("/files/recent", get(list_recent))
        .route("/files/not-attached", get(list_not_attached))
        .route("/files/{uuid}", get(get_file))
        .route("/files/{uuid}/download", get(download_file))
        .route("/files/{uuid}/delete", post(delete_file))
        .route("/files/{uuid}/restore", post(restore_file))
}

#[derive(Debug, Clone, FromRow, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileRow {
    pub uuid: String,
    pub filename: String,
    pub file_type: String,
    pub created: String,
    pub deleted: Option<String>,
    pub blob: Option<String>,
    pub s3_key: Option<String>,
    pub storage_class: Option<String>,
    pub last_accessed_at: Option<String>,
    pub access_count_weekly: Option<i32>,
    pub access_count_total: Option<i32>,
    pub promoted_to_standard_at: Option<String>,
}

const FILE_SELECT: &str = r#"
    uuid::text, filename, type as file_type,
    to_char(created_at, 'YYYY-MM-DD"T"HH24:MI:SS"Z"') as created,
    to_char(deleted_at, 'YYYY-MM-DD"T"HH24:MI:SS"Z"') as deleted,
    NULL as blob, s3_key, storage_class,
    to_char(last_accessed_at, 'YYYY-MM-DD"T"HH24:MI:SS"Z"') as last_accessed_at,
    access_count_weekly, access_count_total,
    to_char(promoted_to_standard_at, 'YYYY-MM-DD"T"HH24:MI:SS"Z"') as promoted_to_standard_at
"#;

#[derive(Debug, Serialize)]
struct ListResponse {
    files: Vec<FileRow>,
}

#[derive(Debug, Deserialize)]
struct ListQuery {
    #[serde(rename = "type")]
    type_filter: Option<String>,
}

async fn list_files(
    State(state): State<AppState>,
    Extension(tenant_id): Extension<TenantId>,
    Query(q): Query<ListQuery>,
) -> Result<Json<ListResponse>, StatusCode> {
    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.0.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let rows = if let Some(ref t) = q.type_filter {
        sqlx::query_as::<_, FileRow>(
            &format!("SELECT {FILE_SELECT} FROM files WHERE deleted_at IS NULL AND type = $1 ORDER BY created_at DESC"),
        )
        .bind(t)
        .fetch_all(&mut *conn)
        .await
    } else {
        sqlx::query_as::<_, FileRow>(
            &format!("SELECT {FILE_SELECT} FROM files WHERE deleted_at IS NULL ORDER BY created_at DESC"),
        )
        .fetch_all(&mut *conn)
        .await
    }
    .map_err(|e| {
        tracing::error!("list_files failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(ListResponse { files: rows }))
}

async fn list_recent(
    State(state): State<AppState>,
    Extension(tenant_id): Extension<TenantId>,
) -> Result<Json<ListResponse>, StatusCode> {
    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.0.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let rows = sqlx::query_as::<_, FileRow>(
        &format!("SELECT {FILE_SELECT} FROM files WHERE deleted_at IS NULL ORDER BY created_at DESC LIMIT 50"),
    )
    .fetch_all(&mut *conn)
    .await
    .map_err(|e| {
        tracing::error!("list_recent failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(ListResponse { files: rows }))
}

async fn list_not_attached(
    State(state): State<AppState>,
    Extension(tenant_id): Extension<TenantId>,
) -> Result<Json<ListResponse>, StatusCode> {
    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.0.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let rows = sqlx::query_as::<_, FileRow>(&format!(
        r#"SELECT f.{FILE_SELECT_F}
            FROM files f
            LEFT JOIN car_inspection_files_a cif ON f.uuid = cif.uuid
            WHERE f.deleted_at IS NULL AND cif.uuid IS NULL
            ORDER BY f.created_at DESC"#
    ))
    .fetch_all(&mut *conn)
    .await
    .map_err(|e| {
        tracing::error!("list_not_attached failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(ListResponse { files: rows }))
}

const FILE_SELECT_F: &str = r#"
    uuid::text, f.filename, f.type as file_type,
    to_char(f.created_at, 'YYYY-MM-DD"T"HH24:MI:SS"Z"') as created,
    to_char(f.deleted_at, 'YYYY-MM-DD"T"HH24:MI:SS"Z"') as deleted,
    NULL as blob, f.s3_key, f.storage_class,
    to_char(f.last_accessed_at, 'YYYY-MM-DD"T"HH24:MI:SS"Z"') as last_accessed_at,
    f.access_count_weekly, f.access_count_total,
    to_char(f.promoted_to_standard_at, 'YYYY-MM-DD"T"HH24:MI:SS"Z"') as promoted_to_standard_at
"#;

async fn get_file(
    State(state): State<AppState>,
    Extension(tenant_id): Extension<TenantId>,
    Path(uuid): Path<String>,
) -> Result<Json<FileRow>, StatusCode> {
    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.0.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let row = sqlx::query_as::<_, FileRow>(&format!(
        "SELECT {FILE_SELECT} FROM files WHERE uuid = $1::uuid"
    ))
    .bind(&uuid)
    .fetch_optional(&mut *conn)
    .await
    .map_err(|e| {
        tracing::error!("get_file failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?
    .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(row))
}

async fn download_file(
    State(state): State<AppState>,
    Extension(tenant_id): Extension<TenantId>,
    Path(uuid): Path<String>,
) -> Result<impl IntoResponse, StatusCode> {
    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.0.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Get file metadata
    let row = sqlx::query_as::<_, FileRow>(
        "SELECT uuid::text, filename, type as file_type, \
             to_char(created_at, 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') as created, \
             to_char(deleted_at, 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') as deleted, \
             blob, s3_key, storage_class, \
             to_char(last_accessed_at, 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') as last_accessed_at, \
             access_count_weekly, access_count_total, \
             to_char(promoted_to_standard_at, 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') as promoted_to_standard_at \
             FROM files WHERE uuid = $1::uuid",
    )
    .bind(&uuid)
    .fetch_optional(&mut *conn)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;

    // Download from GCS
    if let Some(ref s3_key) = row.s3_key {
        let data = state
            .carins_storage
            .as_ref()
            .unwrap_or(&state.storage)
            .download(s3_key)
            .await
            .map_err(|e| {
                tracing::error!("GCS download failed: {e}");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;

        let content_type = row.file_type.clone();
        let filename = row.filename.clone();

        Ok((
            [
                (header::CONTENT_TYPE, content_type),
                (
                    header::CONTENT_DISPOSITION,
                    format!("attachment; filename=\"{}\"", filename),
                ),
            ],
            data,
        ))
    } else if let Some(ref blob) = row.blob {
        // Legacy blob storage (base64)
        use base64::{engine::general_purpose::STANDARD, Engine};
        let data = STANDARD
            .decode(blob)
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        let content_type = row.file_type.clone();
        let filename = row.filename.clone();

        Ok((
            [
                (header::CONTENT_TYPE, content_type),
                (
                    header::CONTENT_DISPOSITION,
                    format!("attachment; filename=\"{}\"", filename),
                ),
            ],
            data,
        ))
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

#[derive(Debug, Deserialize)]
struct CreateFileRequest {
    filename: String,
    #[serde(rename = "type")]
    file_type: String,
    content: String, // base64 encoded
}

async fn create_file(
    State(state): State<AppState>,
    Extension(tenant_id): Extension<TenantId>,
    Json(body): Json<CreateFileRequest>,
) -> Result<(StatusCode, Json<FileRow>), StatusCode> {
    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.0.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let file_uuid = Uuid::new_v4();
    let now = chrono::Utc::now();
    let gcs_key = format!("{}/{}", tenant_id.0, file_uuid);

    // Decode base64 and upload to GCS
    use base64::{engine::general_purpose::STANDARD, Engine};
    let data = STANDARD
        .decode(&body.content)
        .map_err(|_| StatusCode::BAD_REQUEST)?;

    state
        .storage
        .upload(&gcs_key, &data, &body.file_type)
        .await
        .map_err(|e| {
            tracing::error!("GCS upload failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let row = sqlx::query_as::<_, FileRow>(
        &format!(
            "INSERT INTO files (uuid, tenant_id, filename, type, created_at, s3_key, storage_class, last_accessed_at) \
             VALUES ($1::uuid, $2::uuid, $3, $4, $5, $6, 'STANDARD', $5) \
             RETURNING {FILE_SELECT}"
        ),
    )
    .bind(file_uuid.to_string())
    .bind(tenant_id.0.to_string())
    .bind(&body.filename)
    .bind(&body.file_type)
    .bind(now)
    .bind(&gcs_key)
    .fetch_one(&mut *conn)
    .await
    .map_err(|e| {
        tracing::error!("create_file DB insert failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok((StatusCode::CREATED, Json(row)))
}

async fn delete_file(
    State(state): State<AppState>,
    Extension(tenant_id): Extension<TenantId>,
    Path(uuid): Path<String>,
) -> Result<StatusCode, StatusCode> {
    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.0.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let now = chrono::Utc::now();
    let result = sqlx::query("UPDATE files SET deleted_at = $1 WHERE uuid = $2::uuid")
        .bind(now)
        .bind(&uuid)
        .execute(&mut *conn)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if result.rows_affected() == 0 {
        return Err(StatusCode::NOT_FOUND);
    }

    Ok(StatusCode::NO_CONTENT)
}

async fn restore_file(
    State(state): State<AppState>,
    Extension(tenant_id): Extension<TenantId>,
    Path(uuid): Path<String>,
) -> Result<StatusCode, StatusCode> {
    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.0.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let result = sqlx::query("UPDATE files SET deleted_at = NULL WHERE uuid = $1::uuid")
        .bind(&uuid)
        .execute(&mut *conn)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if result.rows_affected() == 0 {
        return Err(StatusCode::NOT_FOUND);
    }

    Ok(StatusCode::NO_CONTENT)
}
