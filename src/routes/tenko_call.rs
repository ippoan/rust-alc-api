use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::AppState;

/// 公開ルート (認証不要) - Android アプリから呼ばれる
pub fn public_router() -> Router<AppState> {
    Router::new()
        .route("/tenko-call/register", post(register))
        .route("/tenko-call/tenko", post(tenko))
}

/// テナント認証付きルート - 管理画面から呼ばれる
pub fn tenant_router() -> Router<AppState> {
    Router::new()
        .route("/tenko-call/numbers", get(list_numbers).post(create_number))
        .route("/tenko-call/numbers/{id}", delete(delete_number))
        .route("/tenko-call/drivers", get(list_drivers))
}

// --- ドライバー登録 ---

#[derive(Debug, Deserialize)]
struct RegisterRequest {
    phone_number: String,
    driver_name: String,
    call_number: String,
}

#[derive(Debug, Serialize)]
struct RegisterResponse {
    success: bool,
    driver_id: i32,
    call_number: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

async fn register(
    State(state): State<AppState>,
    Json(body): Json<RegisterRequest>,
) -> Result<Json<RegisterResponse>, (StatusCode, Json<RegisterResponse>)> {
    let mut tx = state.pool.begin().await.map_err(|e| {
        tracing::error!("tenko_call register tx begin error: {e}");
        register_err("internal error")
    })?;

    // call_number がマスタに存在するか検証 (RLS 前にマスタ参照)
    let master = sqlx::query_as::<_, (String,)>(
        "SELECT tenant_id FROM tenko_call_numbers WHERE call_number = $1",
    )
    .bind(&body.call_number)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!("tenko_call register master lookup error: {e}");
        register_err("internal error")
    })?;

    let tenant_id = match master {
        Some(row) => row.0,
        None => {
            return Err((StatusCode::BAD_REQUEST, Json(RegisterResponse {
                success: false, driver_id: 0, call_number: None,
                error: Some("未登録の点呼用番号です".into()),
            })));
        }
    };

    // RLS 用にテナントをセット
    sqlx::query("SELECT set_current_tenant($1)")
        .bind(&tenant_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            tracing::error!("tenko_call register set_tenant error: {e}");
            register_err("internal error")
        })?;

    let row = sqlx::query_as::<_, (i32, Option<String>)>(
        r#"
        INSERT INTO tenko_call_drivers (phone_number, driver_name, call_number, tenant_id, updated_at)
        VALUES ($1, $2, $3, $4, now())
        ON CONFLICT (phone_number) DO UPDATE SET
            driver_name = $2, call_number = $3, tenant_id = $4, updated_at = now()
        RETURNING id, call_number
        "#,
    )
    .bind(&body.phone_number)
    .bind(&body.driver_name)
    .bind(&body.call_number)
    .bind(&tenant_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!("tenko_call register error: {e}");
        register_err("internal error")
    })?;

    tx.commit().await.map_err(|e| {
        tracing::error!("tenko_call register tx commit error: {e}");
        register_err("internal error")
    })?;

    Ok(Json(RegisterResponse {
        success: true,
        driver_id: row.0,
        call_number: row.1,
        error: None,
    }))
}

fn register_err(msg: &str) -> (StatusCode, Json<RegisterResponse>) {
    (StatusCode::INTERNAL_SERVER_ERROR, Json(RegisterResponse {
        success: false, driver_id: 0, call_number: None, error: Some(msg.into()),
    }))
}

// --- 点呼送信 ---

#[derive(Debug, Deserialize)]
struct TenkoRequest {
    phone_number: String,
    driver_name: String,
    latitude: f64,
    longitude: f64,
}

#[derive(Debug, Serialize)]
struct TenkoResponse {
    success: bool,
    call_number: Option<String>,
}

async fn tenko(
    State(state): State<AppState>,
    Json(body): Json<TenkoRequest>,
) -> Result<Json<TenkoResponse>, StatusCode> {
    let mut tx = state.pool.begin().await.map_err(|e| {
        tracing::error!("tenko_call tenko tx begin error: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // 登録済みドライバーを検索 (tenant_id も取得)
    let driver = sqlx::query_as::<_, (i32, Option<String>, String)>(
        "SELECT id, call_number, tenant_id FROM tenko_call_drivers WHERE phone_number = $1",
    )
    .bind(&body.phone_number)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!("tenko_call tenko driver lookup error: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?
    .ok_or(StatusCode::NOT_FOUND)?;

    // RLS 用にテナントをセット
    sqlx::query("SELECT set_current_tenant($1)")
        .bind(&driver.2)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            tracing::error!("tenko_call tenko set_tenant error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    // 位置情報ログを保存
    sqlx::query(
        r#"
        INSERT INTO tenko_call_logs (driver_id, phone_number, driver_name, latitude, longitude)
        VALUES ($1, $2, $3, $4, $5)
        "#,
    )
    .bind(driver.0)
    .bind(&body.phone_number)
    .bind(&body.driver_name)
    .bind(body.latitude)
    .bind(body.longitude)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!("tenko_call tenko log insert error: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    tx.commit().await.map_err(|e| {
        tracing::error!("tenko_call tenko tx commit error: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(TenkoResponse {
        success: true,
        call_number: driver.1,
    }))
}

// --- マスタ管理 (テナント認証付き) ---

#[derive(Debug, Serialize)]
struct TenkoCallNumber {
    id: i32,
    call_number: String,
    tenant_id: String,
    label: Option<String>,
    created_at: String,
}

async fn list_numbers(
    State(state): State<AppState>,
) -> Result<Json<Vec<TenkoCallNumber>>, StatusCode> {
    let rows = sqlx::query_as::<_, (i32, String, String, Option<String>, String)>(
        "SELECT id, call_number, tenant_id, label, created_at::text FROM tenko_call_numbers ORDER BY id",
    )
    .fetch_all(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("tenko_call list_numbers error: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(rows.into_iter().map(|r| TenkoCallNumber {
        id: r.0, call_number: r.1, tenant_id: r.2, label: r.3, created_at: r.4,
    }).collect()))
}

#[derive(Debug, Deserialize)]
struct CreateNumberRequest {
    call_number: String,
    tenant_id: Option<String>,
    label: Option<String>,
}

#[derive(Debug, Serialize)]
struct CreateNumberResponse {
    success: bool,
    id: i32,
}

async fn create_number(
    State(state): State<AppState>,
    Json(body): Json<CreateNumberRequest>,
) -> Result<Json<CreateNumberResponse>, StatusCode> {
    let tenant = body.tenant_id.unwrap_or_else(|| "default".into());
    let row = sqlx::query_as::<_, (i32,)>(
        "INSERT INTO tenko_call_numbers (call_number, tenant_id, label) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(&body.call_number)
    .bind(&tenant)
    .bind(&body.label)
    .fetch_one(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("tenko_call create_number error: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(CreateNumberResponse { success: true, id: row.0 }))
}

async fn delete_number(
    State(state): State<AppState>,
    Path(id): Path<i32>,
) -> Result<StatusCode, StatusCode> {
    sqlx::query("DELETE FROM tenko_call_numbers WHERE id = $1")
        .bind(id)
        .execute(&state.pool)
        .await
        .map_err(|e| {
            tracing::error!("tenko_call delete_number error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Serialize)]
struct TenkoCallDriver {
    id: i32,
    phone_number: String,
    driver_name: String,
    call_number: Option<String>,
    tenant_id: String,
    created_at: String,
}

async fn list_drivers(
    State(state): State<AppState>,
) -> Result<Json<Vec<TenkoCallDriver>>, StatusCode> {
    let rows = sqlx::query_as::<_, (i32, String, String, Option<String>, String, String)>(
        "SELECT id, phone_number, driver_name, call_number, tenant_id, created_at::text FROM tenko_call_drivers ORDER BY id",
    )
    .fetch_all(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("tenko_call list_drivers error: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(rows.into_iter().map(|r| TenkoCallDriver {
        id: r.0, phone_number: r.1, driver_name: r.2, call_number: r.3, tenant_id: r.4, created_at: r.5,
    }).collect()))
}
