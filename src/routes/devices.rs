use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get, post},
    Extension, Json, Router,
};
use rand::RngExt;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::db::tenant::set_current_tenant;
use crate::middleware::auth::{AuthUser, TenantId};
use crate::AppState;

/// 公開ルート (認証不要) - 端末側から呼ばれる
pub fn public_router() -> Router<AppState> {
    Router::new()
        .route("/devices/register/request", post(create_registration_request))
        .route(
            "/devices/register/status/{code}",
            get(check_registration_status),
        )
        .route("/devices/register/claim", post(claim_registration))
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
        .route("/devices/approve/{id}", post(approve_device))
        .route("/devices/approve-by-code/{code}", post(approve_by_code))
        .route("/devices/reject/{id}", post(reject_device))
        .route("/devices/disable/{id}", post(disable_device))
        .route("/devices/enable/{id}", post(enable_device))
        .route("/devices/{id}", delete(delete_device))
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
    .map_err(|e| {
        tracing::error!("create_registration_request error: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

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
}

async fn check_registration_status(
    State(state): State<AppState>,
    Path(code): Path<String>,
) -> Result<Json<RegistrationStatusResponse>, StatusCode> {
    let row = sqlx::query_as::<_, (String, Option<Uuid>, Option<Uuid>, Option<String>)>(
        r#"
        SELECT status, device_id, tenant_id, expires_at::text
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
            let expired = sqlx::query_as::<_, (bool,)>(
                "SELECT $1::timestamptz < NOW()",
            )
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

    // リクエスト検索
    let req = sqlx::query_as::<_, (Uuid, String, Option<Uuid>, String, Option<String>)>(
        r#"
        SELECT id, flow_type, tenant_id, status, expires_at::text
        FROM device_registration_requests
        WHERE registration_code = $1
        "#,
    )
    .bind(&body.registration_code)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("claim_registration lookup error: {e}");
        claim_err("internal error")
    })?
    .ok_or_else(|| claim_err("無効な登録コードです"))?;

    let (req_id, flow_type, tenant_id, status, _expires_at) = req;

    if status != "pending" {
        return Err(claim_err("このコードは既に使用済みです"));
    }

    let device_name = body.device_name.clone().unwrap_or_default();

    match flow_type.as_str() {
        "url" => {
            // URLフロー: 即承認 (管理者がURL作成=承認)
            let tenant_id = tenant_id.ok_or_else(|| claim_err("無効なトークンです"))?;

            let mut tx = state.pool.begin().await.map_err(|e| {
                tracing::error!("claim tx begin error: {e}");
                claim_err("internal error")
            })?;

            set_current_tenant(&mut tx, &tenant_id.to_string())
                .await
                .map_err(|e| {
                    tracing::error!("claim set_tenant error: {e}");
                    claim_err("internal error")
                })?;

            // デバイス作成
            let device_id = sqlx::query_as::<_, (Uuid,)>(
                r#"
                INSERT INTO devices (tenant_id, device_name, device_type, phone_number, status, approved_at)
                VALUES ($1, $2, 'android', $3, 'active', NOW())
                RETURNING id
                "#,
            )
            .bind(tenant_id)
            .bind(&device_name)
            .bind(&body.phone_number)
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| {
                tracing::error!("claim create device error: {e}");
                claim_err("internal error")
            })?
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
            .map_err(|e| {
                tracing::error!("claim update request error: {e}");
                claim_err("internal error")
            })?;

            tx.commit().await.map_err(|e| {
                tracing::error!("claim tx commit error: {e}");
                claim_err("internal error")
            })?;

            Ok(Json(ClaimRegistrationResponse {
                success: true,
                flow_type: "url".into(),
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
            .map_err(|e| {
                tracing::error!("claim update permanent qr error: {e}");
                claim_err("internal error")
            })?;

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
               created_at::text, updated_at::text
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
               status, device_id, expires_at::text, created_at::text
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
}

#[derive(Debug, Serialize)]
struct CreateTokenResponse {
    registration_code: String,
    registration_url: String,
}

async fn create_url_token(
    State(state): State<AppState>,
    Extension(tenant): Extension<TenantId>,
    Extension(auth): Extension<AuthUser>,
    Json(body): Json<CreateTokenBody>,
) -> Result<Json<CreateTokenResponse>, StatusCode> {
    let _ = &auth; // JWT必須を保証
    let code = Uuid::new_v4().to_string();
    let device_name = body.device_name.unwrap_or_default();

    sqlx::query(
        r#"
        INSERT INTO device_registration_requests
            (registration_code, flow_type, tenant_id, device_name, status, expires_at)
        VALUES ($1, 'url', $2, $3, 'pending', NOW() + INTERVAL '24 hours')
        "#,
    )
    .bind(&code)
    .bind(tenant.0)
    .bind(&device_name)
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

// --- QR永久: 管理者がコード生成 ---

#[derive(Debug, Deserialize)]
struct CreatePermanentQrBody {
    device_name: Option<String>,
}

#[derive(Debug, Serialize)]
struct CreatePermanentQrResponse {
    registration_code: String,
}

async fn create_permanent_qr(
    State(state): State<AppState>,
    Extension(tenant): Extension<TenantId>,
    Extension(auth): Extension<AuthUser>,
    Json(body): Json<CreatePermanentQrBody>,
) -> Result<Json<CreatePermanentQrResponse>, StatusCode> {
    let _ = &auth;
    let code = Uuid::new_v4().to_string();
    let device_name = body.device_name.unwrap_or_default();

    sqlx::query(
        r#"
        INSERT INTO device_registration_requests
            (registration_code, flow_type, tenant_id, device_name, status)
        VALUES ($1, 'qr_permanent', $2, $3, 'pending')
        "#,
    )
    .bind(&code)
    .bind(tenant.0)
    .bind(&device_name)
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
    Extension(auth): Extension<AuthUser>,
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
    let req = sqlx::query_as::<_, (Uuid, String, Option<String>, Option<String>, String)>(
        r#"
        SELECT id, flow_type, phone_number, device_name, status
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

    let device_name = body
        .device_name
        .or(req.3.clone())
        .unwrap_or_default();

    let device_type = if req.2.is_some() { "android" } else { "kiosk" };

    // デバイス作成
    let device_id = sqlx::query_as::<_, (Uuid,)>(
        r#"
        INSERT INTO devices (tenant_id, device_name, device_type, phone_number, status, approved_by, approved_at)
        VALUES ($1, $2, $3, $4, 'active', $5, NOW())
        RETURNING id
        "#,
    )
    .bind(tenant.0)
    .bind(&device_name)
    .bind(device_type)
    .bind(&req.2)
    .bind(auth.user_id)
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
    Extension(auth): Extension<AuthUser>,
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
    let req = sqlx::query_as::<_, (Uuid, String, Option<String>, Option<String>, String)>(
        r#"
        SELECT id, flow_type, phone_number, device_name, status
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

    let device_id = sqlx::query_as::<_, (Uuid,)>(
        r#"
        INSERT INTO devices (tenant_id, device_name, device_type, phone_number, status, approved_by, approved_at)
        VALUES ($1, $2, $3, $4, 'active', $5, NOW())
        RETURNING id
        "#,
    )
    .bind(tenant.0)
    .bind(&device_name)
    .bind(device_type)
    .bind(&req.2)
    .bind(auth.user_id)
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
    Extension(auth): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let _ = &auth;
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
        .map_err(|e| {
            tracing::error!("generate_unique_code error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .0;

        if !exists {
            return Ok(code_str);
        }
    }
    tracing::error!("Failed to generate unique code after 10 attempts");
    Err(StatusCode::INTERNAL_SERVER_ERROR)
}
