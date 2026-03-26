use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use uuid::Uuid;

use crate::db::models::{
    BatchCreateTenkoSchedules, CreateTenkoSchedule, TenkoSchedule, TenkoScheduleFilter,
    TenkoSchedulesResponse, UpdateTenkoSchedule,
};
use crate::db::tenant::set_current_tenant;
use crate::middleware::auth::TenantId;
use crate::AppState;

/// テナント対応ルート (JWT or X-Tenant-ID)
pub fn tenant_router() -> Router<AppState> {
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
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Json(body): Json<CreateTenkoSchedule>,
) -> Result<(StatusCode, Json<TenkoSchedule>), StatusCode> {
    let tenant_id = tenant.0 .0;

    validate_schedule(&body)?;

    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let schedule = sqlx::query_as::<_, TenkoSchedule>(
        r#"
        INSERT INTO tenko_schedules (
            tenant_id, employee_id, tenko_type,
            responsible_manager_name, scheduled_at, instruction
        )
        VALUES ($1, $2, $3, $4, $5, $6)
        RETURNING *
        "#,
    )
    .bind(tenant_id)
    .bind(body.employee_id)
    .bind(&body.tenko_type)
    .bind(&body.responsible_manager_name)
    .bind(body.scheduled_at)
    .bind(&body.instruction)
    .fetch_one(&mut *conn)
    .await
    .map_err(|e| {
        tracing::error!("create_schedule error: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok((StatusCode::CREATED, Json(schedule)))
}

async fn batch_create_schedules(
    State(state): State<AppState>,
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

    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut results = Vec::with_capacity(body.schedules.len());
    for s in &body.schedules {
        let schedule = sqlx::query_as::<_, TenkoSchedule>(
            r#"
            INSERT INTO tenko_schedules (
                tenant_id, employee_id, tenko_type,
                responsible_manager_name, scheduled_at, instruction
            )
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING *
            "#,
        )
        .bind(tenant_id)
        .bind(s.employee_id)
        .bind(&s.tenko_type)
        .bind(&s.responsible_manager_name)
        .bind(s.scheduled_at)
        .bind(&s.instruction)
        .fetch_one(&mut *conn)
        .await
        .map_err(|e| {
            tracing::error!("batch_create_schedule error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
        results.push(schedule);
    }

    Ok((StatusCode::CREATED, Json(results)))
}

async fn list_schedules(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Query(filter): Query<TenkoScheduleFilter>,
) -> Result<Json<TenkoSchedulesResponse>, StatusCode> {
    let tenant_id = tenant.0 .0;
    let per_page = filter.per_page.unwrap_or(50).min(100);
    let page = filter.page.unwrap_or(1).max(1);
    let offset = (page - 1) * per_page;

    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut conditions = vec!["s.tenant_id = $1".to_string()];
    let mut param_idx = 2u32;

    if filter.employee_id.is_some() {
        conditions.push(format!("s.employee_id = ${param_idx}"));
        param_idx += 1;
    }
    if filter.tenko_type.is_some() {
        conditions.push(format!("s.tenko_type = ${param_idx}"));
        param_idx += 1;
    }
    if filter.consumed.is_some() {
        conditions.push(format!("s.consumed = ${param_idx}"));
        param_idx += 1;
    }
    if filter.date_from.is_some() {
        conditions.push(format!("s.scheduled_at >= ${param_idx}"));
        param_idx += 1;
    }
    if filter.date_to.is_some() {
        conditions.push(format!("s.scheduled_at <= ${param_idx}"));
        param_idx += 1;
    }

    let where_clause = conditions.join(" AND ");

    // Count
    let count_sql = format!("SELECT COUNT(*) FROM tenko_schedules s WHERE {where_clause}");
    let mut count_query = sqlx::query_scalar::<_, i64>(&count_sql).bind(tenant_id);
    if let Some(employee_id) = filter.employee_id {
        count_query = count_query.bind(employee_id);
    }
    if let Some(ref tenko_type) = filter.tenko_type {
        count_query = count_query.bind(tenko_type);
    }
    if let Some(consumed) = filter.consumed {
        count_query = count_query.bind(consumed);
    }
    if let Some(date_from) = filter.date_from {
        count_query = count_query.bind(date_from);
    }
    if let Some(date_to) = filter.date_to {
        count_query = count_query.bind(date_to);
    }
    let total = count_query
        .fetch_one(&mut *conn)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Data
    let sql = format!(
        "SELECT s.* FROM tenko_schedules s WHERE {where_clause} ORDER BY s.scheduled_at DESC LIMIT ${param_idx} OFFSET ${}",
        param_idx + 1
    );
    let mut query = sqlx::query_as::<_, TenkoSchedule>(&sql).bind(tenant_id);
    if let Some(employee_id) = filter.employee_id {
        query = query.bind(employee_id);
    }
    if let Some(ref tenko_type) = filter.tenko_type {
        query = query.bind(tenko_type);
    }
    if let Some(consumed) = filter.consumed {
        query = query.bind(consumed);
    }
    if let Some(date_from) = filter.date_from {
        query = query.bind(date_from);
    }
    if let Some(date_to) = filter.date_to {
        query = query.bind(date_to);
    }
    query = query.bind(per_page).bind(offset);

    let schedules = query
        .fetch_all(&mut *conn)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(TenkoSchedulesResponse {
        schedules,
        total,
        page,
        per_page,
    }))
}

async fn get_schedule(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Path(id): Path<Uuid>,
) -> Result<Json<TenkoSchedule>, StatusCode> {
    let tenant_id = tenant.0 .0;

    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let schedule = sqlx::query_as::<_, TenkoSchedule>(
        "SELECT * FROM tenko_schedules WHERE id = $1 AND tenant_id = $2",
    )
    .bind(id)
    .bind(tenant_id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(schedule))
}

async fn update_schedule(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateTenkoSchedule>,
) -> Result<Json<TenkoSchedule>, StatusCode> {
    let tenant_id = tenant.0 .0;

    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let schedule = sqlx::query_as::<_, TenkoSchedule>(
        r#"
        UPDATE tenko_schedules SET
            responsible_manager_name = COALESCE($1, responsible_manager_name),
            scheduled_at = COALESCE($2, scheduled_at),
            instruction = COALESCE($3, instruction),
            updated_at = NOW()
        WHERE id = $4 AND tenant_id = $5 AND consumed = FALSE
        RETURNING *
        "#,
    )
    .bind(&body.responsible_manager_name)
    .bind(body.scheduled_at)
    .bind(&body.instruction)
    .bind(id)
    .bind(tenant_id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(|e| {
        tracing::error!("update_schedule error: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?
    .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(schedule))
}

async fn delete_schedule(
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

    let result = sqlx::query(
        "DELETE FROM tenko_schedules WHERE id = $1 AND tenant_id = $2 AND consumed = FALSE",
    )
    .bind(id)
    .bind(tenant_id)
    .execute(&mut *conn)
    .await
    .map_err(|e| {
        tracing::error!("delete_schedule error: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    if result.rows_affected() == 0 {
        return Err(StatusCode::NOT_FOUND);
    }

    Ok(StatusCode::NO_CONTENT)
}

/// キオスク: 特定乗務員の未消費予定を取得
async fn get_pending_schedules(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Path(employee_id): Path<Uuid>,
) -> Result<Json<Vec<TenkoSchedule>>, StatusCode> {
    let tenant_id = tenant.0 .0;

    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let schedules = sqlx::query_as::<_, TenkoSchedule>(
        r#"
        SELECT * FROM tenko_schedules
        WHERE tenant_id = $1 AND employee_id = $2 AND consumed = FALSE
        ORDER BY scheduled_at ASC
        "#,
    )
    .bind(tenant_id)
    .bind(employee_id)
    .fetch_all(&mut *conn)
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
    if s.tenko_type == "pre_operation" && s.instruction.as_ref().map_or(true, |i| i.is_empty()) {
        return Err(StatusCode::BAD_REQUEST);
    }
    Ok(())
}
