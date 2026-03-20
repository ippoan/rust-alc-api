/// Bot Config 管理 REST API
use axum::{
    extract::State,
    http::StatusCode,
    Extension, Json,
    routing::{get, post, delete},
    Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::AppState;
use crate::db::tenant::set_current_tenant;
use crate::middleware::auth::AuthUser;

#[derive(Debug, Serialize, sqlx::FromRow)]
struct BotConfigResponse {
    id: Uuid,
    provider: String,
    name: String,
    client_id: String,
    service_account: String,
    bot_id: String,
    enabled: bool,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize)]
struct ListResponse {
    configs: Vec<BotConfigResponse>,
}

#[derive(Debug, Deserialize)]
struct UpsertRequest {
    id: Option<String>,
    provider: Option<String>,
    name: String,
    client_id: String,
    client_secret: Option<String>,
    service_account: String,
    private_key: Option<String>,
    bot_id: String,
    enabled: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct DeleteRequest {
    id: String,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/admin/bot/configs", get(list_configs))
        .route("/admin/bot/configs", post(upsert_config))
        .route("/admin/bot/configs", delete(delete_config))
}

async fn list_configs(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthUser>,
) -> Result<Json<ListResponse>, StatusCode> {
    if auth_user.role != "admin" {
        return Err(StatusCode::FORBIDDEN);
    }

    let mut conn = state.pool.acquire().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &auth_user.tenant_id.to_string()).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let configs = sqlx::query_as::<_, BotConfigResponse>(
        r#"
        SELECT id, provider, name, client_id, service_account, bot_id, enabled, created_at, updated_at
        FROM bot_configs
        ORDER BY name
        "#,
    )
    .fetch_all(&mut *conn)
    .await
    .map_err(|e| {
        tracing::error!("Failed to list bot configs: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(ListResponse { configs }))
}

async fn upsert_config(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthUser>,
    Json(body): Json<UpsertRequest>,
) -> Result<Json<BotConfigResponse>, StatusCode> {
    if auth_user.role != "admin" {
        return Err(StatusCode::FORBIDDEN);
    }

    let key = std::env::var("SSO_ENCRYPTION_KEY")
        .or_else(|_| std::env::var("JWT_SECRET"))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let provider = body.provider.as_deref().unwrap_or("lineworks");
    let enabled = body.enabled.unwrap_or(true);

    let mut conn = state.pool.acquire().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &auth_user.tenant_id.to_string()).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let config = if let Some(ref id_str) = body.id {
        // 更新
        let id = Uuid::parse_str(id_str).map_err(|_| StatusCode::BAD_REQUEST)?;

        if let Some(ref secret) = body.client_secret {
            if !secret.is_empty() {
                let encrypted = encrypt_secret(secret, &key).map_err(|e| {
                    tracing::error!("Encrypt client_secret failed: {e}");
                    StatusCode::INTERNAL_SERVER_ERROR
                })?;
                sqlx::query("UPDATE bot_configs SET client_secret_encrypted = $1 WHERE id = $2")
                    .bind(&encrypted).bind(id).execute(&mut *conn).await
                    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            }
        }
        if let Some(ref pk) = body.private_key {
            if !pk.is_empty() {
                let encrypted = encrypt_secret(pk, &key).map_err(|e| {
                    tracing::error!("Encrypt private_key failed: {e}");
                    StatusCode::INTERNAL_SERVER_ERROR
                })?;
                sqlx::query("UPDATE bot_configs SET private_key_encrypted = $1 WHERE id = $2")
                    .bind(&encrypted).bind(id).execute(&mut *conn).await
                    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            }
        }

        sqlx::query_as::<_, BotConfigResponse>(
            r#"
            UPDATE bot_configs SET
                provider = $1, name = $2, client_id = $3, service_account = $4,
                bot_id = $5, enabled = $6, updated_at = NOW()
            WHERE id = $7
            RETURNING id, provider, name, client_id, service_account, bot_id, enabled, created_at, updated_at
            "#,
        )
        .bind(provider).bind(&body.name).bind(&body.client_id)
        .bind(&body.service_account).bind(&body.bot_id).bind(enabled).bind(id)
        .fetch_one(&mut *conn)
        .await
    } else {
        // 新規作成
        let encrypted_secret = encrypt_secret(
            body.client_secret.as_deref().unwrap_or(""), &key,
        ).map_err(|e| {
            tracing::error!("Encrypt client_secret failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
        let encrypted_pk = encrypt_secret(
            body.private_key.as_deref().unwrap_or(""), &key,
        ).map_err(|e| {
            tracing::error!("Encrypt private_key failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

        sqlx::query_as::<_, BotConfigResponse>(
            r#"
            INSERT INTO bot_configs (tenant_id, provider, name, client_id, client_secret_encrypted, service_account, private_key_encrypted, bot_id, enabled)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            RETURNING id, provider, name, client_id, service_account, bot_id, enabled, created_at, updated_at
            "#,
        )
        .bind(auth_user.tenant_id).bind(provider).bind(&body.name)
        .bind(&body.client_id).bind(&encrypted_secret).bind(&body.service_account)
        .bind(&encrypted_pk).bind(&body.bot_id).bind(enabled)
        .fetch_one(&mut *conn)
        .await
    };

    config.map(Json).map_err(|e| {
        tracing::error!("Failed to upsert bot config: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })
}

async fn delete_config(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthUser>,
    Json(body): Json<DeleteRequest>,
) -> Result<StatusCode, StatusCode> {
    if auth_user.role != "admin" {
        return Err(StatusCode::FORBIDDEN);
    }

    let id = Uuid::parse_str(&body.id).map_err(|_| StatusCode::BAD_REQUEST)?;

    let mut conn = state.pool.acquire().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &auth_user.tenant_id.to_string()).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    sqlx::query("DELETE FROM bot_configs WHERE id = $1")
        .bind(id)
        .execute(&mut *conn)
        .await
        .map_err(|e| {
            tracing::error!("Failed to delete bot config: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(StatusCode::NO_CONTENT)
}

fn encrypt_secret(plaintext: &str, key_material: &str) -> Result<String, String> {
    use ring::aead::{self, Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM};
    use ring::rand::{SecureRandom, SystemRandom};
    use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
    use sha2::{Sha256, Digest};

    let mut key_bytes = [0u8; 32];
    let hash = Sha256::digest(key_material.as_bytes());
    key_bytes.copy_from_slice(&hash);

    let unbound_key = UnboundKey::new(&AES_256_GCM, &key_bytes).map_err(|e| format!("Key error: {e}"))?;
    let key = LessSafeKey::new(unbound_key);

    let rng = SystemRandom::new();
    let mut nonce_bytes = [0u8; 12];
    rng.fill(&mut nonce_bytes).map_err(|e| format!("RNG error: {e}"))?;
    let nonce = Nonce::assume_unique_for_key(nonce_bytes);

    let mut in_out = plaintext.as_bytes().to_vec();
    let tag_len = aead::AES_256_GCM.tag_len();
    in_out.extend(vec![0u8; tag_len]);

    key.seal_in_place_separate_tag(nonce, Aad::empty(), &mut in_out[..plaintext.len()])
        .map(|tag| { in_out[plaintext.len()..].copy_from_slice(tag.as_ref()); })
        .map_err(|e| format!("Encryption error: {e}"))?;

    let mut result = Vec::with_capacity(12 + in_out.len());
    result.extend_from_slice(&nonce_bytes);
    result.extend_from_slice(&in_out);

    Ok(BASE64.encode(&result))
}
