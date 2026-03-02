use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{header, StatusCode},
    response::Response,
    routing::{get, post},
    Json, Router,
};
use uuid::Uuid;

use crate::db::models::{CreateMeasurement, Measurement, MeasurementFilter, MeasurementsResponse, StartMeasurement, UpdateMeasurement};
use crate::db::tenant::set_current_tenant;
use crate::AppState;
use crate::middleware::auth::TenantId;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/measurements", post(create_measurement).get(list_measurements))
        .route("/measurements/start", post(start_measurement))
        .route("/measurements/{id}", get(get_measurement).put(update_measurement))
}

/// JWT 必須ルート用 (顔写真プロキシ)
pub fn jwt_router() -> Router<AppState> {
    Router::new()
        .route("/measurements/{id}/face-photo", get(get_face_photo))
}

/// 測定開始 — employee_id のみで status='started' レコードを作成
async fn start_measurement(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Json(body): Json<StartMeasurement>,
) -> Result<(StatusCode, Json<Measurement>), StatusCode> {
    let tenant_id = tenant.0 .0;

    let mut conn = state.pool.acquire().await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.to_string()).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let measurement = sqlx::query_as::<_, Measurement>(
        r#"
        INSERT INTO measurements (tenant_id, employee_id, status)
        VALUES ($1, $2, 'started')
        RETURNING *
        "#,
    )
    .bind(tenant_id)
    .bind(body.employee_id)
    .fetch_one(&mut *conn)
    .await
    .map_err(|e| {
        tracing::error!("start_measurement DB error: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok((StatusCode::CREATED, Json(measurement)))
}

/// 測定更新 — COALESCE で各フィールドを段階的に更新
async fn update_measurement(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Path(id): Path<Uuid>,
    raw_body: String,
) -> Result<Json<Measurement>, StatusCode> {
    let body: UpdateMeasurement = match serde_json::from_str(&raw_body) {
        Ok(b) => b,
        Err(e) => {
            tracing::error!("update_measurement deserialize error: {e}, body: {raw_body}");
            return Err(StatusCode::UNPROCESSABLE_ENTITY);
        }
    };
    let tenant_id = tenant.0 .0;

    if let Some(ref rt) = body.result_type {
        let valid = ["pass", "fail", "normal", "over", "error"];
        if !valid.contains(&rt.as_str()) {
            return Err(StatusCode::BAD_REQUEST);
        }
    }

    if let Some(ref status) = body.status {
        let valid = ["started", "completed"];
        if !valid.contains(&status.as_str()) {
            return Err(StatusCode::BAD_REQUEST);
        }
    }

    let mut conn = state.pool.acquire().await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.to_string()).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let measurement = sqlx::query_as::<_, Measurement>(
        r#"
        UPDATE measurements SET
            status = COALESCE($1, status),
            alcohol_level = COALESCE($2, alcohol_level),
            result = COALESCE($3, result),
            face_photo_url = COALESCE($4, face_photo_url),
            measured_at = COALESCE($5, measured_at),
            device_use_count = COALESCE($6, device_use_count),
            temperature = COALESCE($7, temperature),
            systolic = COALESCE($8, systolic),
            diastolic = COALESCE($9, diastolic),
            pulse = COALESCE($10, pulse),
            medical_measured_at = COALESCE($11, medical_measured_at),
            updated_at = NOW()
        WHERE id = $12 AND tenant_id = $13
        RETURNING *
        "#,
    )
    .bind(&body.status)
    .bind(body.alcohol_value)
    .bind(&body.result_type)
    .bind(&body.face_photo_url)
    .bind(body.measured_at)
    .bind(body.device_use_count)
    .bind(body.temperature)
    .bind(body.systolic)
    .bind(body.diastolic)
    .bind(body.pulse)
    .bind(body.medical_measured_at)
    .bind(id)
    .bind(tenant_id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(|e| {
        tracing::error!("update_measurement DB error: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?
    .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(measurement))
}

/// 測定作成 (完了済み) — オフラインキュー互換
async fn create_measurement(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    raw_body: String,
) -> Result<(StatusCode, Json<Measurement>), StatusCode> {
    let body: CreateMeasurement = match serde_json::from_str(&raw_body) {
        Ok(b) => b,
        Err(e) => {
            tracing::error!("create_measurement deserialize error: {e}, body: {raw_body}");
            return Err(StatusCode::UNPROCESSABLE_ENTITY);
        }
    };
    let tenant_id = tenant.0 .0;

    let valid_results = ["pass", "fail", "normal", "over", "error"];
    if !valid_results.contains(&body.result_type.as_str()) {
        return Err(StatusCode::BAD_REQUEST);
    }

    let mut conn = state.pool.acquire().await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.to_string()).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let measurement = sqlx::query_as::<_, Measurement>(
        r#"
        INSERT INTO measurements (
            tenant_id, employee_id, alcohol_level, result,
            face_photo_url, measured_at, device_use_count,
            temperature, systolic, diastolic, pulse, medical_measured_at,
            status
        )
        VALUES ($1, $2, $3, $4, $5, COALESCE($6, NOW()), COALESCE($7, 0),
                $8, $9, $10, $11, $12, 'completed')
        RETURNING *
        "#,
    )
    .bind(tenant_id)
    .bind(body.employee_id)
    .bind(body.alcohol_value)
    .bind(&body.result_type)
    .bind(&body.face_photo_url)
    .bind(body.measured_at)
    .bind(body.device_use_count)
    .bind(body.temperature)
    .bind(body.systolic)
    .bind(body.diastolic)
    .bind(body.pulse)
    .bind(body.medical_measured_at)
    .fetch_one(&mut *conn)
    .await
    .map_err(|e| {
        tracing::error!("create_measurement DB error: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok((StatusCode::CREATED, Json(measurement)))
}

async fn list_measurements(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Query(filter): Query<MeasurementFilter>,
) -> Result<Json<MeasurementsResponse>, StatusCode> {
    let tenant_id = tenant.0 .0;
    let per_page = filter.per_page.unwrap_or(50).min(100);
    let page = filter.page.unwrap_or(1).max(1);
    let offset = (page - 1) * per_page;

    let mut conn = state.pool.acquire().await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.to_string()).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Build dynamic query
    let mut conditions = vec!["m.tenant_id = $1".to_string()];
    let mut param_idx = 2u32;

    if filter.employee_id.is_some() {
        conditions.push(format!("m.employee_id = ${param_idx}"));
        param_idx += 1;
    }
    if filter.result_type.is_some() {
        conditions.push(format!("m.result = ${param_idx}"));
        param_idx += 1;
    }
    if filter.date_from.is_some() {
        conditions.push(format!("m.measured_at >= ${param_idx}"));
        param_idx += 1;
    }
    if filter.date_to.is_some() {
        conditions.push(format!("m.measured_at <= ${param_idx}"));
        param_idx += 1;
    }
    if filter.status.is_some() {
        conditions.push(format!("m.status = ${param_idx}"));
        param_idx += 1;
    }

    let where_clause = conditions.join(" AND ");

    // Count query
    let count_sql = format!("SELECT COUNT(*) FROM measurements m WHERE {where_clause}");
    let mut count_query = sqlx::query_scalar::<_, i64>(&count_sql).bind(tenant_id);
    if let Some(employee_id) = filter.employee_id {
        count_query = count_query.bind(employee_id);
    }
    if let Some(ref result_type) = filter.result_type {
        count_query = count_query.bind(result_type);
    }
    if let Some(date_from) = filter.date_from {
        count_query = count_query.bind(date_from);
    }
    if let Some(date_to) = filter.date_to {
        count_query = count_query.bind(date_to);
    }
    if let Some(ref status) = filter.status {
        count_query = count_query.bind(status);
    }
    let total = count_query
        .fetch_one(&mut *conn)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Data query
    let sql = format!(
        "SELECT m.* FROM measurements m WHERE {where_clause} ORDER BY m.measured_at DESC LIMIT ${param_idx} OFFSET ${}",
        param_idx + 1
    );

    let mut query = sqlx::query_as::<_, Measurement>(&sql).bind(tenant_id);

    if let Some(employee_id) = filter.employee_id {
        query = query.bind(employee_id);
    }
    if let Some(ref result_type) = filter.result_type {
        query = query.bind(result_type);
    }
    if let Some(date_from) = filter.date_from {
        query = query.bind(date_from);
    }
    if let Some(date_to) = filter.date_to {
        query = query.bind(date_to);
    }
    if let Some(ref status) = filter.status {
        query = query.bind(status);
    }

    query = query.bind(per_page).bind(offset);

    let measurements = query
        .fetch_all(&mut *conn)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(MeasurementsResponse {
        measurements,
        total,
        page,
        per_page,
    }))
}

async fn get_measurement(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Path(id): Path<Uuid>,
) -> Result<Json<Measurement>, StatusCode> {
    let tenant_id = tenant.0 .0;

    let mut conn = state.pool.acquire().await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.to_string()).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let measurement = sqlx::query_as::<_, Measurement>(
        "SELECT * FROM measurements WHERE id = $1 AND tenant_id = $2",
    )
    .bind(id)
    .bind(tenant_id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(measurement))
}

/// 顔写真プロキシ — ストレージから画像を取得して返却 (JWT 必須)
async fn get_face_photo(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Path(id): Path<Uuid>,
) -> Result<Response, StatusCode> {
    let tenant_id = tenant.0 .0;

    let mut conn = state.pool.acquire().await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.to_string()).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let measurement = sqlx::query_as::<_, Measurement>(
        "SELECT * FROM measurements WHERE id = $1 AND tenant_id = $2",
    )
    .bind(id)
    .bind(tenant_id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;

    let face_url = measurement.face_photo_url.as_deref().ok_or(StatusCode::NOT_FOUND)?;

    let key = state.storage.extract_key(face_url).ok_or_else(|| {
        tracing::error!("Failed to extract key from face_photo_url: {face_url}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    tracing::info!("face_photo proxy: url={face_url} key={key}");

    let data = state.storage.download(&key).await.map_err(|e| {
        tracing::error!("Failed to download face photo: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    tracing::info!("face_photo proxy: downloaded {} bytes", data.len());

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "image/jpeg")
        .header(header::CACHE_CONTROL, "private, max-age=3600")
        .body(Body::from(data))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}
