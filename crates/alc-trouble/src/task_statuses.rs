use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get},
    Json, Router,
};
use uuid::Uuid;

use crate::models::{CreateTroubleTaskStatus, TroubleTaskStatus, UpdateTroubleTaskStatus};
use crate::TroubleState;
use alc_core::auth_middleware::TenantId;

/// Default seed used when a tenant has no task statuses yet
/// (mirrors migrations/098). Also used for GET fallback seeding.
const DEFAULT_SEED: &[(&str, &str, &str, i32, bool)] = &[
    ("open", "未着手", "#9CA3AF", 10, false),
    ("in_progress", "進行中", "#3B82F6", 20, false),
    ("waiting", "待機", "#F59E0B", 30, false),
    ("done", "完了", "#10B981", 40, true),
];

pub fn tenant_router<S>() -> Router<S>
where
    TroubleState: axum::extract::FromRef<S>,
    S: Clone + Send + Sync + 'static,
{
    Router::new()
        .route(
            "/trouble/task-statuses",
            get(list_task_statuses).post(create_task_status),
        )
        .route(
            "/trouble/task-statuses/{id}",
            delete(delete_task_status).put(update_task_status),
        )
}

async fn list_task_statuses(
    State(state): State<TroubleState>,
    tenant: axum::Extension<TenantId>,
) -> Result<Json<Vec<TroubleTaskStatus>>, StatusCode> {
    let tenant_id = tenant.0 .0;
    let statuses = state
        .trouble_task_statuses
        .list(tenant_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if !statuses.is_empty() {
        return Ok(Json(statuses));
    }

    // Auto-seed defaults for tenants that missed the migration seed
    let mut seeded = Vec::new();
    for (key, name, color, sort_order, is_done) in DEFAULT_SEED {
        let input = CreateTroubleTaskStatus {
            key: Some((*key).to_string()),
            name: (*name).to_string(),
            color: Some((*color).to_string()),
            sort_order: Some(*sort_order),
            is_done: Some(*is_done),
        };
        match state.trouble_task_statuses.create(tenant_id, &input).await {
            Ok(s) => seeded.push(s),
            Err(e) => {
                tracing::warn!("auto-seed task status {key}: {e}");
            }
        }
    }
    Ok(Json(seeded))
}

async fn create_task_status(
    State(state): State<TroubleState>,
    tenant: axum::Extension<TenantId>,
    Json(body): Json<CreateTroubleTaskStatus>,
) -> Result<(StatusCode, Json<TroubleTaskStatus>), StatusCode> {
    if body.name.trim().is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }
    let status = state
        .trouble_task_statuses
        .create(tenant.0 .0, &body)
        .await
        .map_err(|e| {
            if let sqlx::Error::Database(ref db_err) = e {
                if db_err.constraint().is_some() {
                    return StatusCode::CONFLICT;
                }
            }
            tracing::error!("create_task_status error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok((StatusCode::CREATED, Json(status)))
}

async fn update_task_status(
    State(state): State<TroubleState>,
    tenant: axum::Extension<TenantId>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateTroubleTaskStatus>,
) -> Result<Json<TroubleTaskStatus>, StatusCode> {
    let status = state
        .trouble_task_statuses
        .update(tenant.0 .0, id, &body)
        .await
        .map_err(|e| {
            if let sqlx::Error::Database(ref db_err) = e {
                if db_err.constraint().is_some() {
                    return StatusCode::CONFLICT;
                }
            }
            tracing::error!("update_task_status error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(status))
}

async fn delete_task_status(
    State(state): State<TroubleState>,
    tenant: axum::Extension<TenantId>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let deleted = state
        .trouble_task_statuses
        .delete(tenant.0 .0, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}
