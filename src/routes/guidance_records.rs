use axum::{
    body::Body,
    extract::{Multipart, Path, Query, State},
    http::StatusCode,
    response::Response,
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::db::models::{
    CreateGuidanceRecord, GuidanceRecord, GuidanceRecordAttachment, UpdateGuidanceRecord,
};
use crate::db::tenant::set_current_tenant;
use crate::middleware::auth::TenantId;
use crate::AppState;

pub fn tenant_router() -> Router<AppState> {
    Router::new()
        .route("/guidance-records", get(list_records).post(create_record))
        .route(
            "/guidance-records/{id}",
            get(get_record).put(update_record).delete(delete_record),
        )
        .route(
            "/guidance-records/{id}/attachments",
            get(list_attachments).post(upload_attachment),
        )
        .route(
            "/guidance-records/{id}/attachments/{att_id}",
            get(download_attachment).delete(delete_attachment),
        )
}

#[derive(Debug, Deserialize)]
struct GuidanceFilter {
    employee_id: Option<Uuid>,
    guidance_type: Option<String>,
    date_from: Option<String>,
    date_to: Option<String>,
    #[allow(dead_code)]
    parent_id: Option<String>, // "null" for top-level, UUID for children
    page: Option<i64>,
    per_page: Option<i64>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
struct GuidanceRecordWithName {
    id: Uuid,
    tenant_id: Uuid,
    employee_id: Uuid,
    employee_name: Option<String>,
    guidance_type: String,
    title: String,
    content: String,
    guided_by: Option<String>,
    guided_at: chrono::DateTime<chrono::Utc>,
    parent_id: Option<Uuid>,
    depth: i32,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize)]
struct GuidanceRecordTree {
    #[serde(flatten)]
    record: GuidanceRecordWithName,
    children: Vec<GuidanceRecordTree>,
    attachments: Vec<GuidanceRecordAttachment>,
}

#[derive(Debug, Serialize)]
struct GuidanceRecordsResponse {
    records: Vec<GuidanceRecordTree>,
    total: i64,
    page: i64,
    per_page: i64,
}

async fn list_records(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Query(filter): Query<GuidanceFilter>,
) -> Result<Json<GuidanceRecordsResponse>, StatusCode> {
    let tenant_id = tenant.0 .0;
    let page = filter.page.unwrap_or(1).max(1);
    let per_page = filter.per_page.unwrap_or(20).min(100);
    let offset = (page - 1) * per_page;

    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // トップレベルの件数
    let total: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM alc_api.guidance_records g
           WHERE g.parent_id IS NULL
             AND ($1::UUID IS NULL OR g.employee_id = $1)
             AND ($2::TEXT IS NULL OR g.guidance_type = $2)
             AND ($3::TEXT IS NULL OR g.guided_at >= $3::TIMESTAMPTZ)
             AND ($4::TEXT IS NULL OR g.guided_at < ($4::DATE + INTERVAL '1 day'))"#,
    )
    .bind(filter.employee_id)
    .bind(&filter.guidance_type)
    .bind(&filter.date_from)
    .bind(&filter.date_to)
    .fetch_one(&mut *conn)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // WITH RECURSIVE でツリー取得 (トップレベルをページネーション)
    let all_records = sqlx::query_as::<_, GuidanceRecordWithName>(
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
    .bind(filter.employee_id)
    .bind(&filter.guidance_type)
    .bind(&filter.date_from)
    .bind(&filter.date_to)
    .bind(per_page)
    .bind(offset)
    .fetch_all(&mut *conn)
    .await
    .map_err(|e| {
        tracing::error!("guidance_records list error: {:?}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // 全レコードの添付ファイルを一括取得
    let record_ids: Vec<Uuid> = all_records.iter().map(|r| r.id).collect();
    let all_attachments = if record_ids.is_empty() {
        vec![]
    } else {
        sqlx::query_as::<_, GuidanceRecordAttachment>(
            "SELECT * FROM alc_api.guidance_record_attachments WHERE record_id = ANY($1) ORDER BY created_at",
        )
        .bind(&record_ids)
        .fetch_all(&mut *conn)
        .await
        .unwrap_or_default()
    };

    // ツリー構築
    let trees = build_tree(&all_records, &all_attachments, None);

    Ok(Json(GuidanceRecordsResponse {
        records: trees,
        total,
        page,
        per_page,
    }))
}

fn build_tree(
    records: &[GuidanceRecordWithName],
    attachments: &[GuidanceRecordAttachment],
    parent_id: Option<Uuid>,
) -> Vec<GuidanceRecordTree> {
    records
        .iter()
        .filter(|r| r.parent_id == parent_id)
        .map(|r| {
            let children = build_tree(records, attachments, Some(r.id));
            let atts: Vec<_> = attachments
                .iter()
                .filter(|a| a.record_id == r.id)
                .cloned()
                .collect();
            GuidanceRecordTree {
                record: GuidanceRecordWithName {
                    id: r.id,
                    tenant_id: r.tenant_id,
                    employee_id: r.employee_id,
                    employee_name: r.employee_name.clone(),
                    guidance_type: r.guidance_type.clone(),
                    title: r.title.clone(),
                    content: r.content.clone(),
                    guided_by: r.guided_by.clone(),
                    guided_at: r.guided_at,
                    parent_id: r.parent_id,
                    depth: r.depth,
                    created_at: r.created_at,
                    updated_at: r.updated_at,
                },
                children,
                attachments: atts,
            }
        })
        .collect()
}

async fn get_record(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Path(id): Path<Uuid>,
) -> Result<Json<GuidanceRecord>, StatusCode> {
    let tenant_id = tenant.0 .0;
    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    sqlx::query_as::<_, GuidanceRecord>("SELECT * FROM alc_api.guidance_records WHERE id = $1")
        .bind(id)
        .fetch_optional(&mut *conn)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

async fn create_record(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Json(body): Json<CreateGuidanceRecord>,
) -> Result<(StatusCode, Json<GuidanceRecord>), StatusCode> {
    let tenant_id = tenant.0 .0;
    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // 親がある場合は depth を計算
    let depth = if let Some(pid) = body.parent_id {
        let parent_depth: Option<i32> =
            sqlx::query_scalar("SELECT depth FROM alc_api.guidance_records WHERE id = $1")
                .bind(pid)
                .fetch_optional(&mut *conn)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        match parent_depth {
            Some(d) if d >= 2 => {
                return Err(StatusCode::BAD_REQUEST); // 3階層制限
            }
            Some(d) => d + 1,
            None => return Err(StatusCode::NOT_FOUND), // 親が存在しない
        }
    } else {
        0
    };

    let record = sqlx::query_as::<_, GuidanceRecord>(
        r#"INSERT INTO alc_api.guidance_records
               (tenant_id, employee_id, guidance_type, title, content, guided_by, guided_at, parent_id, depth)
           VALUES ($1, $2, $3, $4, $5, $6, COALESCE($7, now()), $8, $9)
           RETURNING *"#,
    )
    .bind(tenant_id)
    .bind(body.employee_id)
    .bind(body.guidance_type.as_deref().unwrap_or("general"))
    .bind(&body.title)
    .bind(body.content.as_deref().unwrap_or(""))
    .bind(&body.guided_by)
    .bind(body.guided_at)
    .bind(body.parent_id)
    .bind(depth)
    .fetch_one(&mut *conn)
    .await
    .map_err(|e| {
        tracing::error!("guidance_records create error: {:?}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok((StatusCode::CREATED, Json(record)))
}

async fn update_record(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateGuidanceRecord>,
) -> Result<Json<GuidanceRecord>, StatusCode> {
    let tenant_id = tenant.0 .0;
    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let record = sqlx::query_as::<_, GuidanceRecord>(
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
    .bind(&body.guidance_type)
    .bind(&body.title)
    .bind(&body.content)
    .bind(&body.guided_by)
    .bind(body.guided_at)
    .bind(id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    match record {
        Some(r) => Ok(Json(r)),
        None => Err(StatusCode::NOT_FOUND),
    }
}

async fn delete_record(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let tenant_id = tenant.0 .0;
    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // 子レコードも含めて再帰削除
    let result = sqlx::query(
        r#"WITH RECURSIVE descendants AS (
            SELECT id FROM alc_api.guidance_records WHERE id = $1
            UNION ALL
            SELECT g.id FROM alc_api.guidance_records g JOIN descendants d ON g.parent_id = d.id
        )
        DELETE FROM alc_api.guidance_records WHERE id IN (SELECT id FROM descendants)"#,
    )
    .bind(id)
    .execute(&mut *conn)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if result.rows_affected() == 0 {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(StatusCode::NO_CONTENT)
}

// --- Attachments ---

async fn list_attachments(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<GuidanceRecordAttachment>>, StatusCode> {
    let tenant_id = tenant.0 .0;
    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let atts = sqlx::query_as::<_, GuidanceRecordAttachment>(
        "SELECT * FROM alc_api.guidance_record_attachments WHERE record_id = $1 ORDER BY created_at",
    )
    .bind(id)
    .fetch_all(&mut *conn)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(atts))
}

async fn upload_attachment(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Path(record_id): Path<Uuid>,
    mut multipart: Multipart,
) -> Result<(StatusCode, Json<GuidanceRecordAttachment>), StatusCode> {
    let tenant_id = tenant.0 .0;

    let field = multipart
        .next_field()
        .await
        .map_err(|_| StatusCode::BAD_REQUEST)?
        .ok_or(StatusCode::BAD_REQUEST)?;

    let original_name = field.file_name().unwrap_or("file").to_string();
    let content_type = field
        .content_type()
        .unwrap_or("application/octet-stream")
        .to_string();

    let data = field.bytes().await.map_err(|_| StatusCode::BAD_REQUEST)?;

    let file_size = data.len() as i32;

    // 拡張子を元ファイル名から取得
    let ext = original_name.rsplit('.').next().unwrap_or("bin");
    let storage_filename = format!("{}.{}", Uuid::new_v4(), ext);
    let object_path = format!("{}/guidance/{}/{}", tenant_id, record_id, storage_filename);

    let url = state
        .storage
        .upload(&object_path, &data, &content_type)
        .await
        .map_err(|e| {
            tracing::error!("Guidance attachment upload failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let att = sqlx::query_as::<_, GuidanceRecordAttachment>(
        r#"INSERT INTO alc_api.guidance_record_attachments (record_id, file_name, file_type, file_size, storage_url)
           VALUES ($1, $2, $3, $4, $5)
           RETURNING *"#,
    )
    .bind(record_id)
    .bind(&original_name)
    .bind(&content_type)
    .bind(file_size)
    .bind(&url)
    .fetch_one(&mut *conn)
    .await
    .map_err(|e| {
        tracing::error!("Guidance attachment DB insert failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok((StatusCode::CREATED, Json(att)))
}

async fn download_attachment(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Path((record_id, att_id)): Path<(Uuid, Uuid)>,
) -> Result<Response<Body>, StatusCode> {
    let tenant_id = tenant.0 .0;
    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let att = sqlx::query_as::<_, GuidanceRecordAttachment>(
        "SELECT * FROM alc_api.guidance_record_attachments WHERE id = $1 AND record_id = $2",
    )
    .bind(att_id)
    .bind(record_id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;

    let key = state.storage.extract_key(&att.storage_url).ok_or_else(|| {
        tracing::error!(
            "Failed to extract key from storage_url: {}",
            att.storage_url
        );
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let data = state.storage.download(&key).await.map_err(|e| {
        tracing::error!("Failed to download attachment: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", &att.file_type)
        .header(
            "Content-Disposition",
            format!("inline; filename=\"{}\"", att.file_name),
        )
        .body(Body::from(data))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn delete_attachment(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Path((record_id, att_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, StatusCode> {
    let tenant_id = tenant.0 .0;
    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let result = sqlx::query(
        "DELETE FROM alc_api.guidance_record_attachments WHERE id = $1 AND record_id = $2",
    )
    .bind(att_id)
    .bind(record_id)
    .execute(&mut *conn)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if result.rows_affected() == 0 {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(StatusCode::NO_CONTENT)
}
