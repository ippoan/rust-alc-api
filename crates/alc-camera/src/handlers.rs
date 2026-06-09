//! 監視カメラ HTTP layer (tenant スコープ、Refs #345)。
//!
//! `require_tenant_header` 配下にマウントする前提。device (拠点タブレット) は
//! X-Tenant-ID ヘッダーで health-log を書き込み、管理画面は JWT でマスタを CRUD する。

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post},
    Extension, Json, Router,
};
use serde::Deserialize;
use uuid::Uuid;

use alc_core::auth_middleware::TenantId;
use alc_core::repository::cameras::{
    Camera, CameraHealthLog, CameraStatusRow, CreateCamera, CreateCameraHealthLog, UpdateCamera,
};

use crate::CameraState;

pub fn tenant_router() -> Router<CameraState> {
    Router::new()
        .route("/cameras", get(list_cameras).post(create_camera))
        .route("/cameras/status", get(camera_statuses))
        .route(
            "/cameras/{id}",
            get(get_camera).patch(update_camera).delete(delete_camera),
        )
        .route(
            "/cameras/{id}/health-logs",
            post(create_health_log).get(list_health_logs),
        )
}

async fn list_cameras(
    State(state): State<CameraState>,
    tenant: Extension<TenantId>,
) -> Result<Json<Vec<Camera>>, StatusCode> {
    let tenant_id = tenant.0 .0;
    state
        .cameras
        .list(tenant_id)
        .await
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn create_camera(
    State(state): State<CameraState>,
    tenant: Extension<TenantId>,
    Json(body): Json<CreateCamera>,
) -> Result<(StatusCode, Json<Camera>), StatusCode> {
    let tenant_id = tenant.0 .0;
    state
        .cameras
        .create(tenant_id, &body)
        .await
        .map(|c| (StatusCode::CREATED, Json(c)))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn get_camera(
    State(state): State<CameraState>,
    tenant: Extension<TenantId>,
    Path(id): Path<Uuid>,
) -> Result<Json<Camera>, StatusCode> {
    let tenant_id = tenant.0 .0;
    match state.cameras.get(tenant_id, id).await {
        Ok(Some(c)) => Ok(Json(c)),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

async fn update_camera(
    State(state): State<CameraState>,
    tenant: Extension<TenantId>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateCamera>,
) -> Result<Json<Camera>, StatusCode> {
    let tenant_id = tenant.0 .0;
    match state.cameras.update(tenant_id, id, &body).await {
        Ok(Some(c)) => Ok(Json(c)),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

async fn delete_camera(
    State(state): State<CameraState>,
    tenant: Extension<TenantId>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let tenant_id = tenant.0 .0;
    match state.cameras.delete(tenant_id, id).await {
        Ok(true) => Ok(StatusCode::NO_CONTENT),
        Ok(false) => Err(StatusCode::NOT_FOUND),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

async fn camera_statuses(
    State(state): State<CameraState>,
    tenant: Extension<TenantId>,
) -> Result<Json<Vec<CameraStatusRow>>, StatusCode> {
    let tenant_id = tenant.0 .0;
    state
        .cameras
        .statuses(tenant_id)
        .await
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn create_health_log(
    State(state): State<CameraState>,
    tenant: Extension<TenantId>,
    Path(id): Path<Uuid>,
    Json(body): Json<CreateCameraHealthLog>,
) -> Result<(StatusCode, Json<CameraHealthLog>), StatusCode> {
    let tenant_id = tenant.0 .0;
    // カメラの存在 (= テナント所属) を確認してから記録する。
    match state.cameras.get(tenant_id, id).await {
        Ok(Some(_)) => {}
        Ok(None) => return Err(StatusCode::NOT_FOUND),
        Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
    crate::health::record_health_and_maybe_ticket(
        state.cameras.as_ref(),
        state.trouble_tickets.as_ref(),
        tenant_id,
        id,
        &body,
        state.down_threshold,
    )
    .await
    .map(|log| (StatusCode::CREATED, Json(log)))
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

#[derive(Debug, Deserialize)]
pub struct HealthLogQuery {
    pub limit: Option<i64>,
}

async fn list_health_logs(
    State(state): State<CameraState>,
    tenant: Extension<TenantId>,
    Path(id): Path<Uuid>,
    Query(q): Query<HealthLogQuery>,
) -> Result<Json<Vec<CameraHealthLog>>, StatusCode> {
    let tenant_id = tenant.0 .0;
    let limit = q.limit.unwrap_or(100).clamp(1, 1000);
    state
        .cameras
        .recent_health_logs(tenant_id, id, limit)
        .await
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}
