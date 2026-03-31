use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use uuid::Uuid;

use alc_core::auth_middleware::TenantId;
use alc_core::models::{
    CreateEquipmentFailure, EquipmentFailure, EquipmentFailureFilter, EquipmentFailuresResponse,
    UpdateEquipmentFailure,
};
use alc_core::AppState;

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

    let failure = state
        .equipment_failures
        .create(tenant_id, &body)
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
    let response = state
        .equipment_failures
        .list(tenant.0 .0, &filter)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(response))
}

/// 個別取得
async fn get_failure(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Path(id): Path<Uuid>,
) -> Result<Json<EquipmentFailure>, StatusCode> {
    let failure = state
        .equipment_failures
        .get(tenant.0 .0, id)
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
    let failure = state
        .equipment_failures
        .resolve(tenant.0 .0, id, &body)
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
    let failures = state
        .equipment_failures
        .list_for_csv(tenant.0 .0, &filter)
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
