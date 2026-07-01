/// Bot Config 管理 REST API
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{delete, get, post},
    Extension, Json, Router,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::json;
use ts_rs::TS;
use uuid::Uuid;

use alc_core::auth_lineworks::normalize_pem_newlines;
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
    /// LINE WORKS Bot webhook 署名検証用 (X-WORKS-Signature HMAC key)
    bot_secret: Option<String>,
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
        .route("/admin/bot/configs/export", get(export_configs))
        .route("/admin/bot/configs/{id}/secrets", get(get_config_secrets))
}

// ---------------------------------------------------------------------------
// Developer-only export (Bot Config dump compatible with /api/staging/import)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct ExportQuery {
    tenant_id: Uuid,
}

/// `DEVELOPER_EMAILS` env var (comma-separated) のいずれかと一致するか判定。
/// 大文字小文字無視 / 空エントリ無視。env が未設定なら常に false (export 不可)。
pub(crate) fn is_developer_email(email: &str) -> bool {
    let allowlist = std::env::var("DEVELOPER_EMAILS").unwrap_or_default();
    allowlist
        .split(',')
        .map(str::trim)
        .any(|entry| !entry.is_empty() && entry.eq_ignore_ascii_case(email))
}

async fn export_configs(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthUser>,
    Query(params): Query<ExportQuery>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    if !is_developer_email(&auth_user.email) {
        tracing::warn!("export refused: {}", auth_user.email);
        return Err(StatusCode::FORBIDDEN);
    }

    let tid = params.tenant_id;

    let tenant = state
        .bot_admin
        .get_tenant_for_export(tid)
        .await
        .map_err(|e| {
            tracing::error!("Failed to fetch tenant for export: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    let bot_configs = state
        .bot_admin
        .list_configs_for_export(tid)
        .await
        .map_err(|e| {
            tracing::error!("Failed to fetch bot_configs for export: {e}");
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
            "sso_provider_configs": [],
            "tenko_call_numbers": [],
            "tenko_call_drivers": [],
            "bot_configs": bot_configs,
            "notify_line_configs": [],
            "notify_recipients": []
        }
    })))
}

#[cfg(test)]
mod tests {
    use super::is_developer_email;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn with_env<F: FnOnce()>(value: Option<&str>, body: F) {
        let _g = ENV_LOCK.lock().unwrap();
        let prev = std::env::var("DEVELOPER_EMAILS").ok();
        match value {
            Some(v) => std::env::set_var("DEVELOPER_EMAILS", v),
            None => std::env::remove_var("DEVELOPER_EMAILS"),
        }
        body();
        match prev {
            Some(v) => std::env::set_var("DEVELOPER_EMAILS", v),
            None => std::env::remove_var("DEVELOPER_EMAILS"),
        }
    }

    #[test]
    fn rejects_when_env_unset() {
        with_env(None, || {
            assert!(!is_developer_email("m.tama.ramu@gmail.com"));
        });
    }

    #[test]
    fn rejects_when_env_empty() {
        with_env(Some(""), || {
            assert!(!is_developer_email("m.tama.ramu@gmail.com"));
        });
    }

    #[test]
    fn accepts_single_match_case_insensitive() {
        with_env(Some("m.tama.ramu@gmail.com"), || {
            assert!(is_developer_email("m.tama.ramu@gmail.com"));
            assert!(is_developer_email("M.Tama.Ramu@Gmail.com"));
            assert!(!is_developer_email("attacker@example.com"));
        });
    }

    #[test]
    fn accepts_csv_with_whitespace() {
        with_env(
            Some(" foo@example.com , m.tama.ramu@gmail.com ,bar@x"),
            || {
                assert!(is_developer_email("foo@example.com"));
                assert!(is_developer_email("m.tama.ramu@gmail.com"));
                assert!(is_developer_email("bar@x"));
                assert!(!is_developer_email("baz@x"));
            },
        );
    }

    #[test]
    fn ignores_empty_csv_entries() {
        with_env(Some(",,,m.tama.ramu@gmail.com,,"), || {
            assert!(is_developer_email("m.tama.ramu@gmail.com"));
            assert!(!is_developer_email(""));
        });
    }

    #[test]
    fn restores_previous_value_from_some() {
        // Pre-set DEVELOPER_EMAILS so with_env's `prev` is Some(_) and the
        // restore path takes the `Some(v) => set_var(...)` branch.
        let _g = ENV_LOCK.lock().unwrap();
        std::env::set_var("DEVELOPER_EMAILS", "preset@example.com");
        drop(_g);

        with_env(Some("temp@example.com"), || {
            assert!(is_developer_email("temp@example.com"));
        });

        let _g = ENV_LOCK.lock().unwrap();
        assert_eq!(
            std::env::var("DEVELOPER_EMAILS").ok().as_deref(),
            Some("preset@example.com")
        );
        std::env::remove_var("DEVELOPER_EMAILS");
    }
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

    let key = std::env::var("SSO_ENCRYPTION_KEY").map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

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
        if let Some(bs) = body.bot_secret.as_deref().filter(|s| !s.is_empty()) {
            let encrypted = encrypt_secret(bs, &key).expect("AES-256-GCM encrypt infallible");
            state
                .bot_admin
                .update_bot_secret(tenant_id, id, &encrypted)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
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

        let created = state
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
            .await;

        if let Ok(ref row) = created {
            if let Some(bs) = body.bot_secret.as_deref().filter(|s| !s.is_empty()) {
                let encrypted = encrypt_secret(bs, &key).expect("AES-256-GCM encrypt infallible");
                state
                    .bot_admin
                    .update_bot_secret(tenant_id, row.id, &encrypted)
                    .await
                    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            }
        }
        created
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

#[derive(Debug, Serialize, TS)]
#[ts(export)]
struct BotConfigSecretsResponse {
    client_id: String,
    client_secret: String,
    service_account: String,
    private_key: String,
    bot_id: String,
    /// LINE WORKS Bot webhook 署名検証用 (未設定なら空文字)
    bot_secret: String,
}

async fn get_config_secrets(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<BotConfigSecretsResponse>, StatusCode> {
    if auth_user.role != "admin" {
        return Err(StatusCode::FORBIDDEN);
    }

    let key = std::env::var("SSO_ENCRYPTION_KEY").map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let row = state
        .bot_admin
        .get_config_with_secrets(auth_user.tenant_id, id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get bot config secrets: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    let client_secret = decrypt_secret(&row.client_secret_encrypted, &key).map_err(|e| {
        tracing::error!("Failed to decrypt client_secret: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let private_key = decrypt_secret(&row.private_key_encrypted, &key)
        .map(normalize_pem_newlines)
        .map_err(|e| {
            tracing::error!("Failed to decrypt private_key: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let bot_secret = match row.bot_secret_encrypted {
        Some(ref enc) => decrypt_secret(enc, &key).map_err(|e| {
            tracing::error!("Failed to decrypt bot_secret: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?,
        None => String::new(),
    };

    Ok(Json(BotConfigSecretsResponse {
        client_id: row.client_id,
        client_secret,
        service_account: row.service_account,
        private_key,
        bot_id: row.bot_id,
        bot_secret,
    }))
}

fn decrypt_secret(ciphertext_b64: &str, key_material: &str) -> Result<String, String> {
    use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
    use ring::aead::{Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM};
    use sha2::{Digest, Sha256};

    let data = BASE64
        .decode(ciphertext_b64)
        .map_err(|e| format!("Base64 decode error: {e}"))?;
    if data.len() < 12 {
        return Err("Ciphertext too short".to_string());
    }

    let mut key_bytes = [0u8; 32];
    let hash = Sha256::digest(key_material.as_bytes());
    key_bytes.copy_from_slice(&hash);

    let unbound_key =
        UnboundKey::new(&AES_256_GCM, &key_bytes).map_err(|e| format!("Key error: {e}"))?;
    let key = LessSafeKey::new(unbound_key);

    let (nonce_bytes, ciphertext) = data.split_at(12);
    let nonce =
        Nonce::try_assume_unique_for_key(nonce_bytes).map_err(|e| format!("Nonce error: {e}"))?;

    let mut in_out = ciphertext.to_vec();
    let plaintext = key
        .open_in_place(nonce, Aad::empty(), &mut in_out)
        .map_err(|e| format!("Decryption error: {e}"))?;

    String::from_utf8(plaintext.to_vec()).map_err(|e| format!("UTF-8 error: {e}"))
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
