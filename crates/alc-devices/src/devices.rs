use axum::{
    extract::{Path, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    routing::{delete, get, post, put},
    Extension, Json, Router,
};
use chrono::{DateTime, Datelike, Timelike, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use alc_core::auth_middleware::{AuthUser, TenantId};
use alc_core::constant_time::constant_time_eq;
use alc_core::repository::devices::{
    DeviceRow, DeviceSettingsRow, FcmDeviceRow, RegistrationRequestRow,
};
use alc_core::AppState;

use crate::re_pair_policy::{
    evaluate_re_pair_request, RePairContext, RePairDecision, RePairPolicy,
};

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
        .route("/devices/re-pair", post(re_pair))
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
        .route("/devices/{id}/authorize-repair", post(authorize_repair))
}

/// DB エラーをログ出力して 500 を返すヘルパー
fn db_err(context: &str, e: sqlx::Error) -> StatusCode {
    tracing::error!("{context}: {e}");
    StatusCode::INTERNAL_SERVER_ERROR
}

/// FCM_INTERNAL_SECRET ヘッダー認証。
///
/// secret 未設定時は **fail-closed** で 503 を返す (旧実装は Ok を返す fail-open
/// で、env 設定漏れ時にエンドポイントが無認証開放されていた。ingest.rs の
/// fail-closed に揃える)。Refs #392。
fn check_internal_secret(headers: &HeaderMap) -> Result<(), StatusCode> {
    let Ok(expected) = std::env::var("FCM_INTERNAL_SECRET") else {
        tracing::error!("FCM_INTERNAL_SECRET not set; refusing request (fail-closed)");
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    };
    let provided = headers
        .get("X-Internal-Secret")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    // 文字列 `==` は非 constant-time でタイミング側チャネルの余地がある (Refs #393 M-2)
    if constant_time_eq(provided.as_bytes(), expected.as_bytes()) {
        Ok(())
    } else {
        Err(StatusCode::UNAUTHORIZED)
    }
}

// ============================================================
// 型定義 (HTTP レスポンス用 — DB 型は repository::devices に定義)
// ============================================================

#[derive(Debug, Serialize)]
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
    re_pair_authorized_until: Option<String>,
    last_re_pair_at: Option<String>,
    re_pair_count: i32,
    hardware_id: Option<String>,
}

impl From<DeviceRow> for Device {
    fn from(r: DeviceRow) -> Self {
        Self {
            id: r.id,
            tenant_id: r.tenant_id,
            device_name: r.device_name,
            device_type: r.device_type,
            phone_number: r.phone_number,
            user_id: r.user_id,
            status: r.status,
            approved_by: r.approved_by,
            approved_at: r.approved_at,
            last_seen_at: r.last_seen_at,
            call_enabled: r.call_enabled,
            call_schedule: r.call_schedule,
            fcm_token: r.fcm_token,
            last_login_employee_id: r.last_login_employee_id,
            last_login_employee_name: r.last_login_employee_name,
            last_login_employee_role: r.last_login_employee_role,
            app_version_code: r.app_version_code,
            app_version_name: r.app_version_name,
            is_device_owner: r.is_device_owner,
            is_dev_device: r.is_dev_device,
            always_on: r.always_on,
            watchdog_running: r.watchdog_running,
            created_at: r.created_at,
            updated_at: r.updated_at,
            re_pair_authorized_until: r.re_pair_authorized_until,
            last_re_pair_at: r.last_re_pair_at,
            re_pair_count: r.re_pair_count,
            hardware_id: r.hardware_id,
        }
    }
}

#[derive(Debug, Serialize)]
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

impl From<RegistrationRequestRow> for RegistrationRequest {
    fn from(r: RegistrationRequestRow) -> Self {
        Self {
            id: r.id,
            registration_code: r.registration_code,
            flow_type: r.flow_type,
            tenant_id: r.tenant_id,
            phone_number: r.phone_number,
            device_name: r.device_name,
            status: r.status,
            device_id: r.device_id,
            expires_at: r.expires_at,
            is_device_owner: r.is_device_owner,
            is_dev_device: r.is_dev_device,
            created_at: r.created_at,
        }
    }
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

    let result = state
        .devices
        .create_registration_request(&code, &device_name)
        .await
        .map_err(|e| db_err("create_registration_request", e))?;

    Ok(Json(CreateRegistrationResponse {
        registration_code: result.registration_code,
        expires_at: result.expires_at,
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
    /// 承認時に発行された device 保有 token (Refs #388)。端末はこれを保存し
    /// settings 取得時に X-Device-Token ヘッダで送る。approved 時のみ返る。
    #[serde(skip_serializing_if = "Option::is_none")]
    settings_token: Option<Uuid>,
}

async fn check_registration_status(
    State(state): State<AppState>,
    Path(code): Path<String>,
) -> Result<Json<RegistrationStatusResponse>, StatusCode> {
    let row = state
        .devices
        .get_registration_status(&code)
        .await
        .map_err(|e| {
            tracing::error!("check_registration_status error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    // 期限切れチェック (expires_at が設定されている場合)
    let status = if row.status == "pending" {
        if let Some(ref expires_at) = row.expires_at {
            let expired = state.devices.is_expired(expires_at).await.unwrap_or(false);
            if expired {
                "expired".to_string()
            } else {
                row.status.clone()
            }
        } else {
            row.status.clone()
        }
    } else {
        row.status.clone()
    };

    // approved になるまで device_id / tenant_id / device_name は返さない (Refs #388)。
    // コードを推測した第三者が pending リクエストから他テナントの tenant_id や
    // デバイス名を収集できる暴露経路を塞ぐ。フロント (DeviceRegistration.vue) は
    // approved 後にしかこれらを使わないため互換。
    let approved = status == "approved";
    Ok(Json(RegistrationStatusResponse {
        status,
        device_id: if approved { row.device_id } else { None },
        tenant_id: if approved { row.tenant_id } else { None },
        device_name: if approved { row.device_name } else { None },
        settings_token: if approved { row.settings_token } else { None },
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
    /// 即時登録 (url / device_owner) 時に発行される device 保有 token (Refs #388)
    #[serde(skip_serializing_if = "Option::is_none")]
    settings_token: Option<Uuid>,
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
                settings_token: None,
            }),
        )
    };
    let claim_db = |ctx: &str, e: sqlx::Error| {
        tracing::error!("{ctx}: {e}");
        claim_err("internal error")
    };

    // リクエスト検索
    let req = state
        .devices
        .find_claim_request(&body.registration_code)
        .await
        .map_err(|e| claim_db("claim lookup", e))?
        .ok_or_else(|| claim_err("無効な登録コードです"))?;

    if req.status != "pending" {
        return Err(claim_err("このコードは既に使用済みです"));
    }

    // 端末入力があればそちらを優先、なければ管理者が設定済みの名前をフォールバック
    let device_name = body
        .device_name
        .as_ref()
        .filter(|s| !s.is_empty())
        .cloned()
        .or(req.device_name)
        .unwrap_or_default();

    match req.flow_type.as_str() {
        "url" | "device_owner" => {
            // URL / Device Owner フロー: 即承認
            let tenant_id = req
                .tenant_id
                .ok_or_else(|| claim_err("無効なトークンです"))?;
            let is_do = req.flow_type == "device_owner";

            // settings 取得用の device 保有 token を発行 (Refs #388)
            let settings_token = Uuid::new_v4();
            let device_id = state
                .devices
                .claim_url_flow(
                    tenant_id,
                    &device_name,
                    body.phone_number.as_deref(),
                    is_do || req.is_device_owner,
                    req.is_dev_device,
                    req.id,
                    settings_token,
                )
                .await
                .map_err(|e| claim_db("claim url flow", e))?;

            Ok(Json(ClaimRegistrationResponse {
                success: true,
                flow_type: req.flow_type,
                device_id: Some(device_id),
                tenant_id: Some(tenant_id),
                message: None,
                settings_token: Some(settings_token),
            }))
        }
        "qr_permanent" => {
            // QR永久: pending のまま、管理者承認待ち
            state
                .devices
                .claim_update_permanent_qr(req.id, body.phone_number.as_deref(), &device_name)
                .await
                .map_err(|e| claim_db("claim update permanent qr", e))?;

            Ok(Json(ClaimRegistrationResponse {
                success: true,
                flow_type: "qr_permanent".into(),
                device_id: None,
                tenant_id: None,
                message: Some("管理者の承認待ちです".into()),
                settings_token: None,
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
    let rows = state.devices.list_devices(tenant.0).await.map_err(|e| {
        tracing::error!("list_devices error: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(rows.into_iter().map(Device::from).collect()))
}

// --- 承認待ちリクエスト一覧 ---

async fn list_pending(
    State(state): State<AppState>,
    Extension(tenant): Extension<TenantId>,
) -> Result<Json<Vec<RegistrationRequest>>, StatusCode> {
    let rows = state.devices.list_pending(tenant.0).await.map_err(|e| {
        tracing::error!("list_pending error: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(
        rows.into_iter().map(RegistrationRequest::from).collect(),
    ))
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

    state
        .devices
        .create_url_token(
            tenant.0,
            &code,
            &device_name,
            body.is_device_owner.unwrap_or(false),
            body.is_dev_device.unwrap_or(false),
        )
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

    state
        .devices
        .create_device_owner_token(
            tenant.0,
            &code,
            &device_name,
            body.is_dev_device.unwrap_or(false),
        )
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

    state
        .devices
        .create_permanent_qr(
            tenant.0,
            &code,
            &device_name,
            body.is_device_owner.unwrap_or(false),
            body.is_dev_device.unwrap_or(false),
        )
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
    // リクエスト取得
    let req = state
        .devices
        .find_approve_request(tenant.0, id)
        .await
        .map_err(|e| {
            tracing::error!("approve lookup error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    let device_name = body
        .device_name
        .or(req.device_name.clone())
        .unwrap_or_default();
    let device_type = req
        .phone_number
        .as_ref()
        .map(|_| "android")
        .unwrap_or("kiosk");
    let approved_by = auth.as_ref().map(|a| a.user_id);

    // settings 取得用の device 保有 token を発行 (Refs #388)。端末には status
    // ポーリング (approved 時) 経由で渡る。管理者向け response には載せない。
    let settings_token = Uuid::new_v4();
    let device_id = state
        .devices
        .approve_device(
            tenant.0,
            req.id,
            &device_name,
            device_type,
            req.phone_number.as_deref(),
            approved_by,
            req.is_device_owner,
            req.is_dev_device,
            settings_token,
        )
        .await
        .map_err(|e| {
            tracing::error!("approve create device error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

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
    // registration_code でリクエスト検索
    let req = state
        .devices
        .find_approve_by_code_request(tenant.0, &code)
        .await
        .map_err(|e| {
            tracing::error!("approve_by_code lookup error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or_else(|| {
            // code enumeration の検知用に「誰がどのテナントから外したか」を残す (Refs #388)。
            // tracing マクロを複数行に割るとフォーマット文字列行が行カバレッジに乗らない
            // (coverage gate に引っかかる) ため 1 行に収める。
            let approver = auth.as_ref().map(|a| a.user_id);
            let t = tenant.0;
            tracing::warn!("approve_by_code miss: tenant={t} approver={approver:?}");
            StatusCode::NOT_FOUND
        })?;

    let device_name = req.device_name.clone().unwrap_or_default();
    let device_type = req
        .phone_number
        .as_ref()
        .map(|_| "android")
        .unwrap_or("kiosk");
    let approved_by = auth.as_ref().map(|a| a.user_id);

    // settings 取得用の device 保有 token を発行 (Refs #388)。端末には status
    // ポーリング (approved 時) 経由で渡る。承認者向け response には載せない。
    let settings_token = Uuid::new_v4();
    let device_id = state
        .devices
        .approve_by_code(
            tenant.0,
            req.id,
            &device_name,
            device_type,
            req.phone_number.as_deref(),
            approved_by,
            req.is_device_owner,
            req.is_dev_device,
            settings_token,
        )
        .await
        .map_err(|e| {
            tracing::error!("approve_by_code create device error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

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
    let affected = state
        .devices
        .reject_device(tenant.0, id)
        .await
        .map_err(|e| {
            tracing::error!("reject_device error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    if !affected {
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
    let affected = state
        .devices
        .disable_device(tenant.0, id)
        .await
        .map_err(|e| {
            tracing::error!("disable_device error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    if !affected {
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
    let affected = state
        .devices
        .enable_device(tenant.0, id)
        .await
        .map_err(|e| {
            tracing::error!("enable_device error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    if !affected {
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
    let affected = state
        .devices
        .delete_device(tenant.0, id)
        .await
        .map_err(|e| {
            tracing::error!("delete_device error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    if !affected {
        return Err(StatusCode::NOT_FOUND);
    }

    Ok(StatusCode::NO_CONTENT)
}

// --- デバイス設定取得 (認証不要) ---

#[derive(Debug, Serialize)]
struct DeviceSettingsResponse {
    call_enabled: bool,
    call_schedule: Option<serde_json::Value>,
    status: String,
    last_login_employee_id: Option<Uuid>,
    last_login_employee_name: Option<String>,
    last_login_employee_role: Option<Vec<String>>,
    always_on: bool,
}

impl From<DeviceSettingsRow> for DeviceSettingsResponse {
    fn from(r: DeviceSettingsRow) -> Self {
        Self {
            call_enabled: r.call_enabled,
            call_schedule: r.call_schedule,
            status: r.status,
            last_login_employee_id: r.last_login_employee_id,
            last_login_employee_name: r.last_login_employee_name,
            last_login_employee_role: r.last_login_employee_role,
            always_on: r.always_on,
        }
    }
}

async fn get_device_settings(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(device_id): Path<Uuid>,
) -> Result<Json<DeviceSettingsResponse>, StatusCode> {
    let row = state
        .devices
        .get_device_settings(device_id)
        .await
        .map_err(|e| {
            tracing::error!("get_device_settings error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    // device 保有 token (X-Device-Token) の検証 (Refs #388)。移行期互換:
    // ヘッダが来たら厳格に検証、token 発行済みなのに未送信なら warn のみ
    // (Phase C で 401 化)、旧端末 (token 未発行 + 未送信) は従来動作。
    let provided = headers.get("X-Device-Token").and_then(|v| v.to_str().ok());
    match (provided, row.settings_token) {
        (Some(p), Some(t)) if constant_time_eq(p.as_bytes(), t.to_string().as_bytes()) => {}
        (Some(_), _) => return Err(StatusCode::UNAUTHORIZED),
        (None, Some(_)) => {
            // rustfmt の折り返しでフォーマット文字列行が coverage に乗らないため短く 1 行
            tracing::warn!("device_settings token not sent: {device_id}");
        }
        (None, None) => {}
    }

    Ok(Json(DeviceSettingsResponse::from(row)))
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
    let msg = format!(
        "update_call_settings: device={id} call_enabled={} always_on={:?}",
        body.call_enabled, body.always_on
    );
    tracing::info!("{msg}");

    let affected = state
        .devices
        .update_call_settings(
            tenant.0,
            id,
            body.call_enabled,
            body.call_schedule.as_ref(),
            body.always_on,
        )
        .await
        .map_err(|e| {
            tracing::error!("update_call_settings error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    if !affected {
        return Err(StatusCode::NOT_FOUND);
    }

    // always_on が変更された場合、FCM で端末に通知して設定を再取得させる
    if let (true, Some(fcm)) = (body.always_on.is_some(), state.fcm.as_ref()) {
        // RLS を回避して fcm_token を取得
        let token_row = state
            .devices
            .get_fcm_token_bypass_rls(id)
            .await
            .ok()
            .flatten();

        let msg = format!(
            "FCM settings_changed: device={id} token={:?}",
            token_row.as_ref().map(|r| r.is_some())
        );
        tracing::info!("{msg}");

        if let Some(Some(token)) = token_row {
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
    let tenant_id = state
        .devices
        .lookup_device_tenant(body.device_id)
        .await
        .map_err(|e| {
            tracing::error!("report_watchdog_state lookup error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    state
        .devices
        .update_watchdog_state(body.device_id, tenant_id, body.running)
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
    let tenant_id = state
        .devices
        .lookup_device_tenant(body.device_id)
        .await
        .map_err(|e| {
            tracing::error!("register_fcm_token lookup error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    state
        .devices
        .update_fcm_token(body.device_id, tenant_id, &body.fcm_token)
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
    let tenant_id = state
        .devices
        .lookup_device_tenant(body.device_id)
        .await
        .map_err(|e| {
            tracing::error!("update_last_login lookup error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    state
        .devices
        .update_last_login(
            body.device_id,
            tenant_id,
            body.employee_id,
            &body.employee_name,
            &body.employee_role,
        )
        .await
        .map_err(|e| {
            tracing::error!("update_last_login error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let msg = format!(
        "Last login updated for device {}: {} ({})",
        body.device_id,
        body.employee_name,
        body.employee_role.join(",")
    );
    tracing::info!("{msg}");
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

async fn fcm_notify_call(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<FcmNotifyCallBody>,
) -> Result<Json<FcmNotifyCallResponse>, StatusCode> {
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
    let devices = state.devices.list_fcm_devices().await.map_err(|e| {
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
fn should_notify_device(device: &FcmDeviceRow) -> bool {
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

    // device_id からテナントを特定
    let tenant_row = state
        .devices
        .get_device_tenant_active(body.device_id)
        .await
        .map_err(|e| {
            tracing::error!("fcm_dismiss_test: query error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    let tenant_id = tenant_row.tenant_id;

    // 同一テナントの他の全デバイスに dismiss を送信
    let tokens = state
        .devices
        .list_tenant_fcm_tokens_except(tenant_id, body.device_id)
        .await
        .map_err(|e| db_err("fcm_dismiss_test tokens query", e))?;

    let mut sent = 0usize;
    for token in &tokens {
        let mut data = std::collections::HashMap::new();
        data.insert("type".to_string(), "test_dismiss".to_string());
        if fcm.send_data_message(token, data).await.is_ok() {
            sent += 1;
        }
    }

    let msg = format!(
        "fcm_dismiss_test: sent dismiss to {sent}/{} devices",
        tokens.len()
    );
    tracing::info!("{msg}");
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

    let rows = state
        .devices
        .list_all_callable_devices()
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

    for device in &rows {
        if exclude_set.contains(device.id.to_string().as_str()) {
            continue;
        }

        let mut data = std::collections::HashMap::new();
        data.insert("type".to_string(), "test".to_string());
        data.insert(
            "timestamp".to_string(),
            chrono::Utc::now().timestamp_millis().to_string(),
        );

        match fcm.send_data_message(&device.fcm_token, data).await {
            Ok(()) => {
                sent += 1;
                results.push(serde_json::json!({
                    "device_id": device.id.to_string(),
                    "device_name": device.device_name,
                    "success": true,
                }));
            }
            Err(e) => {
                errors += 1;
                results.push(serde_json::json!({
                    "device_id": device.id.to_string(),
                    "device_name": device.device_name,
                    "success": false,
                    "error": e.to_string(),
                }));
            }
        }
    }

    let msg = format!(
        "test_fcm_all_exclude: sent={sent}, errors={errors}, excluded={}",
        exclude_set.len()
    );
    tracing::info!("{msg}");
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

    let row = state
        .devices
        .get_device_fcm_token(tenant_id.0, id)
        .await
        .map_err(|e| {
            tracing::error!("test_fcm query error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    let token = row.ok_or_else(|| {
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

    let rows = state
        .devices
        .list_tenant_fcm_devices(tenant_id.0)
        .await
        .map_err(|e| {
            tracing::error!("test_fcm_all query error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let mut sent = 0usize;
    let mut errors = 0usize;
    let mut results = Vec::new();

    for device in &rows {
        let mut data = std::collections::HashMap::new();
        data.insert("type".to_string(), "test".to_string());
        data.insert(
            "timestamp".to_string(),
            chrono::Utc::now().timestamp_millis().to_string(),
        );

        match fcm.send_data_message(&device.fcm_token, data).await {
            Ok(()) => {
                sent += 1;
                results.push(TestFcmAllDeviceResult {
                    device_id: device.id,
                    device_name: device.device_name.clone(),
                    success: true,
                    error: None,
                });
            }
            Err(e) => {
                errors += 1;
                results.push(TestFcmAllDeviceResult {
                    device_id: device.id,
                    device_name: device.device_name.clone(),
                    success: false,
                    error: Some(e.to_string()),
                });
            }
        }
    }

    let skipped = 0; // All active devices with tokens are sent
    let msg = format!(
        "FCM test-all: sent={sent}, errors={errors}, total={}",
        rows.len()
    );
    tracing::info!("{msg}");

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
    let tenant_id = state
        .devices
        .lookup_device_tenant(body.device_id)
        .await
        .map_err(|e| {
            tracing::error!("report_version lookup error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    state
        .devices
        .report_version(
            body.device_id,
            tenant_id,
            body.version_code,
            &body.version_name,
            body.is_device_owner,
            body.is_dev_device,
        )
        .await
        .map_err(|e| {
            tracing::error!("report_version update error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let msg = format!(
        "Version reported for device {}: v{}({}), device_owner={}, dev_device={}",
        body.device_id,
        body.version_name,
        body.version_code,
        body.is_device_owner,
        body.is_dev_device
    );
    tracing::info!("{msg}");
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
    /// 指定時はアプリがこの URL から APK を DL する (PR dev build の prerelease asset 等)。
    /// 省略時はアプリ側が releases/latest に fallback。
    #[serde(default)]
    download_url: Option<String>,
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
    tenant_id: Uuid,
    body: &TriggerUpdateBody,
    dev_only: bool,
) -> Result<TriggerUpdateResponse, StatusCode> {
    let fcm = state.fcm.as_ref().ok_or_else(|| {
        tracing::warn!("trigger_update called but FCM is not configured");
        StatusCode::SERVICE_UNAVAILABLE
    })?;

    let rows = state
        .devices
        .list_ota_devices(tenant_id, dev_only)
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

    for device in &rows {
        if device_id_filter
            .as_ref()
            .is_some_and(|f| !f.contains(&device.id))
        {
            skipped += 1;
            continue;
        }

        if let (Some(target_version), Some(current)) = (body.version_code, device.app_version_code)
        {
            if current >= target_version {
                already_updated += 1;
                results.push(TestFcmAllDeviceResult {
                    device_id: device.id,
                    device_name: device.device_name.clone(),
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
        if let Some(ref url) = body.download_url {
            data.insert("download_url".to_string(), url.clone());
        }

        match fcm.send_data_message(&device.fcm_token, data).await {
            Ok(()) => {
                sent += 1;
                results.push(TestFcmAllDeviceResult {
                    device_id: device.id,
                    device_name: device.device_name.clone(),
                    success: true,
                    error: None,
                });
            }
            Err(e) => {
                errors += 1;
                results.push(TestFcmAllDeviceResult {
                    device_id: device.id,
                    device_name: device.device_name.clone(),
                    success: false,
                    error: Some(e.to_string()),
                });
            }
        }
    }

    let msg = format!("trigger_update: sent={sent}, skipped={skipped}, already_updated={already_updated}, errors={errors}, dev_only={dev_only}");
    tracing::info!("{msg}");

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
    let dev_only = body.dev_only.unwrap_or(false);
    let resp = send_update_fcm(&state, tenant_id.0, &body, dev_only).await?;
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
    // 文字列 `!=` は非 constant-time でタイミング側チャネルの余地がある (Refs #393 M-2)
    if !constant_time_eq(provided.as_bytes(), expected_secret.as_bytes()) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    // 全テナントの dev 端末を対象にする
    let tenant_ids = state
        .devices
        .list_dev_device_tenant_ids()
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
        let tenant_uuid = Uuid::parse_str(tid).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        if let Ok(resp) = send_update_fcm(&state, tenant_uuid, &body, true).await {
            total_resp.sent += resp.sent;
            total_resp.skipped += resp.skipped;
            total_resp.already_updated += resp.already_updated;
            total_resp.errors += resp.errors;
            total_resp.results.extend(resp.results);
        }
    }

    Ok(Json(total_resp))
}

// ============================================================
// ヘルパー
// ============================================================

/// ユニークな登録コードを生成 (衝突チェック付き)。
///
/// 旧実装の 6 桁数字 (900k 空間) は総当たりで他テナントの承認待ちコードに当てられ、
/// approve-by-code の越境奪取 / tenant_id 収集に使えた (Refs #388)。コードは QR /
/// URL 経由で受け渡され人間の手入力性は不要なため、UUID v4 (122bit) に引き上げて
/// enumeration を計算量的に不可能にする。
async fn generate_unique_code(state: &AppState) -> Result<String, StatusCode> {
    loop {
        let code_str = Uuid::new_v4().to_string();
        let exists = state
            .devices
            .code_exists(&code_str)
            .await
            .map_err(|e| db_err("generate_unique_code", e))?;
        if exists {
            continue;
        }
        return Ok(code_str);
    }
}

// ============================================================
// kiosk 端末 re-pair (再認証、Refs #495)
//
// 設計 SoT: docs/plan-device-repair.md。管理者が時限 window を開け
// (authorize_repair、テナント認証)、端末がその window 内で
// auth-worker から device credential を再取得する (re_pair、認証不要)。
// ============================================================

const RE_PAIR_WINDOW_SECS_DEFAULT: i64 = 900;
const RE_PAIR_COOLDOWN_SECS_DEFAULT: i64 = 600;

fn env_i64(key: &str, default: i64) -> i64 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(default)
}

fn env_bool(key: &str, default: bool) -> bool {
    match std::env::var(key) {
        Ok(v) => v == "true",
        Err(_) => default,
    }
}

fn re_pair_window_secs() -> i64 {
    env_i64("RE_PAIR_WINDOW_SECS", RE_PAIR_WINDOW_SECS_DEFAULT)
}

fn re_pair_cooldown_secs() -> i64 {
    env_i64("RE_PAIR_COOLDOWN_SECS", RE_PAIR_COOLDOWN_SECS_DEFAULT)
}

fn re_pair_require_admin() -> bool {
    // デフォルト on (主防御)。明示的に "false" が来た時だけ off にする。
    match std::env::var("RE_PAIR_REQUIRE_ADMIN") {
        Ok(v) => v != "false",
        Err(_) => true,
    }
}

fn re_pair_require_token() -> bool {
    env_bool("RE_PAIR_REQUIRE_TOKEN", false)
}

// --- 管理者: 時限 window を開ける ---

#[derive(Debug, Deserialize, Default)]
struct AuthorizeRepairBody {
    #[serde(default)]
    reset_binding: bool,
}

#[derive(Debug, Serialize)]
struct AuthorizeRepairResponse {
    authorized_until: DateTime<Utc>,
}

async fn authorize_repair(
    State(state): State<AppState>,
    Extension(tenant): Extension<TenantId>,
    Path(id): Path<Uuid>,
    Json(body): Json<AuthorizeRepairBody>,
) -> Result<Json<AuthorizeRepairResponse>, StatusCode> {
    let authorized_until = state
        .devices
        .authorize_repair(tenant.0, id, re_pair_window_secs(), body.reset_binding)
        .await
        .map_err(|e| {
            tracing::error!("authorize_repair error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(AuthorizeRepairResponse { authorized_until }))
}

// --- 端末: device credential を再取得 (認証不要、window が事実上の認可) ---

#[derive(Debug, Deserialize)]
struct RePairBody {
    device_id: Uuid,
    #[serde(default)]
    hardware_id: Option<String>,
    #[serde(default)]
    settings_token: Option<Uuid>,
}

// Debug は意図的に derive しない (device_secret を含むため誤 log を防ぐ)。
// PairedCredential 側 (alc-core::device_pair_client) の redact 済み Debug と
// 揃えたいところだが、devices.rs は coverage_100.toml の 100% gate 対象
// (`type = "combined"`) で未使用コードが即 regression になるため、ここでは
// 単に derive しないことで `{:?}` 誤用をコンパイルエラーで検知する。
#[derive(Serialize)]
struct RePairResponse {
    auth_device_id: String,
    device_secret: String,
}

async fn re_pair(
    State(state): State<AppState>,
    Json(body): Json<RePairBody>,
) -> Result<(HeaderMap, Json<RePairResponse>), StatusCode> {
    let mut no_store = HeaderMap::new();
    no_store.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));

    let row = state
        .devices
        .get_device_re_pair_state(body.device_id)
        .await
        .map_err(|e| {
            tracing::error!("re_pair lookup error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    let policy = RePairPolicy {
        require_admin_window: re_pair_require_admin(),
        require_settings_token: re_pair_require_token(),
        cooldown_secs: re_pair_cooldown_secs(),
    };
    let ctx = RePairContext {
        status: row.status.clone(),
        authorized_until: row.re_pair_authorized_until,
        last_re_pair_at: row.last_re_pair_at,
        bound_hardware_id: row.hardware_id.clone(),
        requested_hardware_id: body.hardware_id.clone(),
        provided_settings_token: body.settings_token,
        stored_settings_token: row.settings_token,
    };

    let bind_hardware_id = match evaluate_re_pair_request(&ctx, &policy, Utc::now()) {
        RePairDecision::DenyNotFound => {
            tracing::warn!("re_pair denied (not_found) device_id={}", body.device_id);
            return Err(StatusCode::NOT_FOUND);
        }
        RePairDecision::DenyTooManyRequests => {
            tracing::warn!("re_pair denied (cooldown) device_id={}", body.device_id);
            return Err(StatusCode::TOO_MANY_REQUESTS);
        }
        RePairDecision::Allow { bind_hardware_id } => bind_hardware_id,
    };

    let Some(pair_client) = state.device_pair_client.as_ref() else {
        tracing::error!("re_pair: device_pair_client not configured");
        return Err(StatusCode::NOT_FOUND);
    };

    let device_id = body.device_id;
    let tenant_id = row.tenant_id;
    let label = format!("alc-app:{device_id}");
    let credential = pair_client.mint(tenant_id, &label).await.map_err(|e| {
        tracing::error!("re_pair: pair-internal call failed device_id={device_id} err={e:?}");
        StatusCode::NOT_FOUND
    })?;

    let consumed = state
        .devices
        .record_re_pair_success(
            tenant_id,
            device_id,
            row.re_pair_authorized_until,
            bind_hardware_id.as_deref(),
        )
        .await
        .map_err(|e| {
            tracing::error!("re_pair: record success failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    if !consumed {
        // 並行リクエストが先に window を消費 (or 別 authorize-repair が書き
        // 換え) していた。mint 済み credential はこの応答では返さない
        // (auth-worker 側 replace_label で最終的にどちらが有効かは PR2 scope)。
        tracing::warn!("re_pair denied (race, window already consumed) device_id={device_id}");
        return Err(StatusCode::NOT_FOUND);
    }

    tracing::info!("device re-pair granted device_id={device_id} tenant_id={tenant_id}");

    Ok((
        no_store,
        Json(RePairResponse {
            auth_device_id: credential.device_id,
            device_secret: credential.device_secret,
        }),
    ))
}
