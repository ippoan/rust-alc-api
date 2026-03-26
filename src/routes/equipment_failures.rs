use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use uuid::Uuid;

use crate::db::models::{
    CreateEquipmentFailure, EquipmentFailure, EquipmentFailureFilter, EquipmentFailuresResponse,
    UpdateEquipmentFailure,
};
use crate::db::tenant::set_current_tenant;
use crate::middleware::auth::TenantId;
use crate::AppState;

/// テナント対応ルート (JWT or X-Tenant-ID)
pub fn tenant_router() -> Router<AppState> {
    Router::new()
        .route(
            "/tenko/equipment-failures",
            post(create_failure).get(list_failures),
        )
        .route("/tenko/equipment-failures/csv", get(export_csv))
        .route(
            "/tenko/equipment-failures/{id}",
            get(get_failure).put(resolve_failure),
        )
}

/// 故障記録作成
async fn create_failure(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Json(body): Json<CreateEquipmentFailure>,
) -> Result<(StatusCode, Json<EquipmentFailure>), StatusCode> {
    let tenant_id = tenant.0 .0;

    let valid_types = [
        "face_recognition_error",
        "measurement_recording_failed",
        "kiosk_offline",
        "database_sync_error",
        "webhook_delivery_failed",
        "session_state_error",
        "photo_storage_error",
        "manual_report",
    ];
    if !valid_types.contains(&body.failure_type.as_str()) {
        return Err(StatusCode::BAD_REQUEST);
    }

    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let failure = sqlx::query_as::<_, EquipmentFailure>(
        r#"
        INSERT INTO equipment_failures (
            tenant_id, failure_type, description, affected_device,
            detected_at, detected_by, session_id
        )
        VALUES ($1, $2, $3, $4, COALESCE($5, NOW()), $6, $7)
        RETURNING *
        "#,
    )
    .bind(tenant_id)
    .bind(&body.failure_type)
    .bind(&body.description)
    .bind(&body.affected_device)
    .bind(body.detected_at)
    .bind(&body.detected_by)
    .bind(body.session_id)
    .fetch_one(&mut *conn)
    .await
    .map_err(|e| {
        tracing::error!("create_failure DB error: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok((StatusCode::CREATED, Json(failure)))
}

/// 故障一覧 (フィルタ+ページネーション)
async fn list_failures(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Query(filter): Query<EquipmentFailureFilter>,
) -> Result<Json<EquipmentFailuresResponse>, StatusCode> {
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

    let mut conditions = vec!["tenant_id = $1".to_string()];
    let mut param_idx = 2u32;

    if filter.failure_type.is_some() {
        conditions.push(format!("failure_type = ${param_idx}"));
        param_idx += 1;
    }
    if let Some(resolved) = filter.resolved {
        if resolved {
            conditions.push("resolved_at IS NOT NULL".to_string());
        } else {
            conditions.push("resolved_at IS NULL".to_string());
        }
    }
    if filter.session_id.is_some() {
        conditions.push(format!("session_id = ${param_idx}"));
        param_idx += 1;
    }
    if filter.date_from.is_some() {
        conditions.push(format!("detected_at >= ${param_idx}"));
        param_idx += 1;
    }
    if filter.date_to.is_some() {
        conditions.push(format!("detected_at <= ${param_idx}"));
        param_idx += 1;
    }

    let where_clause = conditions.join(" AND ");

    // Count
    let count_sql = format!("SELECT COUNT(*) FROM equipment_failures WHERE {where_clause}");
    let mut count_query = sqlx::query_scalar::<_, i64>(&count_sql).bind(tenant_id);
    if let Some(ref ft) = filter.failure_type {
        count_query = count_query.bind(ft);
    }
    if let Some(sid) = filter.session_id {
        count_query = count_query.bind(sid);
    }
    if let Some(df) = filter.date_from {
        count_query = count_query.bind(df);
    }
    if let Some(dt) = filter.date_to {
        count_query = count_query.bind(dt);
    }
    let total = count_query
        .fetch_one(&mut *conn)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // List
    let sql = format!(
        "SELECT * FROM equipment_failures WHERE {where_clause} ORDER BY detected_at DESC LIMIT ${param_idx} OFFSET ${}",
        param_idx + 1
    );
    let mut query = sqlx::query_as::<_, EquipmentFailure>(&sql).bind(tenant_id);
    if let Some(ref ft) = filter.failure_type {
        query = query.bind(ft);
    }
    if let Some(sid) = filter.session_id {
        query = query.bind(sid);
    }
    if let Some(df) = filter.date_from {
        query = query.bind(df);
    }
    if let Some(dt) = filter.date_to {
        query = query.bind(dt);
    }
    query = query.bind(per_page).bind(offset);

    let failures = query
        .fetch_all(&mut *conn)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(EquipmentFailuresResponse {
        failures,
        total,
        page,
        per_page,
    }))
}

/// 個別取得
async fn get_failure(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Path(id): Path<Uuid>,
) -> Result<Json<EquipmentFailure>, StatusCode> {
    let tenant_id = tenant.0 .0;

    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let failure = sqlx::query_as::<_, EquipmentFailure>(
        "SELECT * FROM equipment_failures WHERE id = $1 AND tenant_id = $2",
    )
    .bind(id)
    .bind(tenant_id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(failure))
}

/// 解決記録
async fn resolve_failure(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateEquipmentFailure>,
) -> Result<Json<EquipmentFailure>, StatusCode> {
    let tenant_id = tenant.0 .0;

    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let failure = sqlx::query_as::<_, EquipmentFailure>(
        r#"
        UPDATE equipment_failures SET
            resolved_at = NOW(),
            resolution_notes = $3,
            updated_at = NOW()
        WHERE id = $1 AND tenant_id = $2
        RETURNING *
        "#,
    )
    .bind(id)
    .bind(tenant_id)
    .bind(&body.resolution_notes)
    .fetch_optional(&mut *conn)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(failure))
}

/// CSV出力 (BOM付き)
async fn export_csv(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Query(filter): Query<EquipmentFailureFilter>,
) -> Result<impl IntoResponse, StatusCode> {
    let tenant_id = tenant.0 .0;

    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut conditions = vec!["tenant_id = $1".to_string()];
    let mut param_idx = 2u32;

    if filter.date_from.is_some() {
        conditions.push(format!("detected_at >= ${param_idx}"));
        param_idx += 1;
    }
    if filter.date_to.is_some() {
        conditions.push(format!("detected_at <= ${param_idx}"));
        param_idx += 1;
    }
    let _ = param_idx;

    let where_clause = conditions.join(" AND ");
    let sql =
        format!("SELECT * FROM equipment_failures WHERE {where_clause} ORDER BY detected_at DESC");
    let mut query = sqlx::query_as::<_, EquipmentFailure>(&sql).bind(tenant_id);
    if let Some(df) = filter.date_from {
        query = query.bind(df);
    }
    if let Some(dt) = filter.date_to {
        query = query.bind(dt);
    }

    let failures = query
        .fetch_all(&mut *conn)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut wtr = csv::Writer::from_writer(vec![]);

    wtr.write_record([
        "id",
        "failure_type",
        "description",
        "affected_device",
        "detected_at",
        "detected_by",
        "resolved_at",
        "resolution_notes",
        "session_id",
    ])
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    for f in &failures {
        wtr.write_record([
            f.id.to_string(),
            f.failure_type.clone(),
            f.description.clone(),
            f.affected_device.clone().unwrap_or_default(),
            f.detected_at.to_rfc3339(),
            f.detected_by.clone().unwrap_or_default(),
            f.resolved_at.map_or(String::new(), |t| t.to_rfc3339()),
            f.resolution_notes.clone().unwrap_or_default(),
            f.session_id.map_or(String::new(), |s| s.to_string()),
        ])
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }

    let csv_data = wtr
        .into_inner()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // BOM + CSV
    let mut output = vec![0xEF, 0xBB, 0xBF];
    output.extend_from_slice(&csv_data);

    Ok((
        [
            (axum::http::header::CONTENT_TYPE, "text/csv; charset=utf-8"),
            (
                axum::http::header::CONTENT_DISPOSITION,
                "attachment; filename=\"equipment_failures.csv\"",
            ),
        ],
        output,
    ))
}
