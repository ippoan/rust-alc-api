//! カメラ停止の自動チケットを trouble-worker (ippoan/alc-trouble-worker) の内部口へ
//! 起票する [`DownTicketSink`] の HTTP 実装 (Refs #747)。
//!
//! trouble は Worker へ移り、rust には trouble の DB repo が無い。auth-worker の
//! `alc-internal-proxy` を通して `/api/internal/trouble/camera-down-tickets` を叩く。
//! 認証は既存の `INTERNAL_SHARED_SECRET` を `X-Alc-Proxy-Secret` で送る (proxy が検証する
//! のはこの header。`X-Internal-Shared-Secret` ではない)。テナントは `X-Tenant-ID`。
//! 雛形は `alc_devices::device_pair_client::HttpDevicePairClient`。

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::health::{CameraDownTicket, DownTicketError, DownTicketSink};

const CAMERA_DOWN_TICKETS_PATH: &str =
    "/alc-internal-proxy/api/internal/trouble/camera-down-tickets";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Serialize)]
struct CameraDownTicketRequest {
    title: String,
    description: String,
    location: String,
    occurred_at: DateTime<Utc>,
    camera_id: Uuid,
}

impl From<CameraDownTicket> for CameraDownTicketRequest {
    fn from(t: CameraDownTicket) -> Self {
        Self {
            title: t.title,
            description: t.description,
            location: t.location,
            occurred_at: t.occurred_at,
            camera_id: t.camera_id,
        }
    }
}

#[derive(Deserialize)]
struct CameraDownTicketResponse {
    id: Uuid,
}

/// 実 HTTP 実装。`with_endpoint` はテスト用にエンドポイントを直指定する。
pub struct HttpDownTicketSink {
    client: reqwest::Client,
    endpoint: String,
    shared_secret: String,
}

impl HttpDownTicketSink {
    pub fn new(auth_worker_url: &str, shared_secret: String) -> Self {
        Self::with_endpoint(
            format!(
                "{}{CAMERA_DOWN_TICKETS_PATH}",
                auth_worker_url.trim_end_matches('/')
            ),
            shared_secret,
        )
    }

    /// テスト用にエンドポイント (wiremock 等) を直指定するコンストラクタ。
    pub fn with_endpoint(endpoint: String, shared_secret: String) -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(REQUEST_TIMEOUT)
                .build()
                .unwrap_or_default(),
            endpoint,
            shared_secret,
        }
    }

    /// `AUTH_WORKER_URL` / `INTERNAL_SHARED_SECRET` が両方揃っていれば `Some`。
    /// 片方でも欠けていれば (空文字も) 未設定 (`None`)。
    pub fn from_env() -> Option<Self> {
        Self::from_env_lookup(|k| std::env::var(k).ok())
    }

    /// env 依存を分離したテスト可能な実装。
    fn from_env_lookup<F: Fn(&str) -> Option<String>>(getter: F) -> Option<Self> {
        let auth_worker_url = getter("AUTH_WORKER_URL").filter(|s| !s.is_empty())?;
        let shared_secret = getter("INTERNAL_SHARED_SECRET").filter(|s| !s.is_empty())?;
        Some(Self::new(&auth_worker_url, shared_secret))
    }
}

#[async_trait]
impl DownTicketSink for HttpDownTicketSink {
    async fn open_down_ticket(
        &self,
        tenant_id: Uuid,
        ticket: CameraDownTicket,
    ) -> Result<Uuid, DownTicketError> {
        let resp = self
            .client
            .post(&self.endpoint)
            .header("X-Alc-Proxy-Secret", &self.shared_secret)
            .header("X-Tenant-ID", tenant_id.to_string())
            .json(&CameraDownTicketRequest::from(ticket))
            .send()
            .await
            .map_err(|_| DownTicketError::Request)?;

        let status = resp.status();
        if status != reqwest::StatusCode::CREATED {
            return Err(DownTicketError::Status(status.as_u16()));
        }

        let body: CameraDownTicketResponse = resp
            .json()
            .await
            .map_err(|_| DownTicketError::InvalidResponse)?;
        Ok(body.id)
    }
}

/// 起票先が未設定のときの sink。起票せず、最初の 1 回だけ warn を出して Err を返す。
#[derive(Default)]
pub struct DisabledDownTicketSink {
    warned: AtomicBool,
}

#[async_trait]
impl DownTicketSink for DisabledDownTicketSink {
    async fn open_down_ticket(
        &self,
        _tenant_id: Uuid,
        _ticket: CameraDownTicket,
    ) -> Result<Uuid, DownTicketError> {
        if !self.warned.swap(true, Ordering::SeqCst) {
            tracing::warn!("camera down ticket sink not configured; skipping auto ticket");
        }
        Err(DownTicketError::NotConfigured)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn ticket(camera_id: Uuid) -> CameraDownTicket {
        CameraDownTicket {
            title: "監視カメラ異常: 正門".to_string(),
            description: "応答しません".to_string(),
            location: "192.168.1.10".to_string(),
            occurred_at: "2026-10-10T01:02:03Z".parse().unwrap(),
            camera_id,
        }
    }

    async fn sink_for(server: &MockServer) -> HttpDownTicketSink {
        HttpDownTicketSink::new(&format!("{}/", server.uri()), "s3cr3t".into())
    }

    /// 渡した (key, value) だけを返す env の代わり。
    fn lookup<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |k| {
            pairs
                .iter()
                .find(|(key, _)| *key == k)
                .map(|(_, v)| v.to_string())
        }
    }

    #[test]
    fn from_env_lookup_none_unless_both_set() {
        let url = "https://auth.example.com";
        for pairs in [
            &[("INTERNAL_SHARED_SECRET", "secret")][..],
            &[("AUTH_WORKER_URL", url)][..],
            &[
                ("AUTH_WORKER_URL", ""),
                ("INTERNAL_SHARED_SECRET", "secret"),
            ][..],
            &[("AUTH_WORKER_URL", url), ("INTERNAL_SHARED_SECRET", "")][..],
        ] {
            assert!(HttpDownTicketSink::from_env_lookup(lookup(pairs)).is_none());
        }
    }

    #[test]
    fn from_env_lookup_some_when_both_set() {
        let sink = HttpDownTicketSink::from_env_lookup(lookup(&[
            ("AUTH_WORKER_URL", "https://auth.example.com/"),
            ("INTERNAL_SHARED_SECRET", "secret"),
        ]))
        .unwrap();
        assert_eq!(
            sink.endpoint,
            "https://auth.example.com/alc-internal-proxy/api/internal/trouble/camera-down-tickets"
        );
        assert_eq!(sink.shared_secret, "secret");
    }

    #[test]
    fn from_env_matches_process_env() {
        // 実 env は書き換えない (並行するテストとの競合を避ける)。読むだけで from_env を通す。
        let expected =
            HttpDownTicketSink::from_env_lookup(|k| std::env::var(k).ok()).map(|s| s.endpoint);
        assert_eq!(HttpDownTicketSink::from_env().map(|s| s.endpoint), expected);
    }

    #[tokio::test]
    async fn created_returns_id_and_sends_headers_and_body() {
        let server = MockServer::start().await;
        let tenant_id = Uuid::new_v4();
        let camera_id = Uuid::new_v4();
        let ticket_id = Uuid::new_v4();
        Mock::given(method("POST"))
            .and(path(CAMERA_DOWN_TICKETS_PATH))
            .and(header("X-Alc-Proxy-Secret", "s3cr3t"))
            .and(header("X-Tenant-ID", tenant_id.to_string().as_str()))
            .respond_with(
                ResponseTemplate::new(201).set_body_json(serde_json::json!({ "id": ticket_id })),
            )
            .expect(1)
            .mount(&server)
            .await;

        let got = sink_for(&server)
            .await
            .open_down_ticket(tenant_id, ticket(camera_id))
            .await
            .unwrap();
        assert_eq!(got, ticket_id);

        let requests = server.received_requests().await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
        assert_eq!(
            body,
            serde_json::json!({
                "title": "監視カメラ異常: 正門",
                "description": "応答しません",
                "location": "192.168.1.10",
                "occurred_at": "2026-10-10T01:02:03Z",
                "camera_id": camera_id,
            })
        );
        assert!(requests[0]
            .headers
            .get("X-Internal-Shared-Secret")
            .is_none());
    }

    async fn status_case(code: u16) -> DownTicketError {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path(CAMERA_DOWN_TICKETS_PATH))
            .respond_with(
                ResponseTemplate::new(code)
                    .set_body_json(serde_json::json!({ "id": Uuid::new_v4() })),
            )
            .mount(&server)
            .await;
        sink_for(&server)
            .await
            .open_down_ticket(Uuid::new_v4(), ticket(Uuid::new_v4()))
            .await
            .unwrap_err()
    }

    #[tokio::test]
    async fn non_created_statuses_are_errors() {
        assert_eq!(status_case(200).await, DownTicketError::Status(200));
        assert_eq!(status_case(403).await, DownTicketError::Status(403));
        assert_eq!(status_case(503).await, DownTicketError::Status(503));
    }

    #[tokio::test]
    async fn invalid_json_is_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path(CAMERA_DOWN_TICKETS_PATH))
            .respond_with(ResponseTemplate::new(201).set_body_string("not json"))
            .mount(&server)
            .await;
        let err = sink_for(&server)
            .await
            .open_down_ticket(Uuid::new_v4(), ticket(Uuid::new_v4()))
            .await
            .unwrap_err();
        assert_eq!(err, DownTicketError::InvalidResponse);
    }

    #[tokio::test]
    async fn unreachable_is_request_error() {
        let sink = HttpDownTicketSink::with_endpoint("http://127.0.0.1:1/x".into(), "s".into());
        let err = sink
            .open_down_ticket(Uuid::new_v4(), ticket(Uuid::new_v4()))
            .await
            .unwrap_err();
        assert_eq!(err, DownTicketError::Request);
    }

    #[tokio::test]
    async fn disabled_sink_always_errors() {
        let sink = DisabledDownTicketSink::default();
        for _ in 0..2 {
            let err = sink
                .open_down_ticket(Uuid::new_v4(), ticket(Uuid::new_v4()))
                .await
                .unwrap_err();
            assert_eq!(err, DownTicketError::NotConfigured);
        }
    }

    #[test]
    fn error_display_has_fixed_words() {
        assert_eq!(
            DownTicketError::NotConfigured.to_string(),
            "down ticket sink not configured"
        );
        assert_eq!(
            DownTicketError::Request.to_string(),
            "down ticket request failed"
        );
        assert_eq!(
            DownTicketError::Status(500).to_string(),
            "down ticket unexpected status 500"
        );
        assert_eq!(
            DownTicketError::InvalidResponse.to_string(),
            "down ticket invalid response"
        );
    }
}
