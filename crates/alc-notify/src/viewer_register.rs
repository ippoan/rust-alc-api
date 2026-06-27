//! viewer Worker (nuxt-notify) の KV に `view:{token}` を登録する client (Refs #434)。
//!
//! lockdown 後の公開 viewer は Worker(KV+R2) だけで完結する。rust は配信 (distribute) 時に
//! token→{r2_key, メタ} を nuxt-notify の `/api/notify/register-view` に **best-effort** で
//! POST し KV を満たすだけ。失敗しても配信は止めない (旧 rust viewer / 再 distribute が fallback)。
//!
//! 認証は `INTERNAL_SHARED_SECRET` (全 stack 同値、`x-notify-internal-secret` header)。
//! 外部 API なのでテスト可能性のため endpoint を struct field 化 (wiremock で差し替え)。

use chrono::{DateTime, Utc};
use serde_json::json;
use uuid::Uuid;

pub struct ViewerRegisterClient {
    endpoint: String,
    secret: String,
    http: reqwest::Client,
}

impl ViewerRegisterClient {
    /// env から構築。`NOTIFY_FRONTEND_URL` + `INTERNAL_SHARED_SECRET` の両方が無ければ
    /// `None` (= register 無効、非破壊)。
    pub fn from_env() -> Option<Self> {
        let frontend = std::env::var("NOTIFY_FRONTEND_URL").ok()?;
        let secret = std::env::var("INTERNAL_SHARED_SECRET").ok()?;
        let endpoint = format!(
            "{}/api/notify/register-view",
            frontend.trim_end_matches('/')
        );
        Some(Self::with_endpoint(endpoint, secret))
    }

    pub fn with_endpoint(endpoint: String, secret: String) -> Self {
        Self {
            endpoint,
            secret,
            http: reqwest::Client::new(),
        }
    }

    /// KV に view レコードを登録。2xx 以外 / 通信失敗は `Err` (呼び出し側は warn して継続)。
    pub async fn register(&self, body: &serde_json::Value) -> Result<(), String> {
        let resp = self
            .http
            .post(&self.endpoint)
            .header("x-notify-internal-secret", &self.secret)
            .json(body)
            .send()
            .await
            .map_err(|e| format!("register-view request failed: {e}"))?;
        if resp.status().is_success() {
            Ok(())
        } else {
            Err(format!("register-view status {}", resp.status()))
        }
    }
}

/// register-view へ送る JSON body を組み立てる (pure)。
/// `tenant_id` は viewer Worker が既読を `read:{tenant_id}:{document_id}:{recipient_id}` に
/// テナント前置で記録し、管理画面 read-status をサーバ側でテナント分離するために要る
/// (KV multi-tenant の定石 = tenant prefix。Refs #434)。
#[allow(clippy::too_many_arguments)]
pub fn build_register_body(
    token: Uuid,
    tenant_id: Uuid,
    r2_key: &str,
    document_id: Uuid,
    recipient_id: Uuid,
    file_name: Option<&str>,
    file_size_bytes: Option<i64>,
    source_subject: Option<&str>,
    source_sender: Option<&str>,
    source_received_at: Option<DateTime<Utc>>,
    expire_at: DateTime<Utc>,
) -> serde_json::Value {
    json!({
        "token": token.to_string(),
        "tenant_id": tenant_id.to_string(),
        "r2_key": r2_key,
        "document_id": document_id.to_string(),
        "recipient_id": recipient_id.to_string(),
        "file_name": file_name,
        "file_size_bytes": file_size_bytes,
        "source_subject": source_subject,
        "source_sender": source_sender,
        "source_received_at": source_received_at.map(|t| t.to_rfc3339()),
        "expire_at": expire_at.to_rfc3339(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn ts(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    #[test]
    fn build_register_body_shape() {
        let token = Uuid::nil();
        let tenant = Uuid::nil();
        let doc = Uuid::nil();
        let rcp = Uuid::nil();
        let body = build_register_body(
            token,
            tenant,
            "tenant/m/file.pdf",
            doc,
            rcp,
            Some("file.pdf"),
            Some(2048),
            Some("件名"),
            Some("from@example.com"),
            Some(ts("2026-06-27T00:00:00Z")),
            ts("2026-07-04T00:00:00Z"),
        );
        assert_eq!(body["token"], token.to_string());
        assert_eq!(body["tenant_id"], tenant.to_string());
        assert_eq!(body["r2_key"], "tenant/m/file.pdf");
        assert_eq!(body["document_id"], doc.to_string());
        assert_eq!(body["recipient_id"], rcp.to_string());
        assert_eq!(body["file_name"], "file.pdf");
        assert_eq!(body["file_size_bytes"], 2048);
        assert_eq!(body["source_subject"], "件名");
        assert_eq!(body["source_sender"], "from@example.com");
        assert_eq!(body["source_received_at"], "2026-06-27T00:00:00+00:00");
        assert_eq!(body["expire_at"], "2026-07-04T00:00:00+00:00");
    }

    #[test]
    fn build_register_body_nulls() {
        let body = build_register_body(
            Uuid::nil(),
            Uuid::nil(),
            "k",
            Uuid::nil(),
            Uuid::nil(),
            None,
            None,
            None,
            None,
            None,
            ts("2026-07-04T00:00:00Z"),
        );
        assert!(body["file_name"].is_null());
        assert!(body["file_size_bytes"].is_null());
        assert!(body["source_subject"].is_null());
        assert!(body["source_sender"].is_null());
        assert!(body["source_received_at"].is_null());
    }

    #[tokio::test]
    async fn register_success_sends_secret_header() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/notify/register-view"))
            .and(header("x-notify-internal-secret", "sek"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"ok": true})))
            .expect(1)
            .mount(&server)
            .await;

        let client = ViewerRegisterClient::with_endpoint(
            format!("{}/api/notify/register-view", server.uri()),
            "sek".into(),
        );
        let body = build_register_body(
            Uuid::nil(),
            Uuid::nil(),
            "k",
            Uuid::nil(),
            Uuid::nil(),
            None,
            None,
            None,
            None,
            None,
            ts("2026-07-04T00:00:00Z"),
        );
        assert!(client.register(&body).await.is_ok());
    }

    #[tokio::test]
    async fn register_non_2xx_is_err() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;
        let client = ViewerRegisterClient::with_endpoint(
            format!("{}/api/notify/register-view", server.uri()),
            "sek".into(),
        );
        let err = client.register(&json!({})).await.unwrap_err();
        assert!(err.contains("401"));
    }

    #[tokio::test]
    async fn register_unreachable_is_err() {
        // 即座に閉じている port に向けて通信失敗を起こす
        let client = ViewerRegisterClient::with_endpoint(
            "http://127.0.0.1:1/api/notify/register-view".into(),
            "sek".into(),
        );
        let err = client.register(&json!({})).await.unwrap_err();
        assert!(err.contains("request failed"));
    }
}
