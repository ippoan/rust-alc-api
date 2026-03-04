use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post, put},
    Json, Router,
};
use uuid::Uuid;

use crate::db::models::{CreateEmployee, Employee, FaceDataEntry, UpdateEmployee, UpdateFace, UpdateLicense, UpdateNfcId};
use crate::db::tenant::set_current_tenant;
use crate::AppState;
use crate::middleware::auth::TenantId;

/// JWT 必須ルート (管理者)
pub fn jwt_router() -> Router<AppState> {
    Router::new()
        .route("/employees", post(create_employee).get(list_employees))
        .route("/employees/{id}", put(update_employee).delete(delete_employee))
        .route("/employees/{id}/face", put(update_face))
        .route("/employees/{id}/nfc", put(update_nfc_id))
        .route("/employees/{id}/license", put(update_license))
}

/// キオスク対応ルート (JWT or X-Tenant-ID)
pub fn tenant_router() -> Router<AppState> {
    Router::new()
        .route("/employees/face-data", get(list_face_data))
        .route("/employees/by-nfc/{nfc_id}", get(get_employee_by_nfc))
        .route("/employees/by-code/{code}", get(get_employee_by_code))
}

async fn create_employee(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Json(body): Json<CreateEmployee>,
) -> Result<(StatusCode, Json<Employee>), StatusCode> {
    let tenant_id = tenant.0 .0;

    let mut conn = state.pool.acquire().await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.to_string()).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let employee = sqlx::query_as::<_, Employee>(
        r#"
        INSERT INTO employees (tenant_id, code, nfc_id, name, role)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING *
        "#,
    )
    .bind(tenant_id)
    .bind(&body.code)
    .bind(&body.nfc_id)
    .bind(&body.name)
    .bind(&body.role)
    .fetch_one(&mut *conn)
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
    let tenant_id = tenant.0 .0;

    let mut conn = state.pool.acquire().await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.to_string()).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let employees = sqlx::query_as::<_, Employee>(
        "SELECT * FROM employees WHERE tenant_id = $1 AND deleted_at IS NULL ORDER BY name",
    )
    .bind(tenant_id)
    .fetch_all(&mut *conn)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(employees))
}

async fn get_employee_by_nfc(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Path(nfc_id): Path<String>,
) -> Result<Json<Employee>, StatusCode> {
    let tenant_id = tenant.0 .0;

    let mut conn = state.pool.acquire().await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.to_string()).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let employee = sqlx::query_as::<_, Employee>(
        "SELECT * FROM employees WHERE tenant_id = $1 AND nfc_id = $2 AND deleted_at IS NULL",
    )
    .bind(tenant_id)
    .bind(&nfc_id)
    .fetch_optional(&mut *conn)
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
    let tenant_id = tenant.0 .0;

    let mut conn = state.pool.acquire().await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.to_string()).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let employee = sqlx::query_as::<_, Employee>(
        "SELECT * FROM employees WHERE tenant_id = $1 AND code = $2 AND deleted_at IS NULL",
    )
    .bind(tenant_id)
    .bind(&code)
    .fetch_optional(&mut *conn)
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
    let tenant_id = tenant.0 .0;

    let mut conn = state.pool.acquire().await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.to_string()).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let employee = sqlx::query_as::<_, Employee>(
        r#"
        UPDATE employees SET name = $1, code = $2, role = COALESCE($5, role), updated_at = NOW()
        WHERE id = $3 AND tenant_id = $4 AND deleted_at IS NULL
        RETURNING *
        "#,
    )
    .bind(&body.name)
    .bind(&body.code)
    .bind(id)
    .bind(tenant_id)
    .bind(&body.role)
    .fetch_optional(&mut *conn)
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
    let tenant_id = tenant.0 .0;

    let mut conn = state.pool.acquire().await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.to_string()).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let result = sqlx::query(
        r#"
        UPDATE employees SET deleted_at = NOW(), updated_at = NOW()
        WHERE id = $1 AND tenant_id = $2 AND deleted_at IS NULL
        "#,
    )
    .bind(id)
    .bind(tenant_id)
    .execute(&mut *conn)
    .await
    .map_err(|e| {
        tracing::error!("delete_employee error: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    if result.rows_affected() == 0 {
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
    let tenant_id = tenant.0 .0;

    // embedding 長の検証 (Human.js faceres モデルは 1024 次元)
    if let Some(ref emb) = body.face_embedding {
        if emb.len() != 1024 {
            return Err(StatusCode::BAD_REQUEST);
        }
    }

    let mut conn = state.pool.acquire().await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.to_string()).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let employee = sqlx::query_as::<_, Employee>(
        r#"
        UPDATE employees SET
            face_photo_url = COALESCE($1, face_photo_url),
            face_embedding = COALESCE($2, face_embedding),
            face_embedding_at = CASE WHEN $2 IS NOT NULL THEN NOW() ELSE face_embedding_at END,
            updated_at = NOW()
        WHERE id = $3 AND tenant_id = $4 AND deleted_at IS NULL
        RETURNING *
        "#,
    )
    .bind(&body.face_photo_url)
    .bind(&body.face_embedding)
    .bind(id)
    .bind(tenant_id)
    .fetch_optional(&mut *conn)
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
    let tenant_id = tenant.0 .0;

    let mut conn = state.pool.acquire().await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.to_string()).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let rows = sqlx::query_as::<_, FaceDataEntry>(
        r#"
        SELECT id, face_embedding, face_embedding_at
        FROM employees
        WHERE tenant_id = $1 AND deleted_at IS NULL AND face_embedding IS NOT NULL
        "#,
    )
    .bind(tenant_id)
    .fetch_all(&mut *conn)
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
    let tenant_id = tenant.0 .0;

    let mut conn = state.pool.acquire().await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.to_string()).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let employee = sqlx::query_as::<_, Employee>(
        r#"
        UPDATE employees SET
            license_issue_date = COALESCE($1, license_issue_date),
            license_expiry_date = COALESCE($2, license_expiry_date),
            updated_at = NOW()
        WHERE id = $3 AND tenant_id = $4 AND deleted_at IS NULL
        RETURNING *
        "#,
    )
    .bind(body.license_issue_date)
    .bind(body.license_expiry_date)
    .bind(id)
    .bind(tenant_id)
    .fetch_optional(&mut *conn)
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
    let tenant_id = tenant.0 .0;

    let mut conn = state.pool.acquire().await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.to_string()).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let employee = sqlx::query_as::<_, Employee>(
        r#"
        UPDATE employees SET nfc_id = $1, updated_at = NOW()
        WHERE id = $2 AND tenant_id = $3
        RETURNING *
        "#,
    )
    .bind(&body.nfc_id)
    .bind(id)
    .bind(tenant_id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(|e| {
        tracing::error!("update_nfc_id error: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?
    .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(employee))
}
