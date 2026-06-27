/// SSO Provider Config 管理 REST API
/// auth-worker の admin/sso ページから呼ばれる
use axum::{
    extract::{Query, State},
    http::StatusCode,
    routing::{delete, get, post},
    Extension, Json, Router,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::bot_admin::is_developer_email;
use alc_core::auth_middleware::AuthUser;
use alc_core::repository::sso_admin::SsoConfigRow;
use alc_core::AppState;

#[derive(Debug, Serialize)]
struct ListResponse {
    configs: Vec<SsoConfigRow>,
}

#[derive(Debug, Deserialize)]
struct UpsertRequest {
    provider: String,
    client_id: String,
    client_secret: Option<String>,
    external_org_id: String,
    woff_id: Option<String>,
    enabled: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct DeleteRequest {
    provider: String,
}

#[derive(Debug, Deserialize)]
struct ExportQuery {
    tenant_id: uuid::Uuid,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/admin/sso/configs", get(list_configs))
        .route("/admin/sso/configs", post(upsert_config))
        .route("/admin/sso/configs", delete(delete_config))
        .route("/admin/sso/configs/export", get(export_configs))
}

/// GET /admin/sso/configs — テナントの SSO 設定一覧
async fn list_configs(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthUser>,
) -> Result<Json<ListResponse>, StatusCode> {
    // admin のみ
    if auth_user.role != "admin" {
        return Err(StatusCode::FORBIDDEN);
    }

    let configs = state
        .sso_admin
        .list_configs(auth_user.tenant_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to list SSO configs: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(ListResponse { configs }))
}

/// POST /admin/sso/configs — SSO 設定の作成/更新
async fn upsert_config(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthUser>,
    Json(body): Json<UpsertRequest>,
) -> Result<Json<SsoConfigRow>, StatusCode> {
    if auth_user.role != "admin" {
        return Err(StatusCode::FORBIDDEN);
    }

    // client_secret を暗号化
    let encrypted_secret = if let Some(ref secret) = body.client_secret {
        if secret.is_empty() {
            None
        } else {
            let key = std::env::var("SSO_ENCRYPTION_KEY")
                .or_else(|_| std::env::var("JWT_SECRET"))
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            Some(encrypt_secret(secret, &key).expect("AES-256-GCM encrypt infallible"))
        }
    } else {
        None
    };

    let enabled = body.enabled.unwrap_or(true);

    let config = if let Some(ref encrypted) = encrypted_secret {
        state
            .sso_admin
            .upsert_config_with_secret(
                auth_user.tenant_id,
                &body.provider,
                &body.client_id,
                encrypted,
                &body.external_org_id,
                body.woff_id.as_deref(),
                enabled,
            )
            .await
    } else {
        state
            .sso_admin
            .upsert_config_without_secret(
                auth_user.tenant_id,
                &body.provider,
                &body.client_id,
                &body.external_org_id,
                body.woff_id.as_deref(),
                enabled,
            )
            .await
    };

    config.map(Json).map_err(|e| {
        // secret 無し更新で対象 config が無い場合は新規作成不可 → 400
        if encrypted_secret.is_none() && matches!(e, sqlx::Error::RowNotFound) {
            tracing::warn!("SSO upsert without secret but config missing");
            return StatusCode::BAD_REQUEST;
        }
        tracing::error!("Failed to upsert SSO config: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })
}

/// DELETE /admin/sso/configs — SSO 設定の削除
async fn delete_config(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthUser>,
    Json(body): Json<DeleteRequest>,
) -> Result<StatusCode, StatusCode> {
    if auth_user.role != "admin" {
        return Err(StatusCode::FORBIDDEN);
    }

    let rows = state
        .sso_admin
        .delete_config(auth_user.tenant_id, &body.provider)
        .await
        .map_err(|e| {
            tracing::error!("Failed to delete SSO config: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    // 0 行 = 該当テナントに該当 provider の config が無い → 404 で明示
    if rows == 0 {
        return Err(StatusCode::NOT_FOUND);
    }

    Ok(StatusCode::NO_CONTENT)
}

/// GET /admin/sso/configs/export — developer 限定。
/// staging import と互換のシェイプ (sso_provider_configs のみ populate) で吐く。
async fn export_configs(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthUser>,
    Query(params): Query<ExportQuery>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    if !is_developer_email(&auth_user.email) {
        tracing::warn!("sso export refused: {}", auth_user.email);
        return Err(StatusCode::FORBIDDEN);
    }

    let tid = params.tenant_id;

    let tenant = state
        .sso_admin
        .get_tenant_for_export(tid)
        .await
        .map_err(|e| {
            tracing::error!("Failed to fetch tenant for sso export: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    let sso_provider_configs = state
        .sso_admin
        .list_configs_for_export(tid)
        .await
        .map_err(|e| {
            tracing::error!("Failed to fetch sso_provider_configs for export: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(json!({
        "version": 1,
        "exported_at": Utc::now().to_rfc3339(),
        "tenant_id": tid.to_string(),
        "data": {
            "tenant": tenant,
            "users": [],
            "employees": [],
            "devices": [],
            "tenko_schedules": [],
            "webhook_configs": [],
            "tenant_allowed_emails": [],
            "sso_provider_configs": sso_provider_configs,
            "tenko_call_numbers": [],
            "tenko_call_drivers": [],
            "bot_configs": [],
            "notify_line_configs": [],
            "notify_recipients": []
        }
    })))
}

/// AES-256-GCM で暗号化（rust-logi と同じ形式）
fn encrypt_secret(plaintext: &str, key_material: &str) -> Result<String, String> {
    use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
    use ring::aead::{self, Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM};
    use ring::rand::{SecureRandom, SystemRandom};
    use sha2::{Digest, Sha256};

    let mut key_bytes = [0u8; 32];
    let hash = Sha256::digest(key_material.as_bytes());
    key_bytes.copy_from_slice(&hash);

    let unbound_key =
        UnboundKey::new(&AES_256_GCM, &key_bytes).map_err(|e| format!("Key error: {e}"))?;
    let key = LessSafeKey::new(unbound_key);

    let rng = SystemRandom::new();
    let mut nonce_bytes = [0u8; 12];
    rng.fill(&mut nonce_bytes)
        .map_err(|e| format!("RNG error: {e}"))?;
    let nonce = Nonce::assume_unique_for_key(nonce_bytes);

    let mut in_out = plaintext.as_bytes().to_vec();
    let tag_len = aead::AES_256_GCM.tag_len();
    in_out.extend(vec![0u8; tag_len]);

    key.seal_in_place_separate_tag(nonce, Aad::empty(), &mut in_out[..plaintext.len()])
        .map(|tag| {
            in_out[plaintext.len()..].copy_from_slice(tag.as_ref());
        })
        .map_err(|e| format!("Encryption error: {e}"))?;

    let mut result = Vec::with_capacity(12 + in_out.len());
    result.extend_from_slice(&nonce_bytes);
    result.extend_from_slice(&in_out);

    Ok(BASE64.encode(&result))
}
