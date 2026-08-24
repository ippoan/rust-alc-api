use axum::{
    extract::{Multipart, Path, Query, State},
    http::StatusCode,
    routing::{delete, get, post},
    Json, Router,
};
use uuid::Uuid;

use crate::models::{
    CreateTroubleTask, ReorderTroubleTasks, TroubleFile, TroubleTask, UpdateTroubleTask,
};
use crate::repository::trouble_tasks::{TroubleTasksFilter, TroubleTasksSortBy};
use crate::TroubleState;
use alc_core::auth_middleware::TenantId;

pub fn tenant_router<S>() -> Router<S>
where
    TroubleState: axum::extract::FromRef<S>,
    S: Clone + Send + Sync + 'static,
{
    Router::new()
        .route("/trouble/tasks", get(list_all_tasks))
        .route(
            "/trouble/tickets/{ticket_id}/tasks",
            post(create_task).get(list_tasks),
        )
        .route(
            "/trouble/tickets/{ticket_id}/tasks/reorder",
            axum::routing::put(reorder_tasks),
        )
        .route(
            "/trouble/tasks/{task_id}",
            axum::routing::put(update_task).delete(delete_task),
        )
        .route(
            "/trouble/tasks/{task_id}/files",
            post(upload_task_file).get(list_task_files),
        )
        .route(
            "/trouble/task-files/{file_id}/download",
            get(download_task_file),
        )
        .route("/trouble/task-files/{file_id}", delete(delete_task_file))
}

#[derive(serde::Deserialize, Default)]
struct ListTasksQuery {
    ticket_id: Option<Uuid>,
    status: Option<String>,
    task_type: Option<String>,
    assigned_to: Option<Uuid>,
    q: Option<String>,
    due_from: Option<chrono::DateTime<chrono::Utc>>,
    due_to: Option<chrono::DateTime<chrono::Utc>>,
    occurred_from: Option<chrono::DateTime<chrono::Utc>>,
    occurred_to: Option<chrono::DateTime<chrono::Utc>>,
    sort_by: Option<String>,
    sort_desc: Option<bool>,
    page: Option<i64>,
    per_page: Option<i64>,
}

#[derive(serde::Serialize)]
struct ListTasksResponse {
    items: Vec<TroubleTask>,
    total: i64,
    page: i64,
    per_page: i64,
}

fn parse_sort_by(s: Option<&str>) -> Result<TroubleTasksSortBy, StatusCode> {
    match s.unwrap_or("created_at") {
        "created_at" => Ok(TroubleTasksSortBy::CreatedAt),
        "occurred_at" => Ok(TroubleTasksSortBy::OccurredAt),
        "due_date" => Ok(TroubleTasksSortBy::DueDate),
        "next_action_due" => Ok(TroubleTasksSortBy::NextActionDue),
        "status" => Ok(TroubleTasksSortBy::Status),
        _ => Err(StatusCode::BAD_REQUEST),
    }
}

async fn list_all_tasks(
    State(state): State<TroubleState>,
    tenant: axum::Extension<TenantId>,
    Query(query): Query<ListTasksQuery>,
) -> Result<Json<ListTasksResponse>, StatusCode> {
    let sort_by = parse_sort_by(query.sort_by.as_deref())?;
    let sort_desc = query.sort_desc.unwrap_or(true);

    let per_page = query.per_page.unwrap_or(50).clamp(1, 200);
    let page = query.page.unwrap_or(1).max(1);
    let offset = (page - 1) * per_page;

    let filter = TroubleTasksFilter {
        ticket_id: query.ticket_id,
        status: query.status,
        task_type: query.task_type,
        assigned_to: query.assigned_to,
        q: query.q,
        due_from: query.due_from,
        due_to: query.due_to,
        occurred_from: query.occurred_from,
        occurred_to: query.occurred_to,
    };

    let tenant_id = tenant.0 .0;

    let items = state
        .trouble_tasks
        .list_all(tenant_id, &filter, sort_by, sort_desc, per_page, offset)
        .await
        .map_err(|e| {
            tracing::error!("list_all_tasks error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let total = state
        .trouble_tasks
        .count_all(tenant_id, &filter)
        .await
        .map_err(|e| {
            tracing::error!("count_all_tasks error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(ListTasksResponse {
        items,
        total,
        page,
        per_page,
    }))
}

async fn create_task(
    State(state): State<TroubleState>,
    tenant: axum::Extension<TenantId>,
    Path(ticket_id): Path<Uuid>,
    Json(body): Json<CreateTroubleTask>,
) -> Result<(StatusCode, Json<TroubleTask>), StatusCode> {
    if body.title.trim().is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let tenant_id = tenant.0 .0;

    let task = state
        .trouble_tasks
        .create(tenant_id, ticket_id, None, &body)
        .await
        .map_err(|e| {
            tracing::error!("create_task error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    // タスクアサイン通知
    if let Some(assigned_to_id) = body.assigned_to {
        if let Some(notifier) = &state.notifier {
            if let Ok(Some(pref)) = state
                .trouble_notification_prefs
                .find_enabled(tenant_id, "task_assigned", "lineworks")
                .await
            {
                let ticket_no = state
                    .trouble_tickets
                    .get(tenant_id, ticket_id)
                    .await
                    .ok()
                    .flatten()
                    .map(|t| t.ticket_no)
                    .unwrap_or(0);
                let emp_name = if let Some(ref employees) = state.employees {
                    employees
                        .get(tenant_id, assigned_to_id)
                        .await
                        .ok()
                        .flatten()
                        .map(|e| e.name)
                } else {
                    None
                };
                let msg = format!(
                    "タスクアサイン: #{} {} → {}",
                    ticket_no,
                    task.title,
                    emp_name.as_deref().unwrap_or("不明"),
                );
                notifier
                    .notify(tenant_id, "task_assigned", &msg, &pref.lineworks_user_ids)
                    .await;
            }
        }
    }

    Ok((StatusCode::CREATED, Json(task)))
}

async fn list_tasks(
    State(state): State<TroubleState>,
    tenant: axum::Extension<TenantId>,
    Path(ticket_id): Path<Uuid>,
) -> Result<Json<Vec<TroubleTask>>, StatusCode> {
    let tasks = state
        .trouble_tasks
        .list_by_ticket(tenant.0 .0, ticket_id)
        .await
        .map_err(|e| {
            tracing::error!("list_tasks error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(Json(tasks))
}

/// 経過記録の並び替え。`task_ids` に並べたい順で **そのチケットの全 task_id** を
/// 渡すと、サーバが 0 起点で `sort_order` を採番し直し、更新後の一覧を返す。
///
/// 隣接行の `sort_order` を交換する方式は、既存データが全行 0 のときに
/// 「0 と 0 の交換」= 無変化になって沈黙する。採番はサーバ側 1 文に寄せる
/// (Refs ippoan/nuxt-trouble#240)。
async fn reorder_tasks(
    State(state): State<TroubleState>,
    tenant: axum::Extension<TenantId>,
    Path(ticket_id): Path<Uuid>,
    Json(body): Json<ReorderTroubleTasks>,
) -> Result<Json<Vec<TroubleTask>>, StatusCode> {
    if body.task_ids.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }
    let unique: std::collections::HashSet<&Uuid> = body.task_ids.iter().collect();
    if unique.len() != body.task_ids.len() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let tasks = state
        .trouble_tasks
        .reorder(tenant.0 .0, ticket_id, &body.task_ids)
        .await
        .map_err(|e| {
            tracing::error!("reorder_tasks error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(tasks))
}

async fn update_task(
    State(state): State<TroubleState>,
    tenant: axum::Extension<TenantId>,
    Path(task_id): Path<Uuid>,
    Json(body): Json<UpdateTroubleTask>,
) -> Result<Json<TroubleTask>, StatusCode> {
    // status is now free text validated by the trouble_task_statuses master on the frontend.
    let tenant_id = tenant.0 .0;

    // assigned_to が提供され、かつ Some(uuid) の場合に通知対象
    let has_assigned_to = matches!(body.assigned_to, Some(Some(_)));

    let task = state
        .trouble_tasks
        .update(tenant_id, task_id, &body)
        .await
        .map_err(|e| {
            tracing::error!("update_task error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    // タスクアサイン通知
    if has_assigned_to {
        if let Some(assigned_to_id) = task.assigned_to {
            if let Some(notifier) = &state.notifier {
                if let Ok(Some(pref)) = state
                    .trouble_notification_prefs
                    .find_enabled(tenant_id, "task_assigned", "lineworks")
                    .await
                {
                    let ticket_no = state
                        .trouble_tickets
                        .get(tenant_id, task.ticket_id)
                        .await
                        .ok()
                        .flatten()
                        .map(|t| t.ticket_no)
                        .unwrap_or(0);
                    let emp_name = if let Some(ref employees) = state.employees {
                        employees
                            .get(tenant_id, assigned_to_id)
                            .await
                            .ok()
                            .flatten()
                            .map(|e| e.name)
                    } else {
                        None
                    };
                    let msg = format!(
                        "タスクアサイン: #{} {} → {}",
                        ticket_no,
                        task.title,
                        emp_name.as_deref().unwrap_or("不明"),
                    );
                    notifier
                        .notify(tenant_id, "task_assigned", &msg, &pref.lineworks_user_ids)
                        .await;
                }
            }
        }
    }

    Ok(Json(task))
}

async fn delete_task(
    State(state): State<TroubleState>,
    tenant: axum::Extension<TenantId>,
    Path(task_id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let deleted = state
        .trouble_tasks
        .delete(tenant.0 .0, task_id)
        .await
        .map_err(|e| {
            tracing::error!("delete_task error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

// --- Task File Handlers ---

async fn upload_task_file(
    State(state): State<TroubleState>,
    tenant: axum::Extension<TenantId>,
    Path(task_id): Path<Uuid>,
    mut multipart: Multipart,
) -> Result<(StatusCode, Json<TroubleFile>), StatusCode> {
    let tenant_id = tenant.0 .0;

    // タスクの存在確認 + ticket_id 取得
    let task = state
        .trouble_tasks
        .get(tenant_id, task_id)
        .await
        .map_err(|e| {
            tracing::error!("upload_task_file: get task error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    let storage = state
        .trouble_storage
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
    let storage_key = format!("{tenant_id}/trouble/tasks/{task_id}/{file_uuid}.{ext}");

    storage
        .upload(&storage_key, &data, &content_type)
        .await
        .map_err(|e| {
            tracing::error!("storage upload error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let file = state
        .trouble_files
        .create_for_task(
            tenant_id,
            task.ticket_id,
            task_id,
            &filename,
            &content_type,
            size_bytes,
            &storage_key,
        )
        .await
        .map_err(|e| {
            tracing::error!("create_task_file DB error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok((StatusCode::CREATED, Json(file)))
}

async fn list_task_files(
    State(state): State<TroubleState>,
    tenant: axum::Extension<TenantId>,
    Path(task_id): Path<Uuid>,
) -> Result<Json<Vec<TroubleFile>>, StatusCode> {
    let files = state
        .trouble_files
        .list_by_task(tenant.0 .0, task_id)
        .await
        .map_err(|e| {
            tracing::error!("list_task_files error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(Json(files))
}

async fn download_task_file(
    State(state): State<TroubleState>,
    tenant: axum::Extension<TenantId>,
    Path(file_id): Path<Uuid>,
) -> Result<impl axum::response::IntoResponse, StatusCode> {
    let file = state
        .trouble_files
        .get(tenant.0 .0, file_id)
        .await
        .map_err(|e| {
            tracing::error!("download_task_file error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    let storage = state
        .trouble_storage
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

async fn delete_task_file(
    State(state): State<TroubleState>,
    tenant: axum::Extension<TenantId>,
    Path(file_id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let deleted = state
        .trouble_files
        .soft_delete(tenant.0 .0, file_id)
        .await
        .map_err(|e| {
            tracing::error!("delete_task_file error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}
