/// Bot Config 管理 REST API
use axum::{
    extract::State,
    http::StatusCode,
    routing::{delete, get, post},
    Extension, Json, Router,
};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use alc_core::auth_middleware::AuthUser;
use alc_core::repository::bot_admin::BotConfigRow;
use alc_core::AppState;

#[derive(Debug, Serialize, TS)]
#[ts(export)]
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

impl From<BotConfigRow> for BotConfigResponse {
    fn from(row: BotConfigRow) -> Self {
        Self {
            id: row.id,
            provider: row.provider,
            name: row.name,
            client_id: row.client_id,
            service_account: row.service_account,
            bot_id: row.bot_id,
            enabled: row.enabled,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
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

    let rows = state
        .bot_admin
        .list_configs(auth_user.tenant_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to list bot configs: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let configs = rows.into_iter().map(BotConfigResponse::from).collect();
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

    let tenant_id = auth_user.tenant_id;

    let row = if let Some(ref id_str) = body.id {
        // 更新
        let id = Uuid::parse_str(id_str).map_err(|_| StatusCode::BAD_REQUEST)?;

        if let Some(ref secret) = body.client_secret {
            if !secret.is_empty() {
                let encrypted =
                    encrypt_secret(secret, &key).expect("AES-256-GCM encrypt infallible");
                state
                    .bot_admin
                    .update_client_secret(tenant_id, id, &encrypted)
                    .await
                    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            }
        }
        if let Some(ref pk) = body.private_key {
            if !pk.is_empty() {
                let encrypted = encrypt_secret(pk, &key).expect("AES-256-GCM encrypt infallible");
                state
                    .bot_admin
                    .update_private_key(tenant_id, id, &encrypted)
                    .await
                    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            }
        }

        state
            .bot_admin
            .update_config(
                tenant_id,
                id,
                provider,
                &body.name,
                &body.client_id,
                &body.service_account,
                &body.bot_id,
                enabled,
            )
            .await
    } else {
        // 新規作成
        let encrypted_secret = encrypt_secret(body.client_secret.as_deref().unwrap_or(""), &key)
            .expect("AES-256-GCM encrypt infallible");
        let encrypted_pk = encrypt_secret(body.private_key.as_deref().unwrap_or(""), &key)
            .expect("AES-256-GCM encrypt infallible");

        state
            .bot_admin
            .create_config(
                tenant_id,
                provider,
                &body.name,
                &body.client_id,
                &encrypted_secret,
                &body.service_account,
                &encrypted_pk,
                &body.bot_id,
                enabled,
            )
            .await
    };

    row.map(BotConfigResponse::from).map(Json).map_err(|e| {
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

    state
        .bot_admin
        .delete_config(auth_user.tenant_id, id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to delete bot config: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(StatusCode::NO_CONTENT)
}

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
