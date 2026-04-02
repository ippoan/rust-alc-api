use async_trait::async_trait;
use chrono::Utc;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::sync::Arc;
use uuid::Uuid;

use crate::models::WebhookConfig;
use crate::repository::WebhookRepository;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::TenkoSchedule;
    use std::sync::Mutex;

    // --- Mock Repository ---

    struct MockRepo {
        config: Option<WebhookConfig>,
        deliveries: Mutex<Vec<(String, i32, bool)>>,
        overdue_configs: Vec<WebhookConfig>,
        overdue_schedules: Vec<TenkoSchedule>,
        employee_name: Option<String>,
        notified: Mutex<Vec<Uuid>>,
    }

    impl MockRepo {
        fn new() -> Self {
            Self {
                config: None,
                deliveries: Mutex::new(Vec::new()),
                overdue_configs: Vec::new(),
                overdue_schedules: Vec::new(),
                employee_name: None,
                notified: Mutex::new(Vec::new()),
            }
        }

        fn with_config(mut self, config: WebhookConfig) -> Self {
            self.config = Some(config);
            self
        }

        fn with_overdue(
            mut self,
            configs: Vec<WebhookConfig>,
            schedules: Vec<TenkoSchedule>,
        ) -> Self {
            self.overdue_configs = configs;
            self.overdue_schedules = schedules;
            self
        }

        fn with_employee_name(mut self, name: Option<String>) -> Self {
            self.employee_name = name;
            self
        }
    }

    #[async_trait]
    impl WebhookRepository for MockRepo {
        async fn find_config(
            &self,
            _tenant_id: Uuid,
            _event_type: &str,
        ) -> Result<Option<WebhookConfig>, sqlx::Error> {
            Ok(self.config.clone())
        }

        async fn record_delivery(
            &self,
            _tenant_id: Uuid,
            _config_id: Uuid,
            event_type: &str,
            _payload: &serde_json::Value,
            _status_code: Option<i32>,
            _response_body: Option<&str>,
            attempt: i32,
            success: bool,
        ) -> Result<(), sqlx::Error> {
            self.deliveries
                .lock()
                .unwrap()
                .push((event_type.to_string(), attempt, success));
            Ok(())
        }

        async fn find_overdue_configs(&self) -> Result<Vec<WebhookConfig>, sqlx::Error> {
            Ok(self.overdue_configs.clone())
        }

        async fn find_overdue_schedules(
            &self,
            _tenant_id: Uuid,
            _overdue_minutes: i64,
        ) -> Result<Vec<TenkoSchedule>, sqlx::Error> {
            Ok(self.overdue_schedules.clone())
        }

        async fn get_employee_name(
            &self,
            _employee_id: Uuid,
        ) -> Result<Option<String>, sqlx::Error> {
            Ok(self.employee_name.clone())
        }

        async fn mark_overdue_notified(&self, schedule_id: Uuid) -> Result<(), sqlx::Error> {
            self.notified.lock().unwrap().push(schedule_id);
            Ok(())
        }
    }

    // --- Mock HTTP Client ---

    struct MockHttp {
        responses: Mutex<Vec<(Option<i32>, Option<String>, bool)>>,
    }

    impl MockHttp {
        fn success() -> Self {
            Self {
                responses: Mutex::new(vec![(Some(200), Some("ok".to_string()), true)]),
            }
        }

        fn fail_then_succeed() -> Self {
            Self {
                responses: Mutex::new(vec![
                    (Some(500), Some("error".to_string()), false),
                    (Some(200), Some("ok".to_string()), true),
                ]),
            }
        }

        fn always_fail() -> Self {
            Self {
                responses: Mutex::new(vec![
                    (Some(500), Some("err1".to_string()), false),
                    (Some(500), Some("err2".to_string()), false),
                    (Some(500), Some("err3".to_string()), false),
                ]),
            }
        }
    }

    #[async_trait]
    impl WebhookHttpClient for MockHttp {
        async fn deliver(
            &self,
            _url: &str,
            _event_type: &str,
            _payload: &serde_json::Value,
            _secret: Option<&str>,
        ) -> Result<(Option<i32>, Option<String>, bool), anyhow::Error> {
            let resp = self.responses.lock().unwrap().remove(0);
            Ok(resp)
        }
    }

    // --- Helper ---

    fn make_config(secret: Option<&str>) -> WebhookConfig {
        WebhookConfig {
            id: Uuid::new_v4(),
            tenant_id: Uuid::new_v4(),
            event_type: "test_event".to_string(),
            url: "https://example.com/webhook".to_string(),
            secret: secret.map(|s| s.to_string()),
            enabled: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn make_schedule(tenant_id: Uuid) -> TenkoSchedule {
        TenkoSchedule {
            id: Uuid::new_v4(),
            tenant_id,
            employee_id: Uuid::new_v4(),
            tenko_type: "pre_operation".to_string(),
            responsible_manager_name: "Manager A".to_string(),
            scheduled_at: Utc::now() - chrono::Duration::hours(2),
            instruction: None,
            consumed: false,
            consumed_by_session_id: None,
            overdue_notified_at: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    // --- Tests ---

    #[tokio::test(start_paused = true)]
    async fn test_fire_event_impl_no_config() {
        let repo = MockRepo::new();
        let http = MockHttp::success();
        let tenant_id = Uuid::new_v4();

        let result = fire_event_impl(&repo, &http, tenant_id, "test", serde_json::json!({})).await;

        assert!(result.is_ok());
        assert!(repo.deliveries.lock().unwrap().is_empty());
    }

    #[tokio::test(start_paused = true)]
    async fn test_fire_event_impl_with_config() {
        let config = make_config(None);
        let repo = MockRepo::new().with_config(config);
        let http = MockHttp::success();
        let tenant_id = Uuid::new_v4();

        let result = fire_event_impl(&repo, &http, tenant_id, "test", serde_json::json!({})).await;

        assert!(result.is_ok());
        let deliveries = repo.deliveries.lock().unwrap();
        assert_eq!(deliveries.len(), 1);
        assert_eq!(deliveries[0].1, 1);
        assert!(deliveries[0].2);
    }

    #[tokio::test(start_paused = true)]
    async fn test_deliver_webhook_success_first_attempt() {
        let config = make_config(None);
        let repo = MockRepo::new();
        let http = MockHttp::success();

        let result =
            deliver_webhook(&repo, &http, &config, "test_event", &serde_json::json!({})).await;

        assert!(result.is_ok());
        let deliveries = repo.deliveries.lock().unwrap();
        assert_eq!(deliveries.len(), 1);
        assert!(deliveries[0].2);
    }

    #[tokio::test(start_paused = true)]
    async fn test_deliver_webhook_retry_then_success() {
        let config = make_config(None);
        let repo = MockRepo::new();
        let http = MockHttp::fail_then_succeed();

        let result =
            deliver_webhook(&repo, &http, &config, "test_event", &serde_json::json!({})).await;

        assert!(result.is_ok());
        let deliveries = repo.deliveries.lock().unwrap();
        assert_eq!(deliveries.len(), 2);
        assert!(!deliveries[0].2);
        assert!(deliveries[1].2);
    }

    #[tokio::test(start_paused = true)]
    async fn test_deliver_webhook_all_retries_fail() {
        let config = make_config(None);
        let repo = MockRepo::new();
        let http = MockHttp::always_fail();

        let result =
            deliver_webhook(&repo, &http, &config, "test_event", &serde_json::json!({})).await;

        assert!(result.is_ok());
        let deliveries = repo.deliveries.lock().unwrap();
        assert_eq!(deliveries.len(), 3);
        assert!(!deliveries[0].2);
        assert!(!deliveries[1].2);
        assert!(!deliveries[2].2);
    }

    #[tokio::test(start_paused = true)]
    async fn test_deliver_webhook_with_secret() {
        let config = make_config(Some("my-secret-key"));
        let repo = MockRepo::new();
        let http = MockHttp::success();

        let result = deliver_webhook(
            &repo,
            &http,
            &config,
            "test_event",
            &serde_json::json!({"foo": "bar"}),
        )
        .await;

        assert!(result.is_ok());
        let deliveries = repo.deliveries.lock().unwrap();
        assert_eq!(deliveries.len(), 1);
        assert!(deliveries[0].2);
    }

    #[tokio::test(start_paused = true)]
    async fn test_check_overdue_no_configs() {
        let repo = MockRepo::new();
        let http = MockHttp::success();

        let result = check_overdue_schedules(&repo, &http).await;

        assert!(result.is_ok());
        assert!(repo.notified.lock().unwrap().is_empty());
    }

    #[tokio::test(start_paused = true)]
    async fn test_check_overdue_with_schedules() {
        let config = make_config(None);
        let tenant_id = config.tenant_id;
        let schedule = make_schedule(tenant_id);
        let schedule_id = schedule.id;

        let repo = MockRepo::new()
            .with_overdue(vec![config], vec![schedule])
            .with_employee_name(Some("Taro Yamada".to_string()));
        let http = MockHttp::success();

        let result = check_overdue_schedules(&repo, &http).await;

        assert!(result.is_ok());
        let notified = repo.notified.lock().unwrap();
        assert_eq!(notified.len(), 1);
        assert_eq!(notified[0], schedule_id);
        let deliveries = repo.deliveries.lock().unwrap();
        assert_eq!(deliveries.len(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn test_check_overdue_employee_name_none() {
        let config = make_config(None);
        let tenant_id = config.tenant_id;
        let schedule = make_schedule(tenant_id);

        let repo = MockRepo::new()
            .with_overdue(vec![config], vec![schedule])
            .with_employee_name(None);
        let http = MockHttp::success();

        let result = check_overdue_schedules(&repo, &http).await;

        assert!(result.is_ok());
        assert_eq!(repo.notified.lock().unwrap().len(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn test_pg_webhook_service_new_and_fire_event() {
        let config = make_config(None);
        let repo = Arc::new(MockRepo::new().with_config(config));
        let http = Arc::new(MockHttp::success());

        let service = PgWebhookService::new(repo.clone(), http);

        service
            .fire_event(Uuid::new_v4(), "test", serde_json::json!({}))
            .await;

        let deliveries = repo.deliveries.lock().unwrap();
        assert_eq!(deliveries.len(), 1);
    }

    #[tokio::test]
    async fn test_reqwest_webhook_client_success() {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .respond_with(wiremock::ResponseTemplate::new(200).set_body_string("ok"))
            .mount(&server)
            .await;

        let client = ReqwestWebhookClient;
        let (status, body, success) = client
            .deliver(
                &server.uri(),
                "test_event",
                &serde_json::json!({"key": "value"}),
                None,
            )
            .await
            .unwrap();

        assert_eq!(status, Some(200));
        assert_eq!(body.as_deref(), Some("ok"));
        assert!(success);
    }

    #[tokio::test]
    async fn test_reqwest_webhook_client_with_secret() {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::header_exists("X-Webhook-Signature"))
            .respond_with(wiremock::ResponseTemplate::new(200))
            .mount(&server)
            .await;

        let client = ReqwestWebhookClient;
        let (status, _, success) = client
            .deliver(
                &server.uri(),
                "test_event",
                &serde_json::json!({}),
                Some("my-secret"),
            )
            .await
            .unwrap();

        assert_eq!(status, Some(200));
        assert!(success);
    }

    #[tokio::test]
    async fn test_reqwest_webhook_client_server_error() {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .respond_with(wiremock::ResponseTemplate::new(500).set_body_string("error"))
            .mount(&server)
            .await;

        let client = ReqwestWebhookClient;
        let (status, _, success) = client
            .deliver(&server.uri(), "test", &serde_json::json!({}), None)
            .await
            .unwrap();

        assert_eq!(status, Some(500));
        assert!(!success);
    }

    #[tokio::test]
    async fn test_reqwest_webhook_client_connection_error() {
        let client = ReqwestWebhookClient;
        let (status, body, success) = client
            .deliver("http://127.0.0.1:1", "test", &serde_json::json!({}), None)
            .await
            .unwrap();

        assert!(status.is_none());
        assert!(body.is_some());
        assert!(!success);
    }
}
