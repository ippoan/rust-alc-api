use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post, put},
    Json, Router,
};
use uuid::Uuid;

use alc_core::auth_middleware::TenantId;
use alc_core::models::{
    CreateEmployee, Employee, EmployeeBulkUpsert, EmployeeUpsertItem, EmployeeUpsertSummary,
    FaceDataEntry, UpdateEmployee, UpdateFace, UpdateLicense, UpdateNfcId,
};
use alc_core::AppState;

/// `PUT /employees/bulk-by-code` で 1 リクエストに詰められる items の上限。
const MAX_BULK_UPSERT_ITEMS: usize = 500;

/// code / name の長さ上限、nfc_id は「交付日8桁+有効期限8桁」の固定 16 桁。
const MAX_CODE_LEN: usize = 64;
const MAX_NAME_LEN: usize = 200;

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
        .route(
            "/employees/{id}/license",
            put(update_license).delete(clear_license),
        )
        .route("/employees/face-data", get(list_face_data))
        .route("/employees/{id}/face/approve", put(approve_face))
        .route("/employees/{id}/face/reject", put(reject_face))
        .route("/employees/by-nfc/{nfc_id}", get(get_employee_by_nfc))
        .route("/employees/by-code/{code}", get(get_employee_by_code))
        .route("/employees/bulk-by-code", put(bulk_upsert_by_code))
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

/// 免許証の登録解除 (交付日・有効期限・nfc_id を NULL に戻す)。
/// テスト登録した免許証を消すための口 (Refs ippoan/alc-app#149)。
/// `update_license` は COALESCE なので null では消せない。
async fn clear_license(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Path(id): Path<Uuid>,
) -> Result<Json<Employee>, StatusCode> {
    let employee = state
        .employees
        .clear_license(tenant.0 .0, id)
        .await
        .map_err(|e| {
            tracing::error!("clear_license error: {e}");
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

/// nfc_id は運転免許証 IC の「交付日8桁+有効期限8桁」= 固定16桁の数字。
fn valid_bulk_nfc_id(nfc_id: &str) -> bool {
    nfc_id.len() == 16 && nfc_id.chars().all(|c| c.is_ascii_digit())
}

/// items 1 件の検証。不正なら失敗理由 (index 込み) を返す。
fn validate_bulk_item(idx: usize, item: &EmployeeUpsertItem) -> Result<(), String> {
    if item.code.is_empty() || item.code.chars().count() > MAX_CODE_LEN {
        return Err(format!("items[{idx}].code が不正です"));
    }
    if item.name.is_empty() || item.name.chars().count() > MAX_NAME_LEN {
        return Err(format!("items[{idx}].name が不正です"));
    }
    if let Some(nfc_id) = &item.nfc_id {
        if !valid_bulk_nfc_id(nfc_id) {
            return Err(format!("items[{idx}].nfc_id が不正です"));
        }
    }
    Ok(())
}

/// `PUT /employees/bulk-by-code` — 乗務員CD (code) キーの一括 upsert
/// (Refs ippoan/alc-app-s3#125)。theearth の乗務員マスタを relay 経由で
/// 1 日 5 回取り込む用途で、1 リクエストで最大 500 件をまとめて処理する。
async fn bulk_upsert_by_code(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Json(body): Json<EmployeeBulkUpsert>,
) -> Result<(StatusCode, Json<EmployeeUpsertSummary>), (StatusCode, String)> {
    if body.items.is_empty() || body.items.len() > MAX_BULK_UPSERT_ITEMS {
        return Err((StatusCode::BAD_REQUEST, "items が不正です".to_string()));
    }
    for (idx, item) in body.items.iter().enumerate() {
        validate_bulk_item(idx, item).map_err(|msg| (StatusCode::BAD_REQUEST, msg))?;
    }

    let summary = state
        .employees
        .upsert_by_code(tenant.0 .0, &body.items)
        .await
        .map_err(|e| {
            tracing::error!("upsert_by_code error: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal error".to_string(),
            )
        })?;

    Ok((StatusCode::OK, Json(summary)))
}
