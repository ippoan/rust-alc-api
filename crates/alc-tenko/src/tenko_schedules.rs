use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use uuid::Uuid;

use crate::TenkoState;
use alc_core::auth_middleware::TenantId;
use alc_core::models::{
    BatchCreateTenkoSchedules, CreateTenkoSchedule, TenkoSchedule, TenkoScheduleFilter,
    TenkoSchedulesResponse, UpdateTenkoSchedule,
};

/// テナント対応ルート (JWT or X-Tenant-ID)
pub fn tenant_router<S>() -> Router<S>
where
    TenkoState: axum::extract::FromRef<S>,
    S: Clone + Send + Sync + 'static,
{
    Router::new()
        .route(
            "/tenko/schedules",
            post(create_schedule).get(list_schedules),
        )
        .route("/tenko/schedules/batch", post(batch_create_schedules))
        .route(
            "/tenko/schedules/{id}",
            get(get_schedule)
                .put(update_schedule)
                .delete(delete_schedule),
        )
        .route(
            "/tenko/schedules/pending/{employee_id}",
            get(get_pending_schedules),
        )
}

async fn create_schedule(
    State(state): State<TenkoState>,
    tenant: axum::Extension<TenantId>,
    Json(body): Json<CreateTenkoSchedule>,
) -> Result<(StatusCode, Json<TenkoSchedule>), StatusCode> {
    let tenant_id = tenant.0 .0;

    validate_schedule(&body)?;

    let repo = &*state.tenko_schedules;
    let schedule = repo.create(tenant_id, &body).await.map_err(|e| {
        tracing::error!("create_schedule error: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok((StatusCode::CREATED, Json(schedule)))
}

async fn batch_create_schedules(
    State(state): State<TenkoState>,
    tenant: axum::Extension<TenantId>,
    Json(body): Json<BatchCreateTenkoSchedules>,
) -> Result<(StatusCode, Json<Vec<TenkoSchedule>>), StatusCode> {
    let tenant_id = tenant.0 .0;

    if body.schedules.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    for s in &body.schedules {
        validate_schedule(s)?;
    }

    let repo = &*state.tenko_schedules;
    let results = repo
        .batch_create(tenant_id, &body.schedules)
        .await
        .map_err(|e| {
            tracing::error!("batch_create_schedule error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok((StatusCode::CREATED, Json(results)))
}

async fn list_schedules(
    State(state): State<TenkoState>,
    tenant: axum::Extension<TenantId>,
    Query(filter): Query<TenkoScheduleFilter>,
) -> Result<Json<TenkoSchedulesResponse>, StatusCode> {
    let tenant_id = tenant.0 .0;
    let per_page = filter.per_page.unwrap_or(50).min(100);
    let page = filter.page.unwrap_or(1).max(1);

    let repo = &*state.tenko_schedules;
    let result = repo
        .list(tenant_id, &filter, page, per_page)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(TenkoSchedulesResponse {
        schedules: result.schedules,
        total: result.total,
        page,
        per_page,
    }))
}

async fn get_schedule(
    State(state): State<TenkoState>,
    tenant: axum::Extension<TenantId>,
    Path(id): Path<Uuid>,
) -> Result<Json<TenkoSchedule>, StatusCode> {
    let tenant_id = tenant.0 .0;

    let repo = &*state.tenko_schedules;
    let schedule = repo
        .get(tenant_id, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(schedule))
}

async fn update_schedule(
    State(state): State<TenkoState>,
    tenant: axum::Extension<TenantId>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateTenkoSchedule>,
) -> Result<Json<TenkoSchedule>, StatusCode> {
    let tenant_id = tenant.0 .0;

    let repo = &*state.tenko_schedules;
    let schedule = repo
        .update(tenant_id, id, &body)
        .await
        .map_err(|e| {
            tracing::error!("update_schedule error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(schedule))
}

async fn delete_schedule(
    State(state): State<TenkoState>,
    tenant: axum::Extension<TenantId>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let tenant_id = tenant.0 .0;

    let repo = &*state.tenko_schedules;
    let deleted = repo.delete(tenant_id, id).await.map_err(|e| {
        tracing::error!("delete_schedule error: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    if !deleted {
        return Err(StatusCode::NOT_FOUND);
    }

    Ok(StatusCode::NO_CONTENT)
}

/// キオスク: 特定乗務員の未消費予定を取得
async fn get_pending_schedules(
    State(state): State<TenkoState>,
    tenant: axum::Extension<TenantId>,
    Path(employee_id): Path<Uuid>,
) -> Result<Json<Vec<TenkoSchedule>>, StatusCode> {
    let tenant_id = tenant.0 .0;

    let repo = &*state.tenko_schedules;
    let schedules = repo
        .get_pending(tenant_id, employee_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(schedules))
}

fn validate_schedule(s: &CreateTenkoSchedule) -> Result<(), StatusCode> {
    let valid_types = ["pre_operation", "post_operation"];
    if !valid_types.contains(&s.tenko_type.as_str()) {
        return Err(StatusCode::BAD_REQUEST);
    }
    // 業務前は指示事項必須
    if s.tenko_type == "pre_operation" && s.instruction.as_ref().is_none_or(|i| i.is_empty()) {
        return Err(StatusCode::BAD_REQUEST);
    }
    Ok(())
}
