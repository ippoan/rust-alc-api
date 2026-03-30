use async_trait::async_trait;
use chrono::Utc;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::sync::Arc;
use uuid::Uuid;

use crate::db::models::WebhookConfig;
use crate::db::repository::WebhookRepository;

type HmacSha256 = Hmac<Sha256>;

/// Webhook サービス trait — テスト時に mock 差し替え可能
#[async_trait]
pub trait WebhookService: Send + Sync {
    async fn fire_event(&self, tenant_id: Uuid, event_type: &str, payload: serde_json::Value);
}

/// HTTP 配信 trait — テスト時に mock 差し替え可能
#[async_trait]
pub trait WebhookHttpClient: Send + Sync {
    /// Webhook を配信し、(status_code, response_body, success) を返す
    async fn deliver(
        &self,
        url: &str,
        event_type: &str,
        payload: &serde_json::Value,
        secret: Option<&str>,
    ) -> Result<(Option<i32>, Option<String>, bool), anyhow::Error>;
}

/// 本番用 HTTP クライアント (reqwest)
pub struct ReqwestWebhookClient;

#[async_trait]
impl WebhookHttpClient for ReqwestWebhookClient {
    async fn deliver(
        &self,
        url: &str,
        event_type: &str,
        payload: &serde_json::Value,
        secret: Option<&str>,
    ) -> Result<(Option<i32>, Option<String>, bool), anyhow::Error> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()?;

        let body = serde_json::to_string(payload)?;

        let mut req = client
            .post(url)
            .header("Content-Type", "application/json")
            .header("X-Webhook-Event", event_type);

        if let Some(secret) = secret {
            let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC key length");
            mac.update(body.as_bytes());
            let signature = hex::encode(mac.finalize().into_bytes());
            req = req.header("X-Webhook-Signature", format!("sha256={signature}"));
        }

        let resp = req.body(body).send().await;

        match resp {
            Ok(r) => {
                let code = r.status().as_u16() as i32;
                let body = r.text().await.unwrap_or_default();
                let ok = (200..300).contains(&(code as u16 as usize));
                Ok((Some(code), Some(body), ok))
            }
            Err(e) => {
                tracing::warn!("Webhook delivery failed: {e}");
                Ok((None, Some(e.to_string()), false))
            }
        }
    }
}

/// 本番用 WebhookService (Repository + HTTP)
pub struct PgWebhookService {
    repo: Arc<dyn WebhookRepository>,
    http: Arc<dyn WebhookHttpClient>,
}

impl PgWebhookService {
    pub fn new(repo: Arc<dyn WebhookRepository>, http: Arc<dyn WebhookHttpClient>) -> Self {
        Self { repo, http }
    }
}

#[async_trait]
impl WebhookService for PgWebhookService {
    async fn fire_event(&self, tenant_id: Uuid, event_type: &str, payload: serde_json::Value) {
        let _ = fire_event_impl(&*self.repo, &*self.http, tenant_id, event_type, payload).await;
    }
}

/// Webhook イベントを発火 (非同期で配信)
pub async fn fire_event_impl(
    repo: &dyn WebhookRepository,
    http: &dyn WebhookHttpClient,
    tenant_id: Uuid,
    event_type: &str,
    payload: serde_json::Value,
) -> Result<(), anyhow::Error> {
    let config = repo.find_config(tenant_id, event_type).await?;

    let config = match config {
        Some(c) => c,
        None => return Ok(()), // 設定なし → 何もしない
    };

    deliver_webhook(repo, http, &config, event_type, &payload).await?;

    Ok(())
}

/// Webhook を配信 (リトライ付き)
pub async fn deliver_webhook(
    repo: &dyn WebhookRepository,
    http: &dyn WebhookHttpClient,
    config: &WebhookConfig,
    event_type: &str,
    payload: &serde_json::Value,
) -> Result<(), anyhow::Error> {
    let delays = [1u64, 5, 25]; // 指数バックオフ

    for attempt in 1..=3 {
        let (status_code, response_body, success) = http
            .deliver(&config.url, event_type, payload, config.secret.as_deref())
            .await?;

        // 配信ログ記録
        let _ = repo
            .record_delivery(
                config.tenant_id,
                config.id,
                event_type,
                payload,
                status_code,
                response_body.as_deref(),
                attempt,
                success,
            )
            .await;

        if success {
            return Ok(());
        }

        if attempt < 3 {
            tokio::time::sleep(std::time::Duration::from_secs(delays[attempt as usize - 1])).await;
        }
    }

    Ok(())
}

/// 未完了予定の検出 + overdue通知 (バックグラウンドループから呼ばれる)
pub async fn check_overdue_schedules(
    repo: &dyn WebhookRepository,
    http: &dyn WebhookHttpClient,
) -> Result<(), anyhow::Error> {
    let overdue_minutes: i64 = std::env::var("TENKO_OVERDUE_MINUTES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(60);

    let configs = repo.find_overdue_configs().await?;

    for config in &configs {
        let overdue_schedules = repo
            .find_overdue_schedules(config.tenant_id, overdue_minutes)
            .await?;

        for schedule in &overdue_schedules {
            let employee_name = repo.get_employee_name(schedule.employee_id).await?;

            let minutes = (Utc::now() - schedule.scheduled_at).num_minutes();

            let payload = serde_json::json!({
                "event": "tenko_overdue",
                "timestamp": Utc::now(),
                "tenant_id": config.tenant_id,
                "data": {
                    "schedule_id": schedule.id,
                    "employee_id": schedule.employee_id,
                    "employee_name": employee_name.unwrap_or_default(),
                    "scheduled_at": schedule.scheduled_at,
                    "minutes_overdue": minutes,
                    "responsible_manager_name": schedule.responsible_manager_name,
                    "tenko_type": schedule.tenko_type,
                }
            });

            repo.mark_overdue_notified(schedule.id).await?;

            let _ = deliver_webhook(repo, http, config, "tenko_overdue", &payload).await;
        }
    }

    Ok(())
}
