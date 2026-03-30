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
use crate::db::repository::guidance_records::GuidanceRecordWithName;
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

    let repo = &*state.guidance_records;

    let total = repo
        .count_top_level(
            tenant_id,
            filter.employee_id,
            filter.guidance_type.as_deref(),
            filter.date_from.as_deref(),
            filter.date_to.as_deref(),
        )
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let all_records = repo
        .list_tree(
            tenant_id,
            filter.employee_id,
            filter.guidance_type.as_deref(),
            filter.date_from.as_deref(),
            filter.date_to.as_deref(),
            per_page,
            offset,
        )
        .await
        .map_err(|e| {
            tracing::error!("guidance_records list error: {:?}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    // 全レコードの添付ファイルを一括取得
    let record_ids: Vec<Uuid> = all_records.iter().map(|r| r.id).collect();
    let all_attachments = repo
        .list_attachments_by_record_ids(tenant_id, &record_ids)
        .await
        .unwrap_or_default();

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

    state
        .guidance_records
        .get(tenant_id, id)
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
    // 親がある場合は depth を計算
    let depth = if let Some(pid) = body.parent_id {
        let parent_depth = state
            .guidance_records
            .get_parent_depth(tenant_id, pid)
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

    let record = state
        .guidance_records
        .create(tenant_id, &body, depth)
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

    let record = state
        .guidance_records
        .update(tenant_id, id, &body)
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

    let rows = state
        .guidance_records
        .delete_recursive(tenant_id, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if rows == 0 {
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

    let atts = state
        .guidance_records
        .list_attachments(tenant_id, id)
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

    let att = state
        .guidance_records
        .create_attachment(
            tenant_id,
            record_id,
            &original_name,
            &content_type,
            file_size,
            &url,
        )
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

    let att = state
        .guidance_records
        .get_attachment(tenant_id, record_id, att_id)
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

    let rows = state
        .guidance_records
        .delete_attachment(tenant_id, record_id, att_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if rows == 0 {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(StatusCode::NO_CONTENT)
}
