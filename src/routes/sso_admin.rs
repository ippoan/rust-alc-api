/// SSO Provider Config 管理 REST API
/// auth-worker の admin/sso ページから呼ばれる
use axum::{
    extract::State,
    http::StatusCode,
    routing::{delete, get, post},
    Extension, Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::db::tenant::set_current_tenant;
use crate::middleware::auth::AuthUser;
use crate::AppState;

#[derive(Debug, Serialize, sqlx::FromRow)]
struct SsoConfigResponse {
    provider: String,
    client_id: String,
    external_org_id: String,
    enabled: bool,
    woff_id: Option<String>,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize)]
struct ListResponse {
    configs: Vec<SsoConfigResponse>,
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

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/admin/sso/configs", get(list_configs))
        .route("/admin/sso/configs", post(upsert_config))
        .route("/admin/sso/configs", delete(delete_config))
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

    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &auth_user.tenant_id.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let configs = sqlx::query_as::<_, SsoConfigResponse>(
        r#"
        SELECT provider, client_id, external_org_id, enabled, woff_id, created_at, updated_at
        FROM sso_provider_configs
        ORDER BY provider
        "#,
    )
    .fetch_all(&mut *conn)
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
) -> Result<Json<SsoConfigResponse>, StatusCode> {
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
            Some(encrypt_secret(secret, &key).map_err(|e| {
                tracing::error!("Failed to encrypt client_secret: {e}");
                StatusCode::INTERNAL_SERVER_ERROR
            })?)
        }
    } else {
        None
    };

    let enabled = body.enabled.unwrap_or(true);

    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &auth_user.tenant_id.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let config = if let Some(encrypted) = encrypted_secret {
        sqlx::query_as::<_, SsoConfigResponse>(
            r#"
            INSERT INTO sso_provider_configs (tenant_id, provider, client_id, client_secret_encrypted, external_org_id, woff_id, enabled)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (tenant_id, provider) DO UPDATE SET
                client_id = EXCLUDED.client_id,
                client_secret_encrypted = EXCLUDED.client_secret_encrypted,
                external_org_id = EXCLUDED.external_org_id,
                woff_id = EXCLUDED.woff_id,
                enabled = EXCLUDED.enabled,
                updated_at = NOW()
            RETURNING provider, client_id, external_org_id, enabled, woff_id, created_at, updated_at
            "#,
        )
        .bind(auth_user.tenant_id)
        .bind(&body.provider)
        .bind(&body.client_id)
        .bind(&encrypted)
        .bind(&body.external_org_id)
        .bind(&body.woff_id)
        .bind(enabled)
        .fetch_one(&mut *conn)
        .await
    } else {
        sqlx::query_as::<_, SsoConfigResponse>(
            r#"
            INSERT INTO sso_provider_configs (tenant_id, provider, client_id, external_org_id, woff_id, enabled)
            VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT (tenant_id, provider) DO UPDATE SET
                client_id = EXCLUDED.client_id,
                external_org_id = EXCLUDED.external_org_id,
                woff_id = EXCLUDED.woff_id,
                enabled = EXCLUDED.enabled,
                updated_at = NOW()
            RETURNING provider, client_id, external_org_id, enabled, woff_id, created_at, updated_at
            "#,
        )
        .bind(auth_user.tenant_id)
        .bind(&body.provider)
        .bind(&body.client_id)
        .bind(&body.external_org_id)
        .bind(&body.woff_id)
        .bind(enabled)
        .fetch_one(&mut *conn)
        .await
    };

    config.map(Json).map_err(|e| {
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

    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &auth_user.tenant_id.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    sqlx::query("DELETE FROM sso_provider_configs WHERE tenant_id = $1 AND provider = $2")
        .bind(auth_user.tenant_id)
        .bind(&body.provider)
        .execute(&mut *conn)
        .await
        .map_err(|e| {
            tracing::error!("Failed to delete SSO config: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(StatusCode::NO_CONTENT)
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
