//! redact 完了イベントの fire-and-forget broadcast クライアント。
//!
//! `crates/alc-notify/src/background_redaction.rs` の terminal 状態
//! (`completed` / `skipped` / `failed`) で呼び、shared secret 1 個で経路保護した
//! HTTP POST を Cloudflare Worker (notify-realtime-bus) の `/broadcast` に送る。
//! Worker は `X-Broadcast-Secret` ヘッダを検証し、ペイロードの `tenant_id` を
//! Durable Object 名前空間として該当 DO の hibernated WS にだけ
//! メッセージを fan-out する。
//!
//! `webhook.rs` / `fcm.rs` と同じく alc-core に置き、AppState に Optional で持たせる。
//! 現状 1 impl しかないので trait は導入せず concrete + wiremock テストで 100% 確保する。

use std::time::Duration;

use serde::Serialize;
use uuid::Uuid;

const HEADER_SECRET: &str = "X-Broadcast-Secret";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

/// Realtime Worker に送るイベント。frontend の `useRedactionWatch` composable は
/// この shape でメッセージを受け取る。
#[derive(Debug, Clone, Serialize)]
pub struct RedactEvent<'a> {
    pub tenant_id: Uuid,
    pub document_id: Uuid,
    /// `completed` | `skipped` | `failed`
    pub status: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redactions_applied: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redaction_error: Option<&'a str>,
}

/// `notify-realtime-bus` Cloudflare Worker の `/broadcast` に POST するクライアント。
pub struct RedactBroadcaster {
    client: reqwest::Client,
    endpoint: String,
    secret: String,
}

impl RedactBroadcaster {
    /// Production 用構築。
    pub fn new(endpoint: String, secret: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            endpoint,
            secret,
        }
    }

    /// 環境変数から構築。両方欠けるか空なら `None` を返し、上位で broadcast を no-op にする。
    pub fn from_env() -> Option<Self> {
        Self::from_env_lookup(|k| std::env::var(k).ok())
    }

    /// env 依存を分離したテスト可能な実装。
    fn from_env_lookup<F: Fn(&str) -> Option<String>>(getter: F) -> Option<Self> {
        let endpoint = getter("NOTIFY_REDACT_BROADCAST_URL").filter(|s| !s.is_empty())?;
        let secret = getter("NOTIFY_REDACT_BROADCAST_SECRET").filter(|s| !s.is_empty())?;
        Some(Self::new(endpoint, secret))
    }

    /// Realtime Worker にイベントを送る。失敗してもエラーは伝搬しない (warn のみ)。
    pub async fn broadcast(&self, ev: &RedactEvent<'_>) {
        let res = self
            .client
            .post(&self.endpoint)
            .header(HEADER_SECRET, &self.secret)
            .header("Content-Type", "application/json")
            .timeout(REQUEST_TIMEOUT)
            .json(ev)
            .send()
            .await;

        match res {
            Ok(r) if r.status().is_success() => {
                tracing::debug!(
                    "redact broadcast ok tenant={} doc={} status={}",
                    ev.tenant_id,
                    ev.document_id,
                    ev.status
                );
            }
            Ok(r) => {
                tracing::warn!(
                    "redact broadcast http {}: tenant={} doc={}",
                    r.status(),
                    ev.tenant_id,
                    ev.document_id
                );
            }
            Err(e) => {
                tracing::warn!(
                    "redact broadcast error tenant={} doc={}: {e}",
                    ev.tenant_id,
                    ev.document_id
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn ev_completed() -> RedactEvent<'static> {
        RedactEvent {
            tenant_id: Uuid::nil(),
            document_id: Uuid::nil(),
            status: "completed",
            redactions_applied: Some(3),
            redaction_error: None,
        }
    }

    fn ev_failed() -> RedactEvent<'static> {
        RedactEvent {
            tenant_id: Uuid::nil(),
            document_id: Uuid::nil(),
            status: "failed",
            redactions_applied: None,
            redaction_error: Some("boom"),
        }
    }

    // --- from_env_lookup ---

    #[test]
    fn from_env_lookup_returns_some_when_both_set() {
        let b = RedactBroadcaster::from_env_lookup(|k| match k {
            "NOTIFY_REDACT_BROADCAST_URL" => Some("https://r/broadcast".into()),
            "NOTIFY_REDACT_BROADCAST_SECRET" => Some("s3cret".into()),
            _ => None,
        })
        .unwrap();
        assert_eq!(b.endpoint, "https://r/broadcast");
        assert_eq!(b.secret, "s3cret");
    }

    #[test]
    fn from_env_lookup_none_when_url_missing() {
        let b = RedactBroadcaster::from_env_lookup(|k| match k {
            "NOTIFY_REDACT_BROADCAST_SECRET" => Some("s3cret".into()),
            _ => None,
        });
        assert!(b.is_none());
    }

    #[test]
    fn from_env_lookup_none_when_secret_missing() {
        let b = RedactBroadcaster::from_env_lookup(|k| match k {
            "NOTIFY_REDACT_BROADCAST_URL" => Some("https://r/broadcast".into()),
            _ => None,
        });
        assert!(b.is_none());
    }

    #[test]
    fn from_env_lookup_none_when_url_empty() {
        let b = RedactBroadcaster::from_env_lookup(|k| match k {
            "NOTIFY_REDACT_BROADCAST_URL" => Some(String::new()),
            "NOTIFY_REDACT_BROADCAST_SECRET" => Some("s3cret".into()),
            _ => None,
        });
        assert!(b.is_none());
    }

    #[test]
    fn from_env_lookup_none_when_secret_empty() {
        let b = RedactBroadcaster::from_env_lookup(|k| match k {
            "NOTIFY_REDACT_BROADCAST_URL" => Some("https://r/broadcast".into()),
            "NOTIFY_REDACT_BROADCAST_SECRET" => Some(String::new()),
            _ => None,
        });
        assert!(b.is_none());
    }

    #[test]
    fn from_env_uses_real_env() {
        // 本物 env から呼ぶ薄いラッパなので、両 var 未設定 → None になることだけ確認。
        // 値の test は from_env_lookup でカバー済み。
        let saved_url = std::env::var("NOTIFY_REDACT_BROADCAST_URL").ok();
        let saved_secret = std::env::var("NOTIFY_REDACT_BROADCAST_SECRET").ok();
        std::env::remove_var("NOTIFY_REDACT_BROADCAST_URL");
        std::env::remove_var("NOTIFY_REDACT_BROADCAST_SECRET");
        let result = RedactBroadcaster::from_env();
        // restore
        if let Some(v) = saved_url {
            std::env::set_var("NOTIFY_REDACT_BROADCAST_URL", v);
        }
        if let Some(v) = saved_secret {
            std::env::set_var("NOTIFY_REDACT_BROADCAST_SECRET", v);
        }
        assert!(result.is_none());
    }

    // --- broadcast ---

    #[tokio::test]
    async fn broadcast_success_sends_secret_header_and_body() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/broadcast"))
            .and(header("X-Broadcast-Secret", "s3cret"))
            .and(header("Content-Type", "application/json"))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;

        let client = RedactBroadcaster::new(format!("{}/broadcast", server.uri()), "s3cret".into());
        client.broadcast(&ev_completed()).await;
    }

    #[tokio::test]
    async fn broadcast_serializes_failed_event_with_error_message() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/broadcast"))
            .and(wiremock::matchers::body_string_contains("\"failed\""))
            .and(wiremock::matchers::body_string_contains("\"boom\""))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;

        let client = RedactBroadcaster::new(format!("{}/broadcast", server.uri()), "s3cret".into());
        client.broadcast(&ev_failed()).await;
    }

    #[tokio::test]
    async fn broadcast_4xx_does_not_panic() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/broadcast"))
            .respond_with(ResponseTemplate::new(401).set_body_string("unauthorized"))
            .mount(&server)
            .await;

        let client = RedactBroadcaster::new(format!("{}/broadcast", server.uri()), "wrong".into());
        client.broadcast(&ev_completed()).await;
    }

    #[tokio::test]
    async fn broadcast_unreachable_endpoint_does_not_panic() {
        // 127.0.0.1:1 は接続拒否 → reqwest::Error
        let client = RedactBroadcaster::new("http://127.0.0.1:1/broadcast".into(), "s".into());
        client.broadcast(&ev_completed()).await;
    }
}
