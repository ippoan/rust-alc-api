use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    routing::{delete, get, post, put},
    Extension, Json, Router,
};
use chrono::{Datelike, Timelike};
use rand::RngExt;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::db::tenant::set_current_tenant;
use crate::middleware::auth::{AuthUser, TenantId};
use crate::AppState;

/// 公開ルート (認証不要) - 端末側から呼ばれる
pub fn public_router() -> Router<AppState> {
    Router::new()
        .route(
            "/devices/register/request",
            post(create_registration_request),
        )
        .route(
            "/devices/register/status/{code}",
            get(check_registration_status),
        )
        .route("/devices/register/claim", post(claim_registration))
        .route("/devices/settings/{device_id}", get(get_device_settings))
        .route("/devices/register-fcm-token", put(register_fcm_token))
        .route("/devices/update-last-login", put(update_last_login))
        .route("/devices/fcm-notify-call", post(fcm_notify_call))
        .route("/devices/fcm-dismiss-test", post(fcm_dismiss_test))
        .route("/devices/test-fcm-all-exclude", post(test_fcm_all_exclude))
        .route("/devices/report-version", put(report_version))
        .route("/devices/report-watchdog", put(report_watchdog_state))
        .route("/devices/trigger-update-dev", post(trigger_update_dev))
}

/// テナント認証付きルート - 管理画面から呼ばれる
pub fn tenant_router() -> Router<AppState> {
    Router::new()
        .route("/devices", get(list_devices))
        .route("/devices/pending", get(list_pending))
        .route("/devices/register/create-token", post(create_url_token))
        .route(
            "/devices/register/create-permanent-qr",
            post(create_permanent_qr),
        )
        .route(
            "/devices/register/create-device-owner-token",
            post(create_device_owner_token),
        )
        .route("/devices/approve/{id}", post(approve_device))
        .route("/devices/approve-by-code/{code}", post(approve_by_code))
        .route("/devices/reject/{id}", post(reject_device))
        .route("/devices/disable/{id}", post(disable_device))
        .route("/devices/enable/{id}", post(enable_device))
        .route("/devices/{id}", delete(delete_device))
        .route("/devices/{id}/call-settings", put(update_call_settings))
        .route("/devices/{id}/test-fcm", post(test_fcm))
        .route("/devices/test-fcm-all", post(test_fcm_all))
        .route("/devices/trigger-update", post(trigger_update))
}

/// DB エラーをログ出力して 500 を返すヘルパー
fn db_err(context: &str, e: sqlx::Error) -> StatusCode {
    tracing::error!("{context}: {e}");
    StatusCode::INTERNAL_SERVER_ERROR
}

/// FCM_INTERNAL_SECRET ヘッダー認証 (設定されていなければスキップ)
fn check_internal_secret(headers: &HeaderMap) -> Result<(), StatusCode> {
    let Ok(expected) = std::env::var("FCM_INTERNAL_SECRET") else {
        return Ok(());
    };
    let provided = headers
        .get("X-Internal-Secret")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if provided != expected {
        return Err(StatusCode::UNAUTHORIZED);
    }
    Ok(())
}

// ============================================================
// 型定義
// ============================================================

#[derive(Debug, Serialize, sqlx::FromRow)]
struct Device {
    id: Uuid,
    tenant_id: Uuid,
    device_name: String,
    device_type: String,
    phone_number: Option<String>,
    user_id: Option<Uuid>,
    status: String,
    approved_by: Option<Uuid>,
    approved_at: Option<String>,
    last_seen_at: Option<String>,
    call_enabled: bool,
    call_schedule: Option<serde_json::Value>,
    fcm_token: Option<String>,
    last_login_employee_id: Option<Uuid>,
    last_login_employee_name: Option<String>,
    last_login_employee_role: Option<Vec<String>>,
    app_version_code: Option<i32>,
    app_version_name: Option<String>,
    is_device_owner: bool,
    is_dev_device: bool,
    always_on: bool,
    watchdog_running: Option<bool>,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
struct RegistrationRequest {
    id: Uuid,
    registration_code: String,
    flow_type: String,
    tenant_id: Option<Uuid>,
    phone_number: Option<String>,
    device_name: String,
    status: String,
    device_id: Option<Uuid>,
    expires_at: Option<String>,
    is_device_owner: bool,
    is_dev_device: bool,
    created_at: String,
}

// ============================================================
// 公開エンドポイント
// ============================================================

// --- QR一時: 端末が登録リクエスト生成 ---

#[derive(Debug, Deserialize)]
struct CreateRegistrationRequestBody {
    device_name: Option<String>,
}

#[derive(Debug, Serialize)]
struct CreateRegistrationResponse {
    registration_code: String,
    expires_at: String,
}

async fn create_registration_request(
    State(state): State<AppState>,
    Json(body): Json<CreateRegistrationRequestBody>,
) -> Result<Json<CreateRegistrationResponse>, StatusCode> {
    // 6桁コード生成 (衝突チェック付き)
    let code = generate_unique_code(&state).await?;
    let device_name = body.device_name.unwrap_or_default();

    let row = sqlx::query_as::<_, (String, String)>(
        r#"
        INSERT INTO device_registration_requests
            (registration_code, flow_type, device_name, status, expires_at)
        VALUES ($1, 'qr_temp', $2, 'pending', NOW() + INTERVAL '10 minutes')
        RETURNING registration_code, expires_at::text
        "#,
    )
    .bind(&code)
    .bind(&device_name)
    .fetch_one(&state.pool)
    .await
    .map_err(|e| db_err("create_registration_request", e))?;

    Ok(Json(CreateRegistrationResponse {
        registration_code: row.0,
        expires_at: row.1,
    }))
}

// --- QR一時/永久: ポーリング ---

#[derive(Debug, Serialize)]
struct RegistrationStatusResponse {
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    device_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tenant_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    device_name: Option<String>,
}

async fn check_registration_status(
    State(state): State<AppState>,
    Path(code): Path<String>,
) -> Result<Json<RegistrationStatusResponse>, StatusCode> {
    let row = sqlx::query_as::<
        _,
        (
            String,
            Option<Uuid>,
            Option<Uuid>,
            Option<String>,
            Option<String>,
        ),
    >(
        r#"
        SELECT status, device_id, tenant_id, expires_at::text,
               NULLIF(device_name, '') AS device_name
        FROM device_registration_requests
        WHERE registration_code = $1
        "#,
    )
    .bind(&code)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("check_registration_status error: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?
    .ok_or(StatusCode::NOT_FOUND)?;

    // 期限切れチェック (expires_at が設定されている場合)
    let status = if row.0 == "pending" {
        if let Some(ref expires_at) = row.3 {
            // 簡易チェック: DB側でも確認できるが、ここでも
            let expired = sqlx::query_as::<_, (bool,)>("SELECT $1::timestamptz < NOW()")
                .bind(expires_at)
                .fetch_one(&state.pool)
                .await
                .map(|r| r.0)
                .unwrap_or(false);
            if expired {
                "expired".to_string()
            } else {
                row.0.clone()
            }
        } else {
            row.0.clone()
        }
    } else {
        row.0.clone()
    };

    Ok(Json(RegistrationStatusResponse {
        status,
        device_id: row.1,
        tenant_id: row.2,
        device_name: row.4,
    }))
}

// --- URL / QR永久: 端末がクレーム ---

#[derive(Debug, Deserialize)]
struct ClaimRegistrationBody {
    registration_code: String,
    phone_number: Option<String>,
    device_name: Option<String>,
}

#[derive(Debug, Serialize)]
struct ClaimRegistrationResponse {
    success: bool,
    flow_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    device_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tenant_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}

async fn claim_registration(
    State(state): State<AppState>,
    Json(body): Json<ClaimRegistrationBody>,
) -> Result<Json<ClaimRegistrationResponse>, (StatusCode, Json<ClaimRegistrationResponse>)> {
    let claim_err = |msg: &str| {
        (
            StatusCode::BAD_REQUEST,
            Json(ClaimRegistrationResponse {
                success: false,
                flow_type: String::new(),
                device_id: None,
                tenant_id: None,
                message: Some(msg.into()),
            }),
        )
    };
    let claim_db = |ctx: &str, e: sqlx::Error| {
        tracing::error!("{ctx}: {e}");
        claim_err("internal error")
    };

    // リクエスト検索
    let req = sqlx::query_as::<_, (Uuid, String, Option<Uuid>, String, Option<String>, Option<String>, bool, bool)>(
        r#"
        SELECT id, flow_type, tenant_id, status, expires_at::text, device_name, is_device_owner, is_dev_device
        FROM device_registration_requests
        WHERE registration_code = $1
        "#,
    )
    .bind(&body.registration_code)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| claim_db("claim lookup", e))?
    .ok_or_else(|| claim_err("無効な登録コードです"))?;

    let (
        req_id,
        flow_type,
        tenant_id,
        status,
        _expires_at,
        stored_device_name,
        is_device_owner,
        is_dev_device,
    ) = req;

    if status != "pending" {
        return Err(claim_err("このコードは既に使用済みです"));
    }

    // 端末入力があればそちらを優先、なければ管理者が設定済みの名前をフォールバック
    let device_name = body
        .device_name
        .as_ref()
        .filter(|s| !s.is_empty())
        .cloned()
        .or(stored_device_name)
        .unwrap_or_default();

    match flow_type.as_str() {
        "url" | "device_owner" => {
            // URL / Device Owner フロー: 即承認
            let tenant_id = tenant_id.ok_or_else(|| claim_err("無効なトークンです"))?;
            let is_do = flow_type == "device_owner";

            let mut tx = state
                .pool
                .begin()
                .await
                .map_err(|e| claim_db("claim tx begin", e))?;
            set_current_tenant(&mut tx, &tenant_id.to_string())
                .await
                .map_err(|e| claim_db("claim set_tenant", e))?;

            // デバイス作成
            let device_id = sqlx::query_as::<_, (Uuid,)>(
                r#"
                INSERT INTO devices (tenant_id, device_name, device_type, phone_number, status, approved_at, is_device_owner, is_dev_device)
                VALUES ($1, $2, 'android', $3, 'active', NOW(), $4, $5)
                RETURNING id
                "#,
            )
            .bind(tenant_id)
            .bind(&device_name)
            .bind(&body.phone_number)
            .bind(is_do || is_device_owner)
            .bind(is_dev_device)
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| claim_db("claim create device", e))?
            .0;

            // リクエスト更新
            sqlx::query(
                r#"
                UPDATE device_registration_requests
                SET status = 'approved', device_id = $1, phone_number = $2, device_name = $3
                WHERE id = $4
                "#,
            )
            .bind(device_id)
            .bind(&body.phone_number)
            .bind(&device_name)
            .bind(req_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| claim_db("claim update request", e))?;

            tx.commit()
                .await
                .map_err(|e| claim_db("claim tx commit", e))?;

            Ok(Json(ClaimRegistrationResponse {
                success: true,
                flow_type,
                device_id: Some(device_id),
                tenant_id: Some(tenant_id),
                message: None,
            }))
        }
        "qr_permanent" => {
            // QR永久: pending のまま、管理者承認待ち
            sqlx::query(
                r#"
                UPDATE device_registration_requests
                SET phone_number = $1, device_name = $2
                WHERE id = $3
                "#,
            )
            .bind(&body.phone_number)
            .bind(&device_name)
            .bind(req_id)
            .execute(&state.pool)
            .await
            .map_err(|e| claim_db("claim update permanent qr", e))?;

            Ok(Json(ClaimRegistrationResponse {
                success: true,
                flow_type: "qr_permanent".into(),
                device_id: None,
                tenant_id: None,
                message: Some("管理者の承認待ちです".into()),
            }))
        }
        _ => Err(claim_err("無効なフロータイプです")),
    }
}

// ============================================================
// テナント認証付きエンドポイント
// ============================================================

// --- デバイス一覧 ---

async fn list_devices(
    State(state): State<AppState>,
    Extension(tenant): Extension<TenantId>,
) -> Result<Json<Vec<Device>>, StatusCode> {
    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant.0.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let rows = sqlx::query_as::<_, Device>(
        r#"
        SELECT id, tenant_id, device_name, device_type, phone_number, user_id, status,
               approved_by, approved_at::text, last_seen_at::text,
               call_enabled, call_schedule, fcm_token,
               last_login_employee_id, last_login_employee_name, last_login_employee_role,
               app_version_code, app_version_name, is_device_owner, is_dev_device,
               always_on, watchdog_running, created_at::text, updated_at::text
        FROM devices
        ORDER BY created_at DESC
        "#,
    )
    .fetch_all(&mut *conn)
    .await
    .map_err(|e| {
        tracing::error!("list_devices error: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(rows))
}

// --- 承認待ちリクエスト一覧 ---

async fn list_pending(
    State(state): State<AppState>,
    Extension(tenant): Extension<TenantId>,
) -> Result<Json<Vec<RegistrationRequest>>, StatusCode> {
    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant.0.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let rows = sqlx::query_as::<_, RegistrationRequest>(
        r#"
        SELECT id, registration_code, flow_type, tenant_id, phone_number, device_name,
               status, device_id, expires_at::text, is_device_owner, is_dev_device, created_at::text
        FROM device_registration_requests
        WHERE status = 'pending'
          AND (tenant_id = $1 OR tenant_id IS NULL)
          AND (expires_at IS NULL OR expires_at > NOW())
        ORDER BY created_at DESC
        "#,
    )
    .bind(tenant.0)
    .fetch_all(&mut *conn)
    .await
    .map_err(|e| {
        tracing::error!("list_pending error: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(rows))
}

// --- URL: 管理者がトークン生成 (即承認) ---

#[derive(Debug, Deserialize)]
struct CreateTokenBody {
    device_name: Option<String>,
    is_device_owner: Option<bool>,
    is_dev_device: Option<bool>,
}

#[derive(Debug, Serialize)]
struct CreateTokenResponse {
    registration_code: String,
    registration_url: String,
}

async fn create_url_token(
    State(state): State<AppState>,
    Extension(tenant): Extension<TenantId>,
    _auth: Option<Extension<AuthUser>>,
    Json(body): Json<CreateTokenBody>,
) -> Result<Json<CreateTokenResponse>, StatusCode> {
    let code = Uuid::new_v4().to_string();
    let device_name = body.device_name.unwrap_or_default();

    sqlx::query(
        r#"
        INSERT INTO device_registration_requests
            (registration_code, flow_type, tenant_id, device_name, status, expires_at, is_device_owner, is_dev_device)
        VALUES ($1, 'url', $2, $3, 'pending', NOW() + INTERVAL '24 hours', $4, $5)
        "#,
    )
    .bind(&code)
    .bind(tenant.0)
    .bind(&device_name)
    .bind(body.is_device_owner.unwrap_or(false))
    .bind(body.is_dev_device.unwrap_or(false))
    .execute(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("create_url_token error: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(CreateTokenResponse {
        registration_url: format!("/device-claim?token={}", code),
        registration_code: code,
    }))
}

// --- Device Owner: 管理者がプロビジョニング用コード生成 ---

#[derive(Debug, Deserialize)]
struct CreateDeviceOwnerTokenBody {
    device_name: Option<String>,
    is_dev_device: Option<bool>,
}

#[derive(Debug, Serialize)]
struct CreateDeviceOwnerTokenResponse {
    registration_code: String,
}

async fn create_device_owner_token(
    State(state): State<AppState>,
    Extension(tenant): Extension<TenantId>,
    _auth: Option<Extension<AuthUser>>,
    Json(body): Json<CreateDeviceOwnerTokenBody>,
) -> Result<Json<CreateDeviceOwnerTokenResponse>, StatusCode> {
    let code = Uuid::new_v4().to_string();
    let device_name = body.device_name.unwrap_or_default();

    sqlx::query(
        r#"
        INSERT INTO device_registration_requests
            (registration_code, flow_type, tenant_id, device_name, status, is_device_owner, is_dev_device)
        VALUES ($1, 'device_owner', $2, $3, 'pending', true, $4)
        "#,
    )
    .bind(&code)
    .bind(tenant.0)
    .bind(&device_name)
    .bind(body.is_dev_device.unwrap_or(false))
    .execute(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("create_device_owner_token error: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(CreateDeviceOwnerTokenResponse {
        registration_code: code,
    }))
}

// --- QR永久: 管理者がコード生成 ---

#[derive(Debug, Deserialize)]
struct CreatePermanentQrBody {
    device_name: Option<String>,
    is_device_owner: Option<bool>,
    is_dev_device: Option<bool>,
}

#[derive(Debug, Serialize)]
struct CreatePermanentQrResponse {
    registration_code: String,
}

async fn create_permanent_qr(
    State(state): State<AppState>,
    Extension(tenant): Extension<TenantId>,
    _auth: Option<Extension<AuthUser>>,
    Json(body): Json<CreatePermanentQrBody>,
) -> Result<Json<CreatePermanentQrResponse>, StatusCode> {
    let code = Uuid::new_v4().to_string();
    let device_name = body.device_name.unwrap_or_default();

    sqlx::query(
        r#"
        INSERT INTO device_registration_requests
            (registration_code, flow_type, tenant_id, device_name, status, is_device_owner, is_dev_device)
        VALUES ($1, 'qr_permanent', $2, $3, 'pending', $4, $5)
        "#,
    )
    .bind(&code)
    .bind(tenant.0)
    .bind(&device_name)
    .bind(body.is_device_owner.unwrap_or(false))
    .bind(body.is_dev_device.unwrap_or(false))
    .execute(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("create_permanent_qr error: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(CreatePermanentQrResponse {
        registration_code: code,
    }))
}

// --- 承認 ---

#[derive(Debug, Deserialize)]
struct ApproveBody {
    device_name: Option<String>,
}

#[derive(Debug, Serialize)]
struct ApproveResponse {
    success: bool,
    device_id: Uuid,
    tenant_id: Uuid,
}

async fn approve_device(
    State(state): State<AppState>,
    Extension(tenant): Extension<TenantId>,
    auth: Option<Extension<AuthUser>>,
    Path(id): Path<Uuid>,
    Json(body): Json<ApproveBody>,
) -> Result<Json<ApproveResponse>, StatusCode> {
    let mut tx = state
        .pool
        .begin()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut tx, &tenant.0.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // リクエスト取得
    let req = sqlx::query_as::<
        _,
        (
            Uuid,
            String,
            Option<String>,
            Option<String>,
            String,
            bool,
            bool,
        ),
    >(
        r#"
        SELECT id, flow_type, phone_number, device_name, status, is_device_owner, is_dev_device
        FROM device_registration_requests
        WHERE id = $1 AND status = 'pending'
        "#,
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!("approve lookup error: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?
    .ok_or(StatusCode::NOT_FOUND)?;

    let device_name = body.device_name.or(req.3.clone()).unwrap_or_default();

    let device_type = if req.2.is_some() { "android" } else { "kiosk" };
    let approved_by = auth.as_ref().map(|a| a.user_id);

    // デバイス作成
    let device_id = sqlx::query_as::<_, (Uuid,)>(
        r#"
        INSERT INTO devices (tenant_id, device_name, device_type, phone_number, status, approved_by, approved_at, is_device_owner, is_dev_device)
        VALUES ($1, $2, $3, $4, 'active', $5, NOW(), $6, $7)
        RETURNING id
        "#,
    )
    .bind(tenant.0)
    .bind(&device_name)
    .bind(device_type)
    .bind(&req.2)
    .bind(approved_by)
    .bind(req.5)
    .bind(req.6)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!("approve create device error: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?
    .0;

    // リクエスト更新 (QR一時の場合 tenant_id もセット)
    sqlx::query(
        r#"
        UPDATE device_registration_requests
        SET status = 'approved', device_id = $1, tenant_id = COALESCE(tenant_id, $2)
        WHERE id = $3
        "#,
    )
    .bind(device_id)
    .bind(tenant.0)
    .bind(id)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!("approve update request error: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    tx.commit()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(ApproveResponse {
        success: true,
        device_id,
        tenant_id: tenant.0,
    }))
}

// --- コードで承認 (QR一時フロー用: tenant_id が NULL のリクエストを管理者のテナントで承認) ---

async fn approve_by_code(
    State(state): State<AppState>,
    Extension(tenant): Extension<TenantId>,
    auth: Option<Extension<AuthUser>>,
    Path(code): Path<String>,
) -> Result<Json<ApproveResponse>, StatusCode> {
    let mut tx = state
        .pool
        .begin()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut tx, &tenant.0.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // registration_code でリクエスト検索
    let req = sqlx::query_as::<
        _,
        (
            Uuid,
            String,
            Option<String>,
            Option<String>,
            String,
            bool,
            bool,
        ),
    >(
        r#"
        SELECT id, flow_type, phone_number, device_name, status, is_device_owner, is_dev_device
        FROM device_registration_requests
        WHERE registration_code = $1 AND status = 'pending'
          AND (expires_at IS NULL OR expires_at > NOW())
        "#,
    )
    .bind(&code)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!("approve_by_code lookup error: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?
    .ok_or(StatusCode::NOT_FOUND)?;

    let device_name = req.3.clone().unwrap_or_default();
    let device_type = if req.2.is_some() { "android" } else { "kiosk" };
    let approved_by = auth.as_ref().map(|a| a.user_id);

    let device_id = sqlx::query_as::<_, (Uuid,)>(
        r#"
        INSERT INTO devices (tenant_id, device_name, device_type, phone_number, status, approved_by, approved_at, is_device_owner, is_dev_device)
        VALUES ($1, $2, $3, $4, 'active', $5, NOW(), $6, $7)
        RETURNING id
        "#,
    )
    .bind(tenant.0)
    .bind(&device_name)
    .bind(device_type)
    .bind(&req.2)
    .bind(approved_by)
    .bind(req.5)
    .bind(req.6)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!("approve_by_code create device error: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?
    .0;

    sqlx::query(
        r#"
        UPDATE device_registration_requests
        SET status = 'approved', device_id = $1, tenant_id = COALESCE(tenant_id, $2)
        WHERE id = $3
        "#,
    )
    .bind(device_id)
    .bind(tenant.0)
    .bind(req.0)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!("approve_by_code update request error: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    tx.commit()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(ApproveResponse {
        success: true,
        device_id,
        tenant_id: tenant.0,
    }))
}

// --- 拒否 ---

async fn reject_device(
    State(state): State<AppState>,
    Extension(tenant): Extension<TenantId>,
    _auth: Option<Extension<AuthUser>>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant.0.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let result = sqlx::query(
        r#"
        UPDATE device_registration_requests
        SET status = 'rejected'
        WHERE id = $1 AND status = 'pending'
        "#,
    )
    .bind(id)
    .execute(&mut *conn)
    .await
    .map_err(|e| {
        tracing::error!("reject_device error: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    if result.rows_affected() == 0 {
        return Err(StatusCode::NOT_FOUND);
    }

    Ok(StatusCode::NO_CONTENT)
}

// --- 無効化 ---

async fn disable_device(
    State(state): State<AppState>,
    Extension(tenant): Extension<TenantId>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant.0.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let result = sqlx::query(
        "UPDATE devices SET status = 'disabled', updated_at = NOW() WHERE id = $1 AND status = 'active'",
    )
    .bind(id)
    .execute(&mut *conn)
    .await
    .map_err(|e| {
        tracing::error!("disable_device error: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    if result.rows_affected() == 0 {
        return Err(StatusCode::NOT_FOUND);
    }

    Ok(StatusCode::NO_CONTENT)
}

// --- 有効化 ---

async fn enable_device(
    State(state): State<AppState>,
    Extension(tenant): Extension<TenantId>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant.0.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let result = sqlx::query(
        "UPDATE devices SET status = 'active', updated_at = NOW() WHERE id = $1 AND status = 'disabled'",
    )
    .bind(id)
    .execute(&mut *conn)
    .await
    .map_err(|e| {
        tracing::error!("enable_device error: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    if result.rows_affected() == 0 {
        return Err(StatusCode::NOT_FOUND);
    }

    Ok(StatusCode::NO_CONTENT)
}

// --- 削除 ---

async fn delete_device(
    State(state): State<AppState>,
    Extension(tenant): Extension<TenantId>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant.0.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let result = sqlx::query("DELETE FROM devices WHERE id = $1")
        .bind(id)
        .execute(&mut *conn)
        .await
        .map_err(|e| {
            tracing::error!("delete_device error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    if result.rows_affected() == 0 {
        return Err(StatusCode::NOT_FOUND);
    }

    Ok(StatusCode::NO_CONTENT)
}

// --- デバイス設定取得 (認証不要) ---

#[derive(Debug, Serialize, sqlx::FromRow)]
struct DeviceSettingsResponse {
    call_enabled: bool,
    call_schedule: Option<serde_json::Value>,
    status: String,
    last_login_employee_id: Option<Uuid>,
    last_login_employee_name: Option<String>,
    last_login_employee_role: Option<Vec<String>>,
    always_on: bool,
}

async fn get_device_settings(
    State(state): State<AppState>,
    Path(device_id): Path<Uuid>,
) -> Result<Json<DeviceSettingsResponse>, StatusCode> {
    let row = sqlx::query_as::<_, DeviceSettingsResponse>(
        "SELECT * FROM alc_api.get_device_settings_by_id($1)",
    )
    .bind(device_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("get_device_settings error: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?
    .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(row))
}

// --- 着信設定更新 (テナント認証) ---

#[derive(Debug, Deserialize)]
struct UpdateCallSettingsBody {
    call_enabled: bool,
    call_schedule: Option<serde_json::Value>,
    always_on: Option<bool>,
}

async fn update_call_settings(
    State(state): State<AppState>,
    Extension(tenant): Extension<TenantId>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateCallSettingsBody>,
) -> Result<StatusCode, StatusCode> {
    #[rustfmt::skip]
    tracing::info!("update_call_settings: device={id} call_enabled={} always_on={:?}", body.call_enabled, body.always_on);
    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant.0.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let result = sqlx::query(
        "UPDATE devices SET call_enabled = $1, call_schedule = $2, always_on = COALESCE($3, always_on), updated_at = NOW() WHERE id = $4",
    )
    .bind(body.call_enabled)
    .bind(&body.call_schedule)
    .bind(body.always_on)
    .bind(id)
    .execute(&mut *conn)
    .await
    .map_err(|e| {
        tracing::error!("update_call_settings error: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    if result.rows_affected() == 0 {
        return Err(StatusCode::NOT_FOUND);
    }

    // always_on が変更された場合、FCM で端末に通知して設定を再取得させる
    if let (true, Some(fcm)) = (body.always_on.is_some(), state.fcm.as_ref()) {
        // RLS を回避して fcm_token を取得 (テナントスコープ付き conn ではなく pool を使用)
        let token_row = sqlx::query_as::<_, (Option<String>,)>(
            "SELECT fcm_token FROM alc_api.devices WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&state.pool)
        .await
        .ok()
        .flatten();

        #[rustfmt::skip]
        tracing::info!("FCM settings_changed: device={id} token={:?}", token_row.as_ref().map(|r| r.0.is_some()));

        if let Some((Some(token),)) = token_row {
            let mut data = std::collections::HashMap::new();
            data.insert("type".to_string(), "settings_changed".to_string());
            data.insert(
                "timestamp".to_string(),
                chrono::Utc::now().timestamp_millis().to_string(),
            );
            if let Err(e) = fcm.send_data_message(&token, data).await {
                tracing::warn!("FCM settings_changed to device {} failed: {e}", id);
            }
        }
    }

    Ok(StatusCode::NO_CONTENT)
}

// --- 端末からのWatchdog状態報告 (認証不要) ---

#[derive(Debug, Deserialize)]
struct ReportWatchdogBody {
    device_id: Uuid,
    running: bool,
}

async fn report_watchdog_state(
    State(state): State<AppState>,
    Json(body): Json<ReportWatchdogBody>,
) -> Result<StatusCode, StatusCode> {
    let tenant_id = sqlx::query_as::<_, (Option<Uuid>,)>("SELECT alc_api.lookup_device_tenant($1)")
        .bind(body.device_id)
        .fetch_optional(&state.pool)
        .await
        .map_err(|e| {
            tracing::error!("report_watchdog_state lookup error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?
        .0
        .ok_or(StatusCode::NOT_FOUND)?;

    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    sqlx::query("UPDATE devices SET watchdog_running = $1, updated_at = NOW() WHERE id = $2")
        .bind(body.running)
        .bind(body.device_id)
        .execute(&mut *conn)
        .await
        .map_err(|e| {
            tracing::error!("report_watchdog_state error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(StatusCode::NO_CONTENT)
}

// ============================================================
// FCM エンドポイント
// ============================================================

// --- FCMトークン登録 (認証不要、端末から呼ばれる) ---

#[derive(Debug, Deserialize)]
struct RegisterFcmTokenBody {
    device_id: Uuid,
    fcm_token: String,
}

async fn register_fcm_token(
    State(state): State<AppState>,
    Json(body): Json<RegisterFcmTokenBody>,
) -> Result<StatusCode, StatusCode> {
    let tenant_id = sqlx::query_as::<_, (Option<Uuid>,)>("SELECT alc_api.lookup_device_tenant($1)")
        .bind(body.device_id)
        .fetch_optional(&state.pool)
        .await
        .map_err(|e| {
            tracing::error!("register_fcm_token lookup error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?
        .0
        .ok_or(StatusCode::NOT_FOUND)?;

    // set_current_tenant で RLS UPDATE を通す
    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    sqlx::query("UPDATE devices SET fcm_token = $1, updated_at = NOW() WHERE id = $2")
        .bind(&body.fcm_token)
        .bind(body.device_id)
        .execute(&mut *conn)
        .await
        .map_err(|e| {
            tracing::error!("register_fcm_token update error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    tracing::info!("FCM token registered for device {}", body.device_id);
    Ok(StatusCode::NO_CONTENT)
}

// --- 最終ログインユーザー更新 (認証不要、端末から呼ばれる) ---

#[derive(Debug, Deserialize)]
struct UpdateLastLoginBody {
    device_id: Uuid,
    employee_id: Uuid,
    employee_name: String,
    employee_role: Vec<String>,
}

async fn update_last_login(
    State(state): State<AppState>,
    Json(body): Json<UpdateLastLoginBody>,
) -> Result<StatusCode, StatusCode> {
    let tenant_id = sqlx::query_as::<_, (Option<Uuid>,)>("SELECT alc_api.lookup_device_tenant($1)")
        .bind(body.device_id)
        .fetch_optional(&state.pool)
        .await
        .map_err(|e| {
            tracing::error!("update_last_login lookup error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?
        .0
        .ok_or(StatusCode::NOT_FOUND)?;

    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    sqlx::query(
        "UPDATE devices SET last_login_employee_id = $1, last_login_employee_name = $2, last_login_employee_role = $3, updated_at = NOW() WHERE id = $4",
    )
    .bind(body.employee_id)
    .bind(&body.employee_name)
    .bind(&body.employee_role)
    .bind(body.device_id)
    .execute(&mut *conn)
    .await
    .map_err(|e| {
        tracing::error!("update_last_login error: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    #[rustfmt::skip]
    tracing::info!("Last login updated for device {}: {} ({})", body.device_id, body.employee_name, body.employee_role.join(","));
    Ok(StatusCode::NO_CONTENT)
}

// --- FCM着信通知 (シグナリングサーバーから呼ばれる内部API) ---

#[derive(Debug, Deserialize)]
struct FcmNotifyCallBody {
    room_ids: Vec<String>,
    #[serde(default)]
    exclude_device_ids: Vec<String>,
}

#[derive(Debug, Serialize)]
struct FcmNotifyCallResponse {
    sent: usize,
    skipped: usize,
    errors: usize,
}

#[derive(Debug, sqlx::FromRow)]
struct FcmDevice {
    id: Uuid,
    fcm_token: String,
    call_enabled: bool,
    call_schedule: Option<serde_json::Value>,
}

async fn fcm_notify_call(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<FcmNotifyCallBody>,
) -> Result<Json<FcmNotifyCallResponse>, StatusCode> {
    // 簡易認証: X-Internal-Secret ヘッダー
    check_internal_secret(&headers)?;

    let fcm = state.fcm.as_ref().ok_or_else(|| {
        tracing::warn!("FCM notify called but FCM is not configured");
        StatusCode::SERVICE_UNAVAILABLE
    })?;

    if body.room_ids.is_empty() {
        return Ok(Json(FcmNotifyCallResponse {
            sent: 0,
            skipped: 0,
            errors: 0,
        }));
    }

    // アクティブかつ FCM トークンありのデバイスを取得
    let devices = sqlx::query_as::<_, FcmDevice>(
        "SELECT id, fcm_token, call_enabled, call_schedule FROM devices WHERE fcm_token IS NOT NULL AND status = 'active'",
    )
    .fetch_all(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("fcm_notify_call query error: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let exclude_set: std::collections::HashSet<&str> =
        body.exclude_device_ids.iter().map(|s| s.as_str()).collect();

    let room_ids_json = serde_json::to_string(&body.room_ids).unwrap_or_default();
    let timestamp = chrono::Utc::now().timestamp_millis().to_string();

    let mut sent = 0usize;
    let mut skipped = 0usize;
    let mut errors = 0usize;

    for device in &devices {
        let device_id_str = device.id.to_string();

        // WebSocket で既に配信済みのデバイスはスキップ
        if exclude_set.contains(device_id_str.as_str()) {
            skipped += 1;
            continue;
        }

        // スケジュールチェック
        if !should_notify_device(device) {
            skipped += 1;
            continue;
        }

        let mut data = std::collections::HashMap::new();
        data.insert("type".to_string(), "incoming_call".to_string());
        data.insert("room_ids".to_string(), room_ids_json.clone());
        data.insert("timestamp".to_string(), timestamp.clone());

        match fcm.send_data_message(&device.fcm_token, data).await {
            Ok(()) => {
                tracing::info!("FCM sent to device {}", device.id);
                sent += 1;
            }
            Err(e) => {
                tracing::error!("FCM send to device {} failed: {e}", device.id);
                errors += 1;
            }
        }
    }

    tracing::info!("FCM notify: sent={sent}, skipped={skipped}, errors={errors}");
    Ok(Json(FcmNotifyCallResponse {
        sent,
        skipped,
        errors,
    }))
}

/// スケジュールチェック (room-registry.ts の shouldNotify() と同等)
fn should_notify_device(device: &FcmDevice) -> bool {
    if !device.call_enabled {
        return false;
    }

    let schedule = match &device.call_schedule {
        Some(v) => v,
        None => return true, // スケジュール未設定 → 通知する
    };

    let enabled = schedule
        .get("enabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    if !enabled {
        return false;
    }

    // JST = UTC+9
    let now = chrono::Utc::now();
    let jst_offset = chrono::FixedOffset::east_opt(9 * 3600).unwrap();
    let jst_now = now.with_timezone(&jst_offset);

    let jst_day = jst_now.weekday().num_days_from_sunday() as i64; // 0=日, 1=月, ..., 6=土

    // 曜日チェック
    if schedule
        .get("days")
        .and_then(|v| v.as_array())
        .is_some_and(|days| {
            let ns: Vec<i64> = days.iter().filter_map(|d| d.as_i64()).collect();
            !ns.is_empty() && !ns.contains(&jst_day)
        })
    {
        return false;
    }

    // 時間範囲チェック
    let start_hour = schedule
        .get("startHour")
        .and_then(|v| v.as_i64())
        .unwrap_or(0) as u32;
    let start_min = schedule
        .get("startMin")
        .and_then(|v| v.as_i64())
        .unwrap_or(0) as u32;
    let end_hour = schedule
        .get("endHour")
        .and_then(|v| v.as_i64())
        .unwrap_or(24) as u32;
    let end_min = schedule.get("endMin").and_then(|v| v.as_i64()).unwrap_or(0) as u32;

    let current = jst_now.hour() * 60 + jst_now.minute();
    let start = start_hour * 60 + start_min;
    let end = end_hour * 60 + end_min;

    if start <= end {
        current >= start && current < end
    } else {
        current >= start || current < end
    }
}

// --- FCM テスト dismiss (認証不要 — 端末から呼ばれる) ---

#[derive(Debug, Deserialize)]
struct FcmDismissTestRequest {
    device_id: Uuid,
}

async fn fcm_dismiss_test(
    State(state): State<AppState>,
    Json(body): Json<FcmDismissTestRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let fcm = state.fcm.as_ref().ok_or(StatusCode::SERVICE_UNAVAILABLE)?;

    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // device_id からテナントを特定
    let row = sqlx::query_as::<_, (Uuid,)>(
        "SELECT tenant_id FROM alc_api.devices WHERE id = $1 AND status = 'active'",
    )
    .bind(body.device_id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(|e| {
        tracing::error!("fcm_dismiss_test: query error: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?
    .ok_or(StatusCode::NOT_FOUND)?;

    let tenant_id = row.0;

    // 同一テナントの他の全デバイスに dismiss を送信
    let tokens = sqlx::query_as::<_, (String,)>(
        "SELECT fcm_token FROM alc_api.devices WHERE tenant_id = $1 AND id != $2 AND status = 'active' AND fcm_token IS NOT NULL",
    )
    .bind(tenant_id)
    .bind(body.device_id)
    .fetch_all(&mut *conn)
    .await
    .map_err(|e| db_err("fcm_dismiss_test tokens query", e))?;

    let mut sent = 0usize;
    for (token,) in &tokens {
        let mut data = std::collections::HashMap::new();
        data.insert("type".to_string(), "test_dismiss".to_string());
        if fcm.send_data_message(token, data).await.is_ok() {
            sent += 1;
        }
    }

    #[rustfmt::skip]
    tracing::info!("fcm_dismiss_test: sent dismiss to {sent}/{} devices", tokens.len());
    Ok(Json(serde_json::json!({ "sent": sent })))
}

// --- FCM テスト送信 (exclude指定、シグナリングサーバーから呼ばれる) ---

#[derive(Debug, Deserialize)]
struct TestFcmAllExcludeBody {
    #[serde(default)]
    exclude_device_ids: Vec<String>,
}

async fn test_fcm_all_exclude(
    State(state): State<AppState>,
    Json(body): Json<TestFcmAllExcludeBody>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let fcm = state.fcm.as_ref().ok_or(StatusCode::SERVICE_UNAVAILABLE)?;

    let rows = sqlx::query_as::<_, (Uuid, String, String)>(
        "SELECT id, device_name, fcm_token FROM alc_api.devices WHERE status = 'active' AND fcm_token IS NOT NULL AND call_enabled = true",
    )
    .fetch_all(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("test_fcm_all_exclude query error: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let exclude_set: std::collections::HashSet<&str> =
        body.exclude_device_ids.iter().map(|s| s.as_str()).collect();

    let mut sent = 0usize;
    let mut errors = 0usize;
    let mut results = Vec::<serde_json::Value>::new();

    for (device_id, device_name, token) in &rows {
        if exclude_set.contains(device_id.to_string().as_str()) {
            continue;
        }

        let mut data = std::collections::HashMap::new();
        data.insert("type".to_string(), "test".to_string());
        data.insert(
            "timestamp".to_string(),
            chrono::Utc::now().timestamp_millis().to_string(),
        );

        match fcm.send_data_message(token, data).await {
            Ok(()) => {
                sent += 1;
                results.push(serde_json::json!({
                    "device_id": device_id.to_string(),
                    "device_name": device_name,
                    "success": true,
                }));
            }
            Err(e) => {
                errors += 1;
                results.push(serde_json::json!({
                    "device_id": device_id.to_string(),
                    "device_name": device_name,
                    "success": false,
                    "error": e.to_string(),
                }));
            }
        }
    }

    #[rustfmt::skip]
    tracing::info!("test_fcm_all_exclude: sent={sent}, errors={errors}, excluded={}", exclude_set.len());
    Ok(Json(
        serde_json::json!({ "sent": sent, "errors": errors, "results": results }),
    ))
}

// --- FCM テスト送信 (管理者認証) ---

#[derive(Debug, Serialize)]
struct TestFcmResponse {
    success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

async fn test_fcm(
    State(state): State<AppState>,
    Extension(tenant_id): Extension<TenantId>,
    Path(id): Path<Uuid>,
) -> Result<Json<TestFcmResponse>, StatusCode> {
    let fcm = state.fcm.as_ref().ok_or_else(|| {
        tracing::warn!("test_fcm called but FCM is not configured");
        StatusCode::SERVICE_UNAVAILABLE
    })?;

    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let tid = tenant_id.0.to_string();
    set_current_tenant(&mut conn, &tid)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let row = sqlx::query_as::<_, (Option<String>,)>(
        "SELECT fcm_token FROM devices WHERE id = $1 AND tenant_id = $2::uuid",
    )
    .bind(id)
    .bind(&tid)
    .fetch_optional(&mut *conn)
    .await
    .map_err(|e| {
        tracing::error!("test_fcm query error: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?
    .ok_or(StatusCode::NOT_FOUND)?;

    let token = row.0.ok_or_else(|| {
        tracing::info!("test_fcm: device {} has no FCM token", id);
        StatusCode::BAD_REQUEST
    })?;

    let mut data = std::collections::HashMap::new();
    data.insert("type".to_string(), "test".to_string());
    data.insert(
        "timestamp".to_string(),
        chrono::Utc::now().timestamp_millis().to_string(),
    );

    match fcm.send_data_message(&token, data).await {
        Ok(()) => {
            tracing::info!("FCM test sent to device {}", id);
            Ok(Json(TestFcmResponse {
                success: true,
                error: None,
            }))
        }
        Err(e) => {
            tracing::error!("FCM test to device {} failed: {e}", id);
            Ok(Json(TestFcmResponse {
                success: false,
                error: Some(e.to_string()),
            }))
        }
    }
}

// --- FCM 一括テスト送信 (管理者認証) ---

#[derive(Debug, Serialize)]
struct TestFcmAllResponse {
    sent: usize,
    skipped: usize,
    errors: usize,
    results: Vec<TestFcmAllDeviceResult>,
}

#[derive(Debug, Serialize)]
struct TestFcmAllDeviceResult {
    device_id: Uuid,
    device_name: String,
    success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

async fn test_fcm_all(
    State(state): State<AppState>,
    Extension(tenant_id): Extension<TenantId>,
) -> Result<Json<TestFcmAllResponse>, StatusCode> {
    let fcm = state.fcm.as_ref().ok_or_else(|| {
        tracing::warn!("test_fcm_all called but FCM is not configured");
        StatusCode::SERVICE_UNAVAILABLE
    })?;

    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let tid = tenant_id.0.to_string();
    set_current_tenant(&mut conn, &tid)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let rows = sqlx::query_as::<_, (Uuid, String, String)>(
        "SELECT id, device_name, fcm_token FROM devices WHERE tenant_id = $1::uuid AND status = 'active' AND fcm_token IS NOT NULL",
    )
    .bind(&tid)
    .fetch_all(&mut *conn)
    .await
    .map_err(|e| {
        tracing::error!("test_fcm_all query error: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let mut sent = 0usize;
    let mut errors = 0usize;
    let mut results = Vec::new();

    for (device_id, device_name, token) in &rows {
        let mut data = std::collections::HashMap::new();
        data.insert("type".to_string(), "test".to_string());
        data.insert(
            "timestamp".to_string(),
            chrono::Utc::now().timestamp_millis().to_string(),
        );

        match fcm.send_data_message(token, data).await {
            Ok(()) => {
                sent += 1;
                results.push(TestFcmAllDeviceResult {
                    device_id: *device_id,
                    device_name: device_name.clone(),
                    success: true,
                    error: None,
                });
            }
            Err(e) => {
                errors += 1;
                results.push(TestFcmAllDeviceResult {
                    device_id: *device_id,
                    device_name: device_name.clone(),
                    success: false,
                    error: Some(e.to_string()),
                });
            }
        }
    }

    let skipped = 0; // All active devices with tokens are sent
    #[rustfmt::skip]
    tracing::info!("FCM test-all: sent={sent}, errors={errors}, total={}", rows.len());

    Ok(Json(TestFcmAllResponse {
        sent,
        skipped,
        errors,
        results,
    }))
}

// ============================================================
// バージョン報告 / OTA アップデート
// ============================================================

// --- バージョン報告 (認証不要、端末から呼ばれる) ---

#[derive(Debug, Deserialize)]
struct ReportVersionBody {
    device_id: Uuid,
    version_code: i32,
    version_name: String,
    #[serde(default)]
    is_device_owner: bool,
    #[serde(default)]
    is_dev_device: bool,
}

async fn report_version(
    State(state): State<AppState>,
    Json(body): Json<ReportVersionBody>,
) -> Result<StatusCode, StatusCode> {
    let tenant_id = sqlx::query_as::<_, (Option<Uuid>,)>("SELECT alc_api.lookup_device_tenant($1)")
        .bind(body.device_id)
        .fetch_optional(&state.pool)
        .await
        .map_err(|e| {
            tracing::error!("report_version lookup error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?
        .0
        .ok_or(StatusCode::NOT_FOUND)?;

    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    sqlx::query(
        r#"UPDATE devices
           SET app_version_code = $1, app_version_name = $2,
               is_device_owner = $3, is_dev_device = $4,
               app_version_reported_at = NOW(), updated_at = NOW()
           WHERE id = $5"#,
    )
    .bind(body.version_code)
    .bind(&body.version_name)
    .bind(body.is_device_owner)
    .bind(body.is_dev_device)
    .bind(body.device_id)
    .execute(&mut *conn)
    .await
    .map_err(|e| {
        tracing::error!("report_version update error: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    #[rustfmt::skip]
    tracing::info!("Version reported for device {}: v{}({}), device_owner={}, dev_device={}", body.device_id, body.version_name, body.version_code, body.is_device_owner, body.is_dev_device);
    Ok(StatusCode::NO_CONTENT)
}

// --- OTA アップデートトリガー ---

#[derive(Debug, Deserialize)]
struct TriggerUpdateBody {
    #[serde(default)]
    device_ids: Option<Vec<Uuid>>,
    #[serde(default)]
    version_code: Option<i32>,
    #[serde(default)]
    version_name: Option<String>,
    /// true: dev端末のみ, false/省略: 全端末
    #[serde(default)]
    dev_only: Option<bool>,
}

#[derive(Debug, Serialize)]
struct TriggerUpdateResponse {
    sent: usize,
    skipped: usize,
    already_updated: usize,
    errors: usize,
    results: Vec<TestFcmAllDeviceResult>,
}

/// 共通の OTA トリガー処理
async fn send_update_fcm(
    state: &AppState,
    conn: &mut sqlx::pool::PoolConnection<sqlx::Postgres>,
    tid: &str,
    body: &TriggerUpdateBody,
    dev_only: bool,
) -> Result<TriggerUpdateResponse, StatusCode> {
    let fcm = state.fcm.as_ref().ok_or_else(|| {
        tracing::warn!("trigger_update called but FCM is not configured");
        StatusCode::SERVICE_UNAVAILABLE
    })?;

    let query = if dev_only {
        r#"SELECT id, device_name, fcm_token, app_version_code
           FROM devices
           WHERE tenant_id = $1::uuid AND status = 'active'
             AND fcm_token IS NOT NULL AND is_dev_device = true"#
    } else {
        r#"SELECT id, device_name, fcm_token, app_version_code
           FROM devices
           WHERE tenant_id = $1::uuid AND status = 'active'
             AND fcm_token IS NOT NULL"#
    };

    let rows = sqlx::query_as::<_, (Uuid, String, String, Option<i32>)>(query)
        .bind(tid)
        .fetch_all(&mut **conn)
        .await
        .map_err(|e| {
            tracing::error!("trigger_update query error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let device_id_filter: Option<std::collections::HashSet<Uuid>> = body
        .device_ids
        .as_ref()
        .map(|ids| ids.iter().copied().collect());

    let mut sent = 0usize;
    let mut skipped = 0usize;
    let mut already_updated = 0usize;
    let mut errors = 0usize;
    let mut results = Vec::new();

    for (device_id, device_name, token, current_version) in &rows {
        if device_id_filter
            .as_ref()
            .is_some_and(|f| !f.contains(device_id))
        {
            skipped += 1;
            continue;
        }

        if let (Some(target_version), Some(current)) = (body.version_code, current_version) {
            if *current >= target_version {
                already_updated += 1;
                results.push(TestFcmAllDeviceResult {
                    device_id: *device_id,
                    device_name: device_name.clone(),
                    success: true,
                    error: Some("already up-to-date".to_string()),
                });
                continue;
            }
        }

        let mut data = std::collections::HashMap::new();
        data.insert("type".to_string(), "app_update".to_string());
        data.insert(
            "timestamp".to_string(),
            chrono::Utc::now().timestamp_millis().to_string(),
        );
        if let Some(vc) = body.version_code {
            data.insert("version_code".to_string(), vc.to_string());
        }
        if let Some(ref vn) = body.version_name {
            data.insert("version_name".to_string(), vn.clone());
        }

        match fcm.send_data_message(token, data).await {
            Ok(()) => {
                sent += 1;
                results.push(TestFcmAllDeviceResult {
                    device_id: *device_id,
                    device_name: device_name.clone(),
                    success: true,
                    error: None,
                });
            }
            Err(e) => {
                errors += 1;
                results.push(TestFcmAllDeviceResult {
                    device_id: *device_id,
                    device_name: device_name.clone(),
                    success: false,
                    error: Some(e.to_string()),
                });
            }
        }
    }

    #[rustfmt::skip]
    tracing::info!("trigger_update: sent={sent}, skipped={skipped}, already_updated={already_updated}, errors={errors}, dev_only={dev_only}");

    Ok(TriggerUpdateResponse {
        sent,
        skipped,
        already_updated,
        errors,
        results,
    })
}

/// テナント認証付き: 管理者ダッシュボードから全端末に OTA トリガー
async fn trigger_update(
    State(state): State<AppState>,
    Extension(tenant_id): Extension<TenantId>,
    Json(body): Json<TriggerUpdateBody>,
) -> Result<Json<TriggerUpdateResponse>, StatusCode> {
    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let tid = tenant_id.0.to_string();
    set_current_tenant(&mut conn, &tid)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let dev_only = body.dev_only.unwrap_or(false);
    let resp = send_update_fcm(&state, &mut conn, &tid, &body, dev_only).await?;
    Ok(Json(resp))
}

/// X-Internal-Secret 認証: CI (GitHub Actions) から dev 端末に OTA トリガー
async fn trigger_update_dev(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<TriggerUpdateBody>,
) -> Result<Json<TriggerUpdateResponse>, StatusCode> {
    // X-Internal-Secret 認証
    let expected_secret = std::env::var("FCM_INTERNAL_SECRET").map_err(|_| {
        tracing::warn!("trigger_update_dev: FCM_INTERNAL_SECRET not configured");
        StatusCode::SERVICE_UNAVAILABLE
    })?;
    let provided = headers
        .get("X-Internal-Secret")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if provided != expected_secret {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // 全テナントの dev 端末を対象にする (RLS バイパスなし: テナント一覧を取得して順に処理)
    let tenant_ids = sqlx::query_scalar::<_, String>(
        "SELECT DISTINCT tenant_id::text FROM alc_api.devices WHERE status = 'active' AND is_dev_device = true AND fcm_token IS NOT NULL",
    )
    .fetch_all(&mut *conn)
    .await
    .map_err(|e| {
        tracing::error!("trigger_update_dev tenant query error: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let mut total_resp = TriggerUpdateResponse {
        sent: 0,
        skipped: 0,
        already_updated: 0,
        errors: 0,
        results: Vec::new(),
    };

    for tid in &tenant_ids {
        set_current_tenant(&mut conn, tid)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        match send_update_fcm(&state, &mut conn, tid, &body, true).await {
            Ok(resp) => {
                total_resp.sent += resp.sent;
                total_resp.skipped += resp.skipped;
                total_resp.already_updated += resp.already_updated;
                total_resp.errors += resp.errors;
                total_resp.results.extend(resp.results);
            }
            Err(e) => tracing::error!("trigger_update_dev error for tenant {tid}: {e}"),
        }
    }

    Ok(Json(total_resp))
}

// ============================================================
// ヘルパー
// ============================================================

/// 6桁のユニークな登録コードを生成
async fn generate_unique_code(state: &AppState) -> Result<String, StatusCode> {
    for _ in 0..10 {
        let code_str = {
            let mut rng = rand::rng();
            let code: u32 = rng.random_range(100_000..1_000_000);
            code.to_string()
        };

        let exists = sqlx::query_as::<_, (bool,)>(
            r#"
            SELECT EXISTS(
                SELECT 1 FROM device_registration_requests
                WHERE registration_code = $1 AND status = 'pending'
                  AND (expires_at IS NULL OR expires_at > NOW())
            )
            "#,
        )
        .bind(&code_str)
        .fetch_one(&state.pool)
        .await
        .map_err(|e| db_err("generate_unique_code", e))?
        .0;

        if !exists {
            return Ok(code_str);
        }
    }
    unreachable!("code collision after 10 attempts")
}
