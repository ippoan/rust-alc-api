use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use uuid::Uuid;

use alc_core::auth_middleware::TenantId;
use alc_core::models::{CreateHealthBaseline, EmployeeHealthBaseline, UpdateHealthBaseline};
use alc_core::AppState;

/// テナント対応ルート (JWT or X-Tenant-ID)
pub fn tenant_router() -> Router<AppState> {
    Router::new()
        .route(
            "/tenko/health-baselines",
            post(upsert_baseline).get(list_baselines),
        )
        .route(
            "/tenko/health-baselines/{employee_id}",
            get(get_baseline)
                .put(update_baseline)
                .delete(delete_baseline),
        )
}

/// 基準値作成/更新 (UPSERT)
async fn upsert_baseline(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Json(body): Json<CreateHealthBaseline>,
) -> Result<(StatusCode, Json<EmployeeHealthBaseline>), StatusCode> {
    let baseline = state
        .health_baselines
        .upsert(tenant.0 .0, &body)
        .await
        .map_err(|e| {
            tracing::error!("upsert_baseline DB error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok((StatusCode::CREATED, Json(baseline)))
}

/// テナント内一覧
async fn list_baselines(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
) -> Result<Json<Vec<EmployeeHealthBaseline>>, StatusCode> {
    let baselines = state
        .health_baselines
        .list(tenant.0 .0)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(baselines))
}

/// 個別取得 (employee_id で検索)
async fn get_baseline(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Path(employee_id): Path<Uuid>,
) -> Result<Json<EmployeeHealthBaseline>, StatusCode> {
    let baseline = state
        .health_baselines
        .get(tenant.0 .0, employee_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(baseline))
}

/// 基準値更新
async fn update_baseline(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Path(employee_id): Path<Uuid>,
    Json(body): Json<UpdateHealthBaseline>,
) -> Result<Json<EmployeeHealthBaseline>, StatusCode> {
    let baseline = state
        .health_baselines
        .update(tenant.0 .0, employee_id, &body)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(baseline))
}

/// 基準値削除
async fn delete_baseline(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Path(employee_id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let deleted = state
        .health_baselines
        .delete(tenant.0 .0, employee_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if !deleted {
        return Err(StatusCode::NOT_FOUND);
    }

    Ok(StatusCode::NO_CONTENT)
}
