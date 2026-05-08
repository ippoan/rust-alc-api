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

        // tracing マクロを 1 行に collapse して llvm-cov のマルチライン未到達を回避
        match res {
            Ok(r) if r.status().is_success() => {
                tracing::debug!(tenant=%ev.tenant_id, doc=%ev.document_id, status=ev.status, "redact broadcast ok")
            }
            Ok(r) => {
                tracing::warn!(tenant=%ev.tenant_id, doc=%ev.document_id, http=%r.status(), "redact broadcast http error")
            }
            Err(e) => {
                tracing::warn!(tenant=%ev.tenant_id, doc=%ev.document_id, error=%e, "redact broadcast http call failed")
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

    /// 1 つの `_ => None` 分岐を `HashMap::get` 経由に集約して、各テストごとに
    /// 別個の closure を持たせず llvm-cov のカバレッジを確実にする。
    fn make_getter(
        url: Option<&'static str>,
        secret: Option<&'static str>,
    ) -> impl Fn(&str) -> Option<String> {
        let mut map = std::collections::HashMap::new();
        if let Some(v) = url {
            map.insert("NOTIFY_REDACT_BROADCAST_URL", v.to_string());
        }
        if let Some(v) = secret {
            map.insert("NOTIFY_REDACT_BROADCAST_SECRET", v.to_string());
        }
        move |k| map.get(k).cloned()
    }

    #[test]
    fn from_env_lookup_returns_some_when_both_set() {
        let b =
            RedactBroadcaster::from_env_lookup(make_getter(Some("https://r/broadcast"), Some("s")))
                .unwrap();
        assert_eq!(b.endpoint, "https://r/broadcast");
        assert_eq!(b.secret, "s");
    }

    #[test]
    fn from_env_lookup_none_when_url_missing() {
        // URL なし → endpoint=None で ? early return
        assert!(RedactBroadcaster::from_env_lookup(make_getter(None, Some("s"))).is_none());
    }

    #[test]
    fn from_env_lookup_none_when_secret_missing() {
        assert!(
            RedactBroadcaster::from_env_lookup(make_getter(Some("https://r/broadcast"), None))
                .is_none()
        );
    }

    #[test]
    fn from_env_lookup_none_when_url_empty() {
        assert!(RedactBroadcaster::from_env_lookup(make_getter(Some(""), Some("s"))).is_none());
    }

    #[test]
    fn from_env_lookup_none_when_secret_empty() {
        assert!(RedactBroadcaster::from_env_lookup(make_getter(
            Some("https://r/broadcast"),
            Some("")
        ))
        .is_none());
    }

    /// pure な restore helper。env mutation を 1 関数に集約し、`Some` / `None` 両分岐を
    /// テスト 2 件で確実にカバーする (CI 環境で saved=None でも Some 分岐が落ちない)。
    fn restore_env(key: &str, saved: Option<String>) {
        match saved {
            Some(v) => std::env::set_var(key, v),
            None => std::env::remove_var(key),
        }
    }

    #[test]
    fn restore_env_some_writes_value() {
        // Some 分岐: set_var を通る
        let key = "__REDACT_BROADCAST_TEST_RESTORE_SOME__";
        restore_env(key, Some("v".into()));
        assert_eq!(std::env::var(key).unwrap(), "v");
        std::env::remove_var(key);
    }

    #[test]
    fn restore_env_none_removes_var() {
        // None 分岐: remove_var を通る
        let key = "__REDACT_BROADCAST_TEST_RESTORE_NONE__";
        std::env::set_var(key, "x");
        restore_env(key, None);
        assert!(std::env::var(key).is_err());
    }

    #[test]
    fn from_env_uses_real_env() {
        // 本物 env を一時上書きして from_env (薄いラッパ) が Some/None 両方を返すことを
        // 確認。restore は pure helper `restore_env` 経由で行い、Some/None 分岐は
        // 個別テスト (上 2 つ) でカバー済み。
        let saved_url = std::env::var("NOTIFY_REDACT_BROADCAST_URL").ok();
        let saved_secret = std::env::var("NOTIFY_REDACT_BROADCAST_SECRET").ok();

        // Phase 1: 両 var 設定 → Some
        std::env::set_var("NOTIFY_REDACT_BROADCAST_URL", "https://test/broadcast");
        std::env::set_var("NOTIFY_REDACT_BROADCAST_SECRET", "test-secret");
        let b = RedactBroadcaster::from_env().expect("env vars set above");
        assert_eq!(b.endpoint, "https://test/broadcast");
        assert_eq!(b.secret, "test-secret");

        // Phase 2: 両 var 空文字 → filter で None 扱い → None 返却
        std::env::set_var("NOTIFY_REDACT_BROADCAST_URL", "");
        std::env::set_var("NOTIFY_REDACT_BROADCAST_SECRET", "");
        assert!(RedactBroadcaster::from_env().is_none());

        // Restore (経路は restore_env_* テストで個別カバー済み)
        restore_env("NOTIFY_REDACT_BROADCAST_URL", saved_url);
        restore_env("NOTIFY_REDACT_BROADCAST_SECRET", saved_secret);
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
