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
            // redirect 無効化 (302 → 内部リソースへの SSRF バイパス防止)。Refs #390。
            // URL の allowlist 検証は書き込み時 (config 作成/更新) に validate_webhook_url
            // で実施する。
            .redirect(reqwest::redirect::Policy::none())
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

/// Webhook 配信先 URL が SSRF 的に危険でないか検証する。Refs #390。
///
/// テナント管理者が任意 URL を登録でき、サーバがそこへ POST してレスポンスを
/// 保存するため、内部サービス / クラウドメタデータ (169.254.169.254 等) への
/// 到達を防ぐ。書き込み時と配信時の両方で呼ぶ。
///
/// ルール:
/// - `https` のみ許可 (内部 http サービス / メタデータ叩きを排除)
/// - userinfo 付き URL は拒否
/// - host が IP リテラルなら loopback / private / link-local / unspecified を拒否
/// - host 名が localhost / `*.internal` / `*.local` / メタデータ FQDN なら拒否
///
/// NOTE: ホスト名→IP の DNS 解決時チェック (DNS rebinding 完全対策) は未実装。
/// 完全な保護には配信時に解決済み IP を検証する custom resolver が要る (follow-up)。
pub fn validate_webhook_url(raw: &str) -> bool {
    let url = match url::Url::parse(raw) {
        Ok(u) => u,
        Err(_) => return false,
    };
    // host を scheme 判定より先に取る (host を持たない scheme = data:/javascript:
    // 等を None 経路で拒否しつつ、その経路を到達可能に保つ)。
    let host = match url.host_str() {
        Some(h) => h.trim_end_matches('.').to_ascii_lowercase(),
        None => return false,
    };
    if url.scheme() != "https" {
        return false;
    }
    if !url.username().is_empty() || url.password().is_some() {
        return false;
    }
    // IPv6 は host_str がブラケット付き ("[::1]") で返るため外してから IP 判定。
    let ip_candidate = host.trim_start_matches('[').trim_end_matches(']');
    if let Ok(ip) = ip_candidate.parse::<std::net::IpAddr>() {
        return is_global_ip(ip);
    }
    // ホスト名ベースの明示ブロック
    !(host == "localhost"
        || host.ends_with(".localhost")
        || host.ends_with(".internal")
        || host.ends_with(".local")
        || host == "metadata.google.internal")
}

/// IP が外部到達可能 (loopback/private/link-local/unspecified でない) か。
fn is_global_ip(ip: std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(v4) => {
            !(v4.is_private()
                || v4.is_loopback()
                || v4.is_link_local()
                || v4.is_unspecified()
                || v4.is_broadcast()
                || v4.is_documentation()
                // CGNAT 100.64.0.0/10
                || (v4.octets()[0] == 100 && (v4.octets()[1] & 0xc0) == 0x40))
        }
        std::net::IpAddr::V6(v6) => {
            !(v6.is_loopback()
                || v6.is_unspecified()
                // unique local fc00::/7
                || (v6.segments()[0] & 0xfe00) == 0xfc00
                // link-local fe80::/10
                || (v6.segments()[0] & 0xffc0) == 0xfe80)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn webhook_url_allows_public_https() {
        assert!(validate_webhook_url("https://hooks.example.com/path"));
        assert!(validate_webhook_url("https://example.co.jp/")); // 末尾ドット無し
        assert!(validate_webhook_url("https://example.com./")); // 末尾ドットは除去
        assert!(validate_webhook_url("https://8.8.8.8/cb")); // public IP literal
        assert!(validate_webhook_url("https://[2001:4860:4860::8888]/")); // public v6
    }

    #[test]
    fn webhook_url_rejects_non_https_and_userinfo() {
        assert!(!validate_webhook_url("http://example.com/")); // http 不可
        assert!(!validate_webhook_url("ftp://example.com/"));
        assert!(!validate_webhook_url("https://user@example.com/")); // userinfo
        assert!(!validate_webhook_url("https://u:p@example.com/"));
        assert!(!validate_webhook_url("not-a-url"));
        // host を持たない (host_str()==None) scheme は弾く: parse は成功するが
        // authority が無いので reject されること (line 232 の None 経路)。
        assert!(!validate_webhook_url("data:text/html,evil"));
        assert!(!validate_webhook_url("mailto:admin@ippoan.org"));
    }

    #[test]
    fn webhook_url_rejects_internal_hosts_and_ranges() {
        // クラウドメタデータ
        assert!(!validate_webhook_url(
            "https://169.254.169.254/latest/meta-data/"
        ));
        assert!(!validate_webhook_url("https://metadata.google.internal/"));
        // loopback / private / link-local / unspecified
        assert!(!validate_webhook_url("https://127.0.0.1/"));
        assert!(!validate_webhook_url("https://10.0.0.5/"));
        assert!(!validate_webhook_url("https://192.168.1.1/"));
        assert!(!validate_webhook_url("https://172.16.0.1/"));
        assert!(!validate_webhook_url("https://0.0.0.0/"));
        assert!(!validate_webhook_url("https://100.64.0.1/")); // CGNAT
        assert!(!validate_webhook_url("https://255.255.255.255/")); // broadcast
        assert!(!validate_webhook_url("https://192.0.2.1/")); // documentation
                                                              // v6 内部
        assert!(!validate_webhook_url("https://[::1]/")); // loopback
        assert!(!validate_webhook_url("https://[::]/")); // unspecified
        assert!(!validate_webhook_url("https://[fc00::1]/")); // unique local
        assert!(!validate_webhook_url("https://[fe80::1]/")); // link-local
                                                              // 名前ベース
        assert!(!validate_webhook_url("https://localhost/"));
        assert!(!validate_webhook_url("https://foo.localhost/"));
        assert!(!validate_webhook_url("https://api.internal/"));
        assert!(!validate_webhook_url("https://db.local/"));
    }
    use std::sync::Mutex;

    // --- Mock Repository ---

    struct MockRepo {
        config: Option<WebhookConfig>,
        deliveries: Mutex<Vec<(String, i32, bool)>>,
    }

    impl MockRepo {
        fn new() -> Self {
            Self {
                config: None,
                deliveries: Mutex::new(Vec::new()),
            }
        }

        fn with_config(mut self, config: WebhookConfig) -> Self {
            self.config = Some(config);
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
