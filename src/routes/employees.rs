use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post, put},
    Json, Router,
};
use uuid::Uuid;

use crate::db::models::{
    CreateEmployee, Employee, FaceDataEntry, UpdateEmployee, UpdateFace, UpdateLicense, UpdateNfcId,
};
use crate::middleware::auth::TenantId;
use crate::AppState;

/// テナント対応ルート (JWT or X-Tenant-ID)
pub fn tenant_router() -> Router<AppState> {
    Router::new()
        .route("/employees", post(create_employee).get(list_employees))
        .route(
            "/employees/{id}",
            get(get_employee)
                .put(update_employee)
                .delete(delete_employee),
        )
        .route("/employees/{id}/face", put(update_face))
        .route("/employees/{id}/nfc", put(update_nfc_id))
        .route("/employees/{id}/license", put(update_license))
        .route("/employees/face-data", get(list_face_data))
        .route("/employees/{id}/face/approve", put(approve_face))
        .route("/employees/{id}/face/reject", put(reject_face))
        .route("/employees/by-nfc/{nfc_id}", get(get_employee_by_nfc))
        .route("/employees/by-code/{code}", get(get_employee_by_code))
}

async fn create_employee(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Json(body): Json<CreateEmployee>,
) -> Result<(StatusCode, Json<Employee>), StatusCode> {
    let employee = state
        .employees
        .create(tenant.0 .0, &body)
        .await
        .map_err(|e| {
            tracing::error!("create_employee error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok((StatusCode::CREATED, Json(employee)))
}

async fn list_employees(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
) -> Result<Json<Vec<Employee>>, StatusCode> {
    let employees = state
        .employees
        .list(tenant.0 .0)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(employees))
}

async fn get_employee(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Path(id): Path<Uuid>,
) -> Result<Json<Employee>, StatusCode> {
    let employee = state
        .employees
        .get(tenant.0 .0, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(employee))
}

async fn get_employee_by_nfc(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Path(nfc_id): Path<String>,
) -> Result<Json<Employee>, StatusCode> {
    let employee = state
        .employees
        .get_by_nfc(tenant.0 .0, &nfc_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(employee))
}

async fn get_employee_by_code(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Path(code): Path<String>,
) -> Result<Json<Employee>, StatusCode> {
    let employee = state
        .employees
        .get_by_code(tenant.0 .0, &code)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(employee))
}

async fn update_employee(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateEmployee>,
) -> Result<Json<Employee>, StatusCode> {
    let employee = state
        .employees
        .update(tenant.0 .0, id, &body)
        .await
        .map_err(|e| {
            tracing::error!("update_employee error: {e}");
            if e.to_string().contains("idx_employees_code") {
                return StatusCode::CONFLICT;
            }
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(employee))
}

async fn delete_employee(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let deleted = state.employees.delete(tenant.0 .0, id).await.map_err(|e| {
        tracing::error!("delete_employee error: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    if !deleted {
        return Err(StatusCode::NOT_FOUND);
    }

    Ok(StatusCode::NO_CONTENT)
}

async fn update_face(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateFace>,
) -> Result<Json<Employee>, StatusCode> {
    // embedding 長の検証 (Human.js faceres モデルは 1024 次元)
    if let Some(ref emb) = body.face_embedding {
        if emb.len() != 1024 {
            return Err(StatusCode::BAD_REQUEST);
        }
    }

    let employee = state
        .employees
        .update_face(tenant.0 .0, id, &body)
        .await
        .map_err(|e| {
            tracing::error!("update_face error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(employee))
}

async fn list_face_data(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
) -> Result<Json<Vec<FaceDataEntry>>, StatusCode> {
    let rows = state
        .employees
        .list_face_data(tenant.0 .0)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(rows))
}

async fn update_license(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateLicense>,
) -> Result<Json<Employee>, StatusCode> {
    let employee = state
        .employees
        .update_license(
            tenant.0 .0,
            id,
            body.license_issue_date,
            body.license_expiry_date,
        )
        .await
        .map_err(|e| {
            tracing::error!("update_license error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(employee))
}

async fn update_nfc_id(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateNfcId>,
) -> Result<Json<Employee>, StatusCode> {
    let employee = state
        .employees
        .update_nfc_id(tenant.0 .0, id, &body.nfc_id)
        .await
        .map_err(|e| {
            tracing::error!("update_nfc_id error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(employee))
}

async fn approve_face(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Path(id): Path<Uuid>,
) -> Result<Json<Employee>, StatusCode> {
    let employee = state
        .employees
        .approve_face(tenant.0 .0, id)
        .await
        .map_err(|e| {
            tracing::error!("approve_face error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(employee))
}

async fn reject_face(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Path(id): Path<Uuid>,
) -> Result<Json<Employee>, StatusCode> {
    let employee = state
        .employees
        .reject_face(tenant.0 .0, id)
        .await
        .map_err(|e| {
            tracing::error!("reject_face error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(employee))
}
