//! 配信オーケストレーター
//! ドキュメントを全受信者に配信する

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Extension, Json, Router,
};
use uuid::Uuid;

use alc_core::auth_lineworks::decrypt_secret;
use alc_core::auth_middleware::TenantId;
use alc_core::AppState;

use crate::clients::line::{LineClient, LineConfig};
use crate::clients::lineworks::{LineworksBotClient, LineworksBotConfig};

pub fn tenant_router() -> Router<AppState> {
    Router::new()
        .route(
            "/notify/documents/{id}/distribute",
            axum::routing::post(distribute),
        )
        .route(
            "/notify/test-distribute",
            axum::routing::post(test_distribute),
        )
}

fn encryption_key() -> Result<String, StatusCode> {
    std::env::var("SSO_ENCRYPTION_KEY")
        .or_else(|_| std::env::var("JWT_SECRET"))
        .map_err(|_| {
            tracing::error!("SSO_ENCRYPTION_KEY or JWT_SECRET not set");
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

async fn resolve_line_config(state: &AppState, tenant_id: Uuid) -> Result<LineConfig, String> {
    let config = state
        .notify_line_config
        .get_full(tenant_id)
        .await
        .map_err(|e| format!("DB error: {e}"))?
        .ok_or_else(|| "LINE config not found".to_string())?;

    let key = encryption_key().map_err(|_| "Encryption key not set".to_string())?;
    let access_token = decrypt_secret(&config.channel_access_token_encrypted, &key)
        .map_err(|e| format!("decrypt access_token: {e}"))?;
    let secret = decrypt_secret(&config.channel_secret_encrypted, &key)
        .map_err(|e| format!("decrypt channel_secret: {e}"))?;

    Ok(LineConfig {
        channel_access_token: access_token,
        channel_secret: secret,
    })
}

async fn resolve_lineworks_config(
    state: &AppState,
    tenant_id: Uuid,
) -> Result<(Uuid, LineworksBotConfig), String> {
    let configs = state
        .bot_admin
        .list_configs(tenant_id)
        .await
        .map_err(|e| format!("DB error: {e}"))?;

    let bot_cfg = configs
        .iter()
        .find(|c| c.provider == "lineworks" && c.enabled)
        .ok_or_else(|| "No LINE WORKS bot config".to_string())?;

    let full = state
        .bot_admin
        .get_config_with_secrets(tenant_id, bot_cfg.id)
        .await
        .map_err(|e| format!("DB error: {e}"))?
        .ok_or_else(|| "Bot config not found".to_string())?;

    let key = encryption_key().map_err(|_| "Encryption key not set".to_string())?;
    let client_secret = decrypt_secret(&full.client_secret_encrypted, &key)
        .map_err(|e| format!("decrypt client_secret: {e}"))?;
    let private_key = decrypt_secret(&full.private_key_encrypted, &key)
        .map_err(|e| format!("decrypt private_key: {e}"))?;

    Ok((
        full.id,
        LineworksBotConfig {
            client_id: full.client_id,
            client_secret,
            service_account: full.service_account,
            private_key,
            bot_id: full.bot_id,
        },
    ))
}

/// 受信者にメッセージを送信
async fn send_to_recipient(
    state: &AppState,
    tenant_id: Uuid,
    recipient: &alc_core::repository::notify_recipients::NotifyRecipient,
    message: &str,
    line_client: &LineClient,
    lw_client: &LineworksBotClient,
) -> Result<(), String> {
    match recipient.provider.as_str() {
        "line" => {
            let user_id = recipient.line_user_id.as_deref().ok_or("No line_user_id")?;
            let cfg = resolve_line_config(state, tenant_id).await?;
            line_client
                .push_text(&cfg, user_id, message)
                .await
                .map_err(|e| e.to_string())
        }
        "lineworks" => {
            let user_id = recipient
                .lineworks_user_id
                .as_deref()
                .ok_or("No lineworks_user_id")?;
            let (config_id, cfg) = resolve_lineworks_config(state, tenant_id).await?;
            lw_client
                .send_text_to_user(config_id, &cfg, user_id, message)
                .await
                .map_err(|e| e.to_string())
        }
        other => Err(format!("Unknown provider: {other}")),
    }
}

/// ドキュメントを全受信者に配信
async fn distribute(
    State(state): State<AppState>,
    Extension(tenant): Extension<TenantId>,
    Path(document_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let tenant_id = tenant.0;

    let doc = state
        .notify_documents
        .get(tenant_id, document_id)
        .await
        .map_err(|e| {
            tracing::error!("get document: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    let recipients = state
        .notify_recipients
        .list_enabled(tenant_id)
        .await
        .map_err(|e| {
            tracing::error!("list recipients: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    if recipients.is_empty() {
        return Ok(Json(
            serde_json::json!({"message": "No enabled recipients"}),
        ));
    }

    let _ = state
        .notify_documents
        .update_distribution_status(tenant_id, document_id, "in_progress")
        .await;

    let recipient_pairs: Vec<(Uuid, String)> = recipients
        .iter()
        .map(|r| (r.id, r.provider.clone()))
        .collect();

    let deliveries = state
        .notify_deliveries
        .create_batch(tenant_id, document_id, &recipient_pairs)
        .await
        .map_err(|e| {
            tracing::error!("create deliveries: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let api_origin =
        std::env::var("API_ORIGIN").unwrap_or_else(|_| "https://localhost:8080".into());
    let summary = doc
        .extracted_summary
        .as_deref()
        .unwrap_or("新しいドキュメントが届きました");
    let title = doc
        .extracted_title
        .as_deref()
        .unwrap_or(doc.file_name.as_deref().unwrap_or("ドキュメント"));

    let line_client = LineClient::new();
    let lw_client = LineworksBotClient::new();

    let mut sent = 0;
    let mut failed = 0;

    for (delivery, recipient) in deliveries.iter().zip(recipients.iter()) {
        let read_url = format!("{}/api/notify/read/{}", api_origin, delivery.read_token);
        let message = format!("📄 {}\n\n{}\n\n▶ 詳細を見る: {}", title, summary, read_url);

        match send_to_recipient(
            &state,
            tenant_id,
            recipient,
            &message,
            &line_client,
            &lw_client,
        )
        .await
        {
            Ok(()) => {
                let _ = state
                    .notify_deliveries
                    .mark_sent(tenant_id, delivery.id)
                    .await;
                sent += 1;
            }
            Err(e) => {
                tracing::error!("deliver to {}: {e}", recipient.name);
                let _ = state
                    .notify_deliveries
                    .update_status(tenant_id, delivery.id, "failed", Some(&e))
                    .await;
                failed += 1;
            }
        }
    }

    let status = "completed";
    let _ = state
        .notify_documents
        .update_distribution_status(tenant_id, document_id, status)
        .await;

    Ok(Json(serde_json::json!({
        "sent": sent,
        "failed": failed,
        "total": sent + failed,
    })))
}

/// テスト配信 — 指定テキストを全受信者に送信
#[derive(serde::Deserialize)]
struct TestDistributeRequest {
    message: String,
}

async fn test_distribute(
    State(state): State<AppState>,
    Extension(tenant): Extension<TenantId>,
    Json(input): Json<TestDistributeRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let tenant_id = tenant.0;

    let recipients = state
        .notify_recipients
        .list_enabled(tenant_id)
        .await
        .map_err(|e| {
            tracing::error!("list recipients: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let line_client = LineClient::new();
    let lw_client = LineworksBotClient::new();

    let mut sent = 0;
    let mut failed = 0;

    for recipient in &recipients {
        match send_to_recipient(
            &state,
            tenant_id,
            recipient,
            &input.message,
            &line_client,
            &lw_client,
        )
        .await
        {
            Ok(()) => sent += 1,
            Err(e) => {
                tracing::error!("test deliver to {}: {e}", recipient.name);
                failed += 1;
            }
        }
    }

    Ok(Json(serde_json::json!({
        "sent": sent,
        "failed": failed,
        "total": sent + failed,
    })))
}
