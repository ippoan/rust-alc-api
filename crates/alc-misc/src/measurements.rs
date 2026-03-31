use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{header, StatusCode},
    response::Response,
    routing::{get, post},
    Json, Router,
};
use uuid::Uuid;

use alc_core::auth_middleware::TenantId;
use alc_core::models::{
    CreateMeasurement, Measurement, MeasurementFilter, MeasurementsResponse, StartMeasurement,
    UpdateMeasurement,
};
use alc_core::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/measurements",
            post(create_measurement).get(list_measurements),
        )
        .route("/measurements/start", post(start_measurement))
        .route(
            "/measurements/{id}",
            get(get_measurement).put(update_measurement),
        )
}

/// テナント対応ルート (顔写真プロキシ)
pub fn tenant_router() -> Router<AppState> {
    Router::new()
        .route("/measurements/{id}/face-photo", get(get_face_photo))
        .route("/measurements/{id}/video", get(get_video))
}

/// 測定開始 — employee_id のみで status='started' レコードを作成
async fn start_measurement(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Json(body): Json<StartMeasurement>,
) -> Result<(StatusCode, Json<Measurement>), StatusCode> {
    let tenant_id = tenant.0 .0;

    let measurement = state
        .measurements
        .start(tenant_id, &body)
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

    let measurement = state
        .measurements
        .update(tenant_id, id, &body)
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

    let measurement = state
        .measurements
        .create(tenant_id, &body)
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

    let result = state
        .measurements
        .list(tenant_id, &filter, page, per_page)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(MeasurementsResponse {
        measurements: result.measurements,
        total: result.total,
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

    let measurement = state
        .measurements
        .get(tenant_id, id)
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

    let measurement = state
        .measurements
        .get(tenant_id, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    let face_url = measurement
        .face_photo_url
        .as_deref()
        .ok_or(StatusCode::NOT_FOUND)?;

    let key = state.storage.extract_key(face_url).ok_or_else(|| {
        tracing::error!("Failed to extract key from face_photo_url: {face_url}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let data = state.storage.download(&key).await.map_err(|e| {
        tracing::error!("Failed to download face photo: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "image/jpeg")
        .header(header::CACHE_CONTROL, "private, max-age=3600")
        .body(Body::from(data))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// 録画プロキシ — ストレージから動画を取得して返却 (JWT 必須)
async fn get_video(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Path(id): Path<Uuid>,
) -> Result<Response, StatusCode> {
    let tenant_id = tenant.0 .0;

    let measurement = state
        .measurements
        .get(tenant_id, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    let video_url = measurement
        .video_url
        .as_deref()
        .ok_or(StatusCode::NOT_FOUND)?;

    let key = state.storage.extract_key(video_url).ok_or_else(|| {
        tracing::error!("Failed to extract key from video_url: {video_url}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let data = state.storage.download(&key).await.map_err(|e| {
        tracing::error!("Failed to download video: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "video/webm")
        .header(header::CACHE_CONTROL, "private, max-age=3600")
        .body(Body::from(data))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}
