use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::TenkoState;

/// 公開ルート (認証不要) - Android アプリから呼ばれる
pub fn public_router<S>() -> Router<S>
where
    TenkoState: axum::extract::FromRef<S>,
    S: Clone + Send + Sync + 'static,
{
    Router::new()
        .route("/tenko-call/register", post(register))
        .route("/tenko-call/tenko", post(tenko))
}

/// テナント認証付きルート - 管理画面から呼ばれる
pub fn tenant_router<S>() -> Router<S>
where
    TenkoState: axum::extract::FromRef<S>,
    S: Clone + Send + Sync + 'static,
{
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
    employee_code: Option<String>,
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
    State(state): State<TenkoState>,
    Json(body): Json<RegisterRequest>,
) -> Result<Json<RegisterResponse>, (StatusCode, Json<RegisterResponse>)> {
    let result = state
        .tenko_call
        .register_driver(
            &body.call_number,
            &body.phone_number,
            &body.driver_name,
            body.employee_code.as_deref(),
        )
        .await
        .map_err(|e| {
            tracing::error!("tenko_call register error: {e}");
            register_err("internal error")
        })?;

    match result {
        Some(r) => Ok(Json(RegisterResponse {
            success: true,
            driver_id: r.driver_id,
            call_number: r.call_number,
            error: None,
        })),
        None => Err((
            StatusCode::BAD_REQUEST,
            Json(RegisterResponse {
                success: false,
                driver_id: 0,
                call_number: None,
                error: Some("未登録の点呼用番号です".into()),
            }),
        )),
    }
}

fn register_err(msg: &str) -> (StatusCode, Json<RegisterResponse>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(RegisterResponse {
            success: false,
            driver_id: 0,
            call_number: None,
            error: Some(msg.into()),
        }),
    )
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
    State(state): State<TenkoState>,
    Json(body): Json<TenkoRequest>,
) -> Result<Json<TenkoResponse>, StatusCode> {
    let result = state
        .tenko_call
        .record_tenko(
            &body.phone_number,
            &body.driver_name,
            body.latitude,
            body.longitude,
        )
        .await
        .map_err(|e| {
            tracing::error!("tenko_call tenko error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    match result {
        Some(info) => Ok(Json(TenkoResponse {
            success: true,
            call_number: info.call_number,
        })),
        None => Err(StatusCode::NOT_FOUND),
    }
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
    State(state): State<TenkoState>,
) -> Result<Json<Vec<TenkoCallNumber>>, StatusCode> {
    let rows = state.tenko_call.list_numbers().await.map_err(|e| {
        tracing::error!("tenko_call list_numbers error: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(
        rows.into_iter()
            .map(|r| TenkoCallNumber {
                id: r.id,
                call_number: r.call_number,
                tenant_id: r.tenant_id,
                label: r.label,
                created_at: r.created_at,
            })
            .collect(),
    ))
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
    State(state): State<TenkoState>,
    Json(body): Json<CreateNumberRequest>,
) -> Result<Json<CreateNumberResponse>, StatusCode> {
    let tenant = body.tenant_id.unwrap_or_else(|| "default".into());
    let id = state
        .tenko_call
        .create_number(&body.call_number, &tenant, body.label.as_deref())
        .await
        .map_err(|e| {
            tracing::error!("tenko_call create_number error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(CreateNumberResponse { success: true, id }))
}

async fn delete_number(
    State(state): State<TenkoState>,
    Path(id): Path<i32>,
) -> Result<StatusCode, StatusCode> {
    state.tenko_call.delete_number(id).await.map_err(|e| {
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
    employee_code: Option<String>,
    created_at: String,
}

async fn list_drivers(
    State(state): State<TenkoState>,
) -> Result<Json<Vec<TenkoCallDriver>>, StatusCode> {
    let rows = state.tenko_call.list_drivers().await.map_err(|e| {
        tracing::error!("tenko_call list_drivers error: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(
        rows.into_iter()
            .map(|r| TenkoCallDriver {
                id: r.id,
                phone_number: r.phone_number,
                driver_name: r.driver_name,
                call_number: r.call_number,
                tenant_id: r.tenant_id,
                employee_code: r.employee_code,
                created_at: r.created_at,
            })
            .collect(),
    ))
}
