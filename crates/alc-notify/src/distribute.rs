//! 配信オーケストレーター
//! ドキュメントを全受信者に配信する

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Extension, Json, Router,
};
use uuid::Uuid;

use alc_core::auth_lineworks::{decrypt_pem_secret, decrypt_secret};
use alc_core::auth_middleware::TenantId;
use alc_core::middleware::AuthUser;
use alc_core::tenant::TenantConn;
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
    let channel_secret = decrypt_secret(&config.channel_secret_encrypted, &key)
        .map_err(|e| format!("decrypt channel_secret: {e}"))?;
    let key_id = config
        .key_id
        .ok_or_else(|| "LINE config missing key_id".to_string())?;
    let private_key_enc = config
        .private_key_encrypted
        .ok_or_else(|| "LINE config missing private_key".to_string())?;
    let private_key = decrypt_pem_secret(&private_key_enc, &key)
        .map_err(|e| format!("decrypt private_key: {e}"))?;

    Ok(LineConfig {
        channel_id: config.channel_id,
        channel_secret,
        key_id,
        private_key,
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
    let private_key = decrypt_pem_secret(&full.private_key_encrypted, &key)
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
#[derive(serde::Deserialize, Default)]
pub struct DistributeTarget {
    #[serde(default)]
    pub all: bool,
    pub group_id: Option<Uuid>,
    #[serde(default)]
    pub recipient_ids: Vec<Uuid>,
}

#[derive(serde::Deserialize, Default)]
pub struct DistributeRequest {
    pub target: Option<DistributeTarget>,
    /// 配信から何日後に閲覧期限切れにするか (デフォルト 7 日)。
    /// 指定範囲: 1〜90 (R2 presigned URL 仕様の最大 7 日 (= 604800 秒) を超えても、
    /// read_tracker が都度 1 時間 presign を発行するので問題なく動く)
    pub retention_days: Option<i64>,
}

async fn resolve_target_recipients(
    state: &AppState,
    tenant_id: Uuid,
    target: &DistributeTarget,
) -> Result<Vec<alc_core::repository::notify_recipients::NotifyRecipient>, StatusCode> {
    if let Some(group_id) = target.group_id {
        return state
            .notify_groups
            .list_enabled_members(tenant_id, group_id)
            .await
            .map_err(|e| {
                tracing::error!("list group members: {e}");
                StatusCode::INTERNAL_SERVER_ERROR
            });
    }
    if !target.recipient_ids.is_empty() {
        let all = state
            .notify_recipients
            .list_enabled(tenant_id)
            .await
            .map_err(|e| {
                tracing::error!("list recipients: {e}");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
        let want: std::collections::HashSet<Uuid> = target.recipient_ids.iter().copied().collect();
        return Ok(all.into_iter().filter(|r| want.contains(&r.id)).collect());
    }
    // default: all enabled
    state
        .notify_recipients
        .list_enabled(tenant_id)
        .await
        .map_err(|e| {
            tracing::error!("list recipients: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

async fn distribute(
    State(state): State<AppState>,
    Extension(tenant): Extension<TenantId>,
    auth_user: Option<Extension<AuthUser>>,
    Path(document_id): Path<Uuid>,
    body: Option<Json<DistributeRequest>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let tenant_id = tenant.0;
    let triggered_by = auth_user.map(|Extension(u)| u.user_id);

    let doc = state
        .notify_documents
        .get(tenant_id, document_id)
        .await
        .map_err(|e| {
            tracing::error!("get document: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    // 配信ブロック (migration 109): redact 完了 (`completed`) または PDF 以外で
    // skip 済 (`skipped`) のみ許可。`pending` / `processing` / `failed` は弾く。
    // 誤って原本 PDF (金額入り) を送信しないための安全装置。
    if !matches!(doc.redaction_status.as_str(), "completed" | "skipped") {
        tracing::warn!(
            "distribute: redaction not done, status={} doc={document_id}",
            doc.redaction_status
        );
        return Err(StatusCode::BAD_REQUEST);
    }

    let req = body.map(|b| b.0).unwrap_or_default();
    let target = req.target.unwrap_or(DistributeTarget {
        all: true,
        group_id: None,
        recipient_ids: Vec::new(),
    });
    // retention_days: クライアントから来なければ 7 日、来た値は 1〜90 日にクランプ
    let retention_days: i32 = req.retention_days.unwrap_or(7).clamp(1, 90) as i32;
    let recipients = resolve_target_recipients(&state, tenant_id, &target).await?;

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

    // triggered_by_user_id (best-effort) と expire_at の調整 (default 7d 以外を指定された場合)
    if !deliveries.is_empty() {
        let ids: Vec<Uuid> = deliveries.iter().map(|d| d.id).collect();
        match TenantConn::acquire(state.pool(), &tenant_id.to_string()).await {
            Ok(mut tc) => {
                if let Some(user_id) = triggered_by {
                    if let Err(e) = sqlx::query(
                        "UPDATE notify_deliveries SET triggered_by_user_id = $1 WHERE id = ANY($2)",
                    )
                    .bind(user_id)
                    .bind(&ids)
                    .execute(&mut *tc.conn)
                    .await
                    {
                        tracing::warn!("set triggered_by_user_id: {e}");
                    }
                }
                // 7 日以外なら expire_at を上書き (default は migration で NOW() + 7 days)
                if retention_days != 7 {
                    if let Err(e) = sqlx::query(
                        "UPDATE notify_deliveries SET expire_at = NOW() + make_interval(days => $1) WHERE id = ANY($2)",
                    )
                    .bind(retention_days)
                    .bind(&ids)
                    .execute(&mut *tc.conn)
                    .await
                    {
                        tracing::warn!("set expire_at: {e}");
                    }
                }
            }
            Err(e) => tracing::warn!("acquire conn for delivery update: {e}"),
        }
    }

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

/// テスト配信 — 指定された受信者にテキストを送信
#[derive(serde::Deserialize)]
struct TestDistributeRequest {
    message: String,
    recipient_ids: Vec<Uuid>,
}

async fn test_distribute(
    State(state): State<AppState>,
    Extension(tenant): Extension<TenantId>,
    Json(input): Json<TestDistributeRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let tenant_id = tenant.0;

    if input.recipient_ids.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let enabled = state
        .notify_recipients
        .list_enabled(tenant_id)
        .await
        .map_err(|e| {
            tracing::error!("list recipients: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let selected: Vec<_> = enabled
        .into_iter()
        .filter(|r| input.recipient_ids.contains(&r.id))
        .collect();

    let line_client = LineClient::new();
    let lw_client = LineworksBotClient::new();

    let mut sent = 0;
    let mut failed = 0;

    for recipient in &selected {
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
