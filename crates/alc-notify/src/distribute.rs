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
use crate::viewer_register::{build_register_body, ViewerRegisterClient};

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

/// 受信者にメッセージを送信。
///
/// `image_url` が `Some` の場合、テキスト送信成功後に同 URL を image メッセージとしても
/// 送信する (テキスト → 画像の順)。画像送信が失敗してもテキストは届いているので関数全体
/// は成功扱い (warning ログのみ) — 画像はあくまで補助。
async fn send_to_recipient(
    state: &AppState,
    tenant_id: Uuid,
    recipient: &alc_core::repository::notify_recipients::NotifyRecipient,
    message: &str,
    image_url: Option<&str>,
    line_client: &LineClient,
    lw_client: &LineworksBotClient,
) -> Result<(), String> {
    match recipient.provider.as_str() {
        "line" => {
            let user_id = recipient.line_user_id.as_deref().ok_or("No line_user_id")?;
            let cfg = resolve_line_config(state, tenant_id).await?;
            // 1. テキスト先行
            line_client
                .push_text(&cfg, user_id, message)
                .await
                .map_err(|e| e.to_string())?;
            // 2. 画像 (extracted_data あり時のみ。失敗は warn のみで成功扱い)
            if let Some(url) = image_url {
                if let Err(e) = line_client.push_image(&cfg, user_id, url, url).await {
                    tracing::warn!(
                        "LINE push_image failed for recipient {} (text was sent OK): {e}",
                        recipient.name
                    );
                }
            }
            Ok(())
        }
        "lineworks" => {
            let user_id = recipient
                .lineworks_user_id
                .as_deref()
                .ok_or("No lineworks_user_id")?;
            let (config_id, cfg) = resolve_lineworks_config(state, tenant_id).await?;
            // 1. テキスト先行
            lw_client
                .send_text_to_user(config_id, &cfg, user_id, message)
                .await
                .map_err(|e| e.to_string())?;
            // 2. 画像
            if let Some(url) = image_url {
                if let Err(e) = lw_client
                    .send_image_to_user(config_id, &cfg, user_id, url, url)
                    .await
                {
                    tracing::warn!(
                        "LINE WORKS send_image_to_user failed for recipient {} (text was sent OK): {e}",
                        recipient.name
                    );
                }
            }
            Ok(())
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

    let line_client = LineClient::new();
    let lw_client = LineworksBotClient::new();

    // viewer Worker (nuxt-notify) の KV へ view:{token} を登録する client (Refs #434)。
    // env 未設定なら None (= 非破壊)。viewer が配信する r2_key は redacted 優先
    // (= rust viewer の COALESCE(redacted_r2_key, r2_key) と一致させる)。
    let viewer_register = ViewerRegisterClient::from_env();
    let view_r2_key = doc
        .redacted_r2_key
        .clone()
        .unwrap_or_else(|| doc.r2_key.clone());
    let view_expire = chrono::Utc::now() + chrono::Duration::days(retention_days as i64);

    // image メッセージは PDF + extracted_data.logistics がある時のみ送る
    // (PDF 以外、または配車手配票でない PDF は画像送信しない)
    let send_image = should_send_image(&doc);

    let mut sent = 0;
    let mut failed = 0;

    for (delivery, recipient) in deliveries.iter().zip(recipients.iter()) {
        // KV 登録は best-effort。失敗しても配信は止めない (旧 viewer / 再配信が fallback)。
        if let Some(rc) = viewer_register.as_ref() {
            let body = build_register_body(
                delivery.read_token,
                &view_r2_key,
                document_id,
                delivery.recipient_id,
                doc.file_name.as_deref(),
                doc.file_size_bytes,
                doc.source_subject.as_deref(),
                doc.source_sender.as_deref(),
                Some(doc.created_at),
                view_expire,
            );
            if let Err(e) = rc.register(&body).await {
                tracing::warn!("register-view: {e}");
            }
        }

        let read_url = format!("{}/api/notify/read/{}", api_origin, delivery.read_token);
        // 画像を併送する場合は本文の「▶ 詳細: {url}」行を省く (画像 inline で見えるので冗長)。
        // 画像なし (テキストのみ) の場合は従来通り URL を入れる。
        let message_url: Option<&str> = if send_image { None } else { Some(&read_url) };
        let message = build_distribute_message(&doc, message_url);
        // 注: URL を `.jpg` 終わりにするのは必須。LINE Messaging API / LINE WORKS
        // Bot は image message の `originalContentUrl` を **拡張子で** 画像判定
        // するため、`/image` (拡張子なし) だと URL がテキストリンクとして表示される。
        let image_url = if send_image {
            Some(format!(
                "{}/api/notify/v/{}/image.jpg",
                api_origin, delivery.read_token
            ))
        } else {
            None
        };

        match send_to_recipient(
            &state,
            tenant_id,
            recipient,
            &message,
            image_url.as_deref(),
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
            None, // テスト配信は画像なし (テキストのみ)
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

/// LINE / LINE WORKS に送信する本文を組み立てる。
///
/// `doc.extracted_data.logistics` (12 フィールドのいずれかが非空) があれば物流テンプレに
/// 切り替え、なければ既存テンプレ (`title` + `summary` + URL) を返す。
///
/// 物流テンプレ:
/// ```text
/// 📄 {title}
/// 📍 積地: {loading_place}
///    {loading_place_address}
///    ☎ {loading_place_phone}
/// 📦 卸地: {unloading_place}
///    {unloading_place_address}
///    ☎ {unloading_place_phone}
/// 🕐 積込: {loading_at}
/// 🕓 卸し: {unloading_at}
/// ⚠️ 注意: {notes}
/// 🏢 連絡先: {contact_company}
/// 👤 担当: {contact_person}
/// 📞 電話: {contact_phone}
///
/// ▶ 詳細: {url}
/// ```
/// 各フィールドのうち存在するものだけ列挙する (一部 null OK)。場所の住所と電話は
/// 全角スペース 1 個 + 空白 2 個で indent して、どの場所に紐付くか視覚的に明確にする。
/// 連絡先 3 フィールドは `extract.rs` 側で「相手先」を抽出済みで自社情報は除外されている。
/// `read_url` が `None` のときは「▶ 詳細: ...」行を省く。
/// 画像メッセージを併送する場合 (LINE/LINE WORKS が画像を inline 展開する) は
/// 詳細リンクが冗長になるので、呼び出し側で None を渡す。
pub(crate) fn build_distribute_message(
    doc: &alc_core::repository::notify_documents::NotifyDocument,
    read_url: Option<&str>,
) -> String {
    let title = doc
        .extracted_title
        .as_deref()
        .unwrap_or(doc.file_name.as_deref().unwrap_or("ドキュメント"));

    if let Some(logistics) = doc
        .extracted_data
        .as_ref()
        .and_then(|d| d.get("logistics"))
        .filter(|v| v.is_object())
    {
        let mut lines: Vec<String> = Vec::with_capacity(15);
        lines.push(format!("📄 {}", title));

        let get_str = |key: &str| -> Option<&str> {
            logistics
                .get(key)
                .and_then(|v| v.as_str())
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
        };

        let push_field = |lines: &mut Vec<String>, prefix: &str, key: &str| {
            if let Some(v) = get_str(key) {
                lines.push(format!("{} {}", prefix, v));
            }
        };

        // 場所セクション (住所/担当/電話は indent して place の下にぶら下げる)
        let push_place = |lines: &mut Vec<String>,
                          prefix: &str,
                          place_key: &str,
                          address_key: &str,
                          phone_key: &str,
                          person_key: &str| {
            if let Some(v) = get_str(place_key) {
                lines.push(format!("{} {}", prefix, v));
                if let Some(addr) = get_str(address_key) {
                    lines.push(format!("　 {}", addr));
                }
                if let Some(person) = get_str(person_key) {
                    lines.push(format!("　 👤 {}", person));
                }
                if let Some(phone) = get_str(phone_key) {
                    lines.push(format!("　 ☎ {}", phone));
                }
            } else {
                // place 自体が空でも address/phone/person が来ていたら捨てずに親なしで列挙
                // (defensive、ほぼ起きないが念のため)
                if let Some(addr) = get_str(address_key) {
                    lines.push(format!("{} {}", prefix, addr));
                }
                if let Some(person) = get_str(person_key) {
                    lines.push(format!("　 👤 {}", person));
                }
                if let Some(phone) = get_str(phone_key) {
                    lines.push(format!("　 ☎ {}", phone));
                }
            }
        };

        // 配車情報
        push_place(
            &mut lines,
            "📍 積地:",
            "loading_place",
            "loading_place_address",
            "loading_place_phone",
            "loading_place_person",
        );
        push_place(
            &mut lines,
            "📦 卸地:",
            "unloading_place",
            "unloading_place_address",
            "unloading_place_phone",
            "unloading_place_person",
        );
        push_field(&mut lines, "🕐 積込:", "loading_at");
        push_field(&mut lines, "🕓 卸し:", "unloading_at");
        push_field(&mut lines, "⚠️ 注意:", "notes");
        // 相手先連絡先セクション (相手先のみ、自社は extract 側で除外済)
        push_field(&mut lines, "🏢 連絡先:", "contact_company");
        push_field(&mut lines, "👤 担当:", "contact_person");
        push_field(&mut lines, "📞 電話:", "contact_phone");

        // logistics キー自体は object だが全 string が空文字 / 欠落のとき (defensive: schema
        // 違反値の混入対策)、本文が「📄 タイトル」だけになって URL が浮くのを避けるため
        // 既存テンプレに fallback する。
        if lines.len() == 1 {
            return fallback_template(doc, read_url);
        }

        // read_url=Some のときだけ「▶ 詳細: …」を追加。画像メッセージを併送する
        // 場合 (image inline 展開される) は読者が直接見えるので URL は冗長。
        if let Some(url) = read_url {
            lines.push(String::new()); // 詳細リンク前の空行
            lines.push(format!("▶ 詳細: {}", url));
        }
        lines.join("\n")
    } else {
        fallback_template(doc, read_url)
    }
}

/// 画像メッセージ (LINE / LINE WORKS の inline 表示) を送るべきかを判定する。
///
/// 配車手配票 PDF のように `extracted_data.logistics` が抽出済みかつ PDF 拡張子の
/// 場合のみ true。テキスト PDF や画像なし PDF は false (extract_first_page_jpeg で
/// 415 を返すので、無駄打ちを避ける)。
///
/// pure 関数。filename と extracted_data だけで判定。
pub(crate) fn should_send_image(
    doc: &alc_core::repository::notify_documents::NotifyDocument,
) -> bool {
    let is_pdf = doc
        .file_name
        .as_deref()
        .map(|n| n.to_lowercase().ends_with(".pdf"))
        .unwrap_or(false);
    if !is_pdf {
        return false;
    }
    let has_logistics = doc
        .extracted_data
        .as_ref()
        .and_then(|d| d.get("logistics"))
        .filter(|v| v.is_object())
        .is_some();
    has_logistics
}

fn fallback_template(
    doc: &alc_core::repository::notify_documents::NotifyDocument,
    read_url: Option<&str>,
) -> String {
    let summary = doc
        .extracted_summary
        .as_deref()
        .unwrap_or("新しいドキュメントが届きました");
    let title = doc
        .extracted_title
        .as_deref()
        .unwrap_or(doc.file_name.as_deref().unwrap_or("ドキュメント"));
    match read_url {
        Some(url) => format!("📄 {}\n\n{}\n\n▶ 詳細を見る: {}", title, summary, url),
        None => format!("📄 {}\n\n{}", title, summary),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alc_core::repository::notify_documents::NotifyDocument;

    fn build_doc() -> NotifyDocument {
        NotifyDocument {
            id: Uuid::new_v4(),
            tenant_id: Uuid::new_v4(),
            source_type: "manual".into(),
            source_sender: None,
            source_subject: None,
            r2_key: "k".into(),
            file_name: Some("haisou.pdf".into()),
            file_size_bytes: None,
            extracted_title: None,
            extracted_date: None,
            extracted_summary: None,
            extracted_phone_numbers: None,
            extracted_data: None,
            extraction_status: "pending".into(),
            extraction_error: None,
            distribution_status: "pending".into(),
            distributed_at: None,
            redacted_r2_key: None,
            redacted_at: None,
            redactions_applied: None,
            redaction_status: "completed".into(),
            redaction_error: None,
            redact_dl_ms: None,
            redact_llm_ms: None,
            redact_render_ms: None,
            redact_upload_ms: None,
            redact_total_ms: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn message_uses_logistics_template_when_all_fields_present() {
        let mut doc = build_doc();
        doc.extracted_data = Some(serde_json::json!({
            "logistics": {
                "loading_place": "東京都港区",
                "loading_place_address": null,
                "loading_place_phone": null,
                "loading_place_person": null,
                "unloading_place": "大阪府大阪市",
                "unloading_place_address": null,
                "unloading_place_phone": null,
                "unloading_place_person": null,
                "loading_at": "5/9 10:00",
                "unloading_at": "5/10 14:00",
                "notes": "冷凍便\n要時間厳守",
                "contact_company": "ABC運送株式会社",
                "contact_person": "田中太郎",
                "contact_phone": "03-1234-5678"
            }
        }));
        let msg = build_distribute_message(&doc, Some("https://x/v/abc"));
        assert!(msg.contains("📄 haisou.pdf"));
        assert!(msg.contains("📍 積地: 東京都港区"));
        assert!(msg.contains("📦 卸地: 大阪府大阪市"));
        assert!(msg.contains("🕐 積込: 5/9 10:00"));
        assert!(msg.contains("🕓 卸し: 5/10 14:00"));
        assert!(msg.contains("⚠️ 注意: 冷凍便\n要時間厳守"));
        assert!(msg.contains("🏢 連絡先: ABC運送株式会社"));
        assert!(msg.contains("👤 担当: 田中太郎"));
        assert!(msg.contains("📞 電話: 03-1234-5678"));
        assert!(msg.contains("▶ 詳細: https://x/v/abc"));
        // 「新しいドキュメントが届きました」の既定句は出ない
        assert!(!msg.contains("新しいドキュメント"));
    }

    #[test]
    fn message_indents_place_address_phone_and_person_under_place() {
        // 積地: 会社名 + 住所 + 担当 + 電話、卸地: 住所のみ
        let mut doc = build_doc();
        doc.extracted_data = Some(serde_json::json!({
            "logistics": {
                "loading_place": "イオン関西RDC",
                "loading_place_address": "京都府乙訓郡大山崎町字大山崎小字鏡田38",
                "loading_place_phone": "075-959-5008",
                "loading_place_person": "佐藤",
                "unloading_place": "熊本県八代市新港町3-9-8",
                "loading_at": "令和8年4月17日 (金) 19時"
            }
        }));
        let msg = build_distribute_message(&doc, Some("https://x/v/abc"));
        // 積地: 会社名 → 住所 → 👤 担当 → ☎ 電話の順 (indent あり)
        let i_place = msg.find("📍 積地: イオン関西RDC").unwrap();
        let i_addr = msg.find("　 京都府乙訓郡").unwrap();
        let i_person = msg.find("　 👤 佐藤").unwrap();
        let i_phone = msg.find("　 ☎ 075-959-5008").unwrap();
        let i_unload = msg.find("📦 卸地: 熊本県八代市").unwrap();
        let i_loadat = msg.find("🕐 積込:").unwrap();
        assert!(i_place < i_addr);
        assert!(i_addr < i_person);
        assert!(i_person < i_phone);
        assert!(i_phone < i_unload);
        assert!(i_unload < i_loadat);
        // 卸地は住所のみ → ☎ が 1 個、👤 も 1 個 (積地のみ)
        assert_eq!(msg.matches('☎').count(), 1);
        assert_eq!(msg.matches("👤 ").count(), 1);
    }

    #[test]
    fn message_omits_null_contact_fields() {
        // 連絡先 3 フィールドのうち 1 つだけ。配車情報なし。配信本文は contact 1 行 + URL のみ
        let mut doc = build_doc();
        doc.extracted_data = Some(serde_json::json!({
            "logistics": {
                "contact_phone": "06-9876-5432"
            }
        }));
        let msg = build_distribute_message(&doc, Some("https://x/v/abc"));
        assert!(msg.contains("📞 電話: 06-9876-5432"));
        assert!(!msg.contains("🏢 連絡先"));
        assert!(!msg.contains("👤 担当"));
        assert!(!msg.contains("📍 積地"));
        assert!(msg.contains("▶ 詳細: https://x/v/abc"));
    }

    #[test]
    fn message_includes_contact_in_correct_order_after_logistics() {
        // 物流 + 連絡先のフィールド順序を確認: notes の後に contact_company が来る
        let mut doc = build_doc();
        doc.extracted_data = Some(serde_json::json!({
            "logistics": {
                "loading_place": "東京",
                "notes": "冷凍",
                "contact_company": "ABC",
                "contact_phone": "03-1234"
            }
        }));
        let msg = build_distribute_message(&doc, Some("https://x/v/abc"));
        let i_notes = msg.find("⚠️ 注意").unwrap();
        let i_contact = msg.find("🏢 連絡先").unwrap();
        let i_phone = msg.find("📞 電話").unwrap();
        let i_url = msg.find("▶ 詳細").unwrap();
        assert!(i_notes < i_contact);
        assert!(i_contact < i_phone);
        assert!(i_phone < i_url);
    }

    #[test]
    fn message_omits_null_fields_in_logistics_template() {
        let mut doc = build_doc();
        doc.extracted_data = Some(serde_json::json!({
            "logistics": {
                "loading_place": "成田",
                "unloading_place": null,
                "loading_at": null,
                "unloading_at": null,
                "notes": null
            }
        }));
        let msg = build_distribute_message(&doc, Some("https://x/v/abc"));
        assert!(msg.contains("📍 積地: 成田"));
        assert!(!msg.contains("📦 卸地"));
        assert!(!msg.contains("🕐 積込"));
        assert!(msg.contains("▶ 詳細: https://x/v/abc"));
    }

    #[test]
    fn message_falls_back_to_legacy_when_no_logistics_key() {
        let mut doc = build_doc();
        doc.extracted_title = Some("見積書".into());
        doc.extracted_summary = Some("〇〇社からの見積です".into());
        doc.extracted_data = Some(serde_json::json!({"phone_numbers_ext": ["090-..."]}));
        let msg = build_distribute_message(&doc, Some("https://x/v/abc"));
        assert_eq!(
            msg,
            "📄 見積書\n\n〇〇社からの見積です\n\n▶ 詳細を見る: https://x/v/abc"
        );
    }

    #[test]
    fn message_falls_back_when_extracted_data_is_none() {
        let doc = build_doc();
        let msg = build_distribute_message(&doc, Some("https://x/v/abc"));
        assert!(msg.contains("📄 haisou.pdf"));
        assert!(msg.contains("新しいドキュメントが届きました"));
        assert!(msg.contains("▶ 詳細を見る: https://x/v/abc"));
    }

    #[test]
    fn message_falls_back_when_logistics_object_has_only_empty_strings() {
        // defensive: schema 違反値が混入しても既存テンプレに退避
        let mut doc = build_doc();
        doc.extracted_data = Some(serde_json::json!({
            "logistics": {
                "loading_place": "  ",
                "unloading_place": "",
                "loading_at": null,
                "unloading_at": null,
                "notes": null
            }
        }));
        let msg = build_distribute_message(&doc, Some("https://x/v/abc"));
        // logistics テンプレ部分の絵文字は出ない
        assert!(!msg.contains("📍 積地"));
        assert!(msg.contains("新しいドキュメントが届きました"));
    }

    #[test]
    fn message_falls_back_when_logistics_value_is_not_object() {
        // defensive: extracted_data.logistics が string や array なら無視
        let mut doc = build_doc();
        doc.extracted_data = Some(serde_json::json!({"logistics": "broken"}));
        let msg = build_distribute_message(&doc, Some("https://x/v/abc"));
        assert!(msg.contains("新しいドキュメントが届きました"));
    }

    #[test]
    fn message_uses_extracted_title_when_available_in_logistics_path() {
        let mut doc = build_doc();
        doc.extracted_title = Some("配車手配票".into());
        doc.extracted_data = Some(serde_json::json!({
            "logistics": {"loading_place": "東京"}
        }));
        let msg = build_distribute_message(&doc, Some("https://x/v/abc"));
        assert!(msg.starts_with("📄 配車手配票"));
        assert!(!msg.contains("haisou.pdf"));
    }

    #[test]
    fn message_uses_doc_default_when_no_filename() {
        let mut doc = build_doc();
        doc.file_name = None;
        let msg = build_distribute_message(&doc, Some("https://x/v/abc"));
        assert!(msg.contains("📄 ドキュメント"));
    }
}
