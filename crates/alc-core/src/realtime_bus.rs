//! 任意の `Serialize` ペイロードを notify-realtime-bus Worker `/broadcast` へ
//! POST する汎用クライアント。
//!
//! 用途:
//! - redact 完了通知 (`crates/alc-core/src/redact_broadcast.rs` の RedactBroadcaster
//!   が同 Worker に送る既存仕様と互換)
//! - Y時間 export job 完了通知 (Phase: 2026-05-10 perf 改善、`crates/alc-dtako/src/dtako_y_time_export/`)
//! - 今後の async job 完了系イベント全般
//!
//! Worker (`workers/realtime-bus/src/index.ts`) は payload に `tenant_id` /
//! `document_id` / `status` の 3 フィールドを必須要求する (現行実装、2026-05-08 phase
//! 2.5 時点)。新しい event 型を流す場合、これらを **必ず** 含めること
//! (`document_id` には job_id 等の subject id を入れる運用)。
//!
//! env vars は RedactBroadcaster と共有 (`NOTIFY_REDACT_BROADCAST_URL` /
//! `NOTIFY_REDACT_BROADCAST_SECRET`)。同一 Worker / 同一 secret に POST するため、
//! どちらの broadcaster を使っても fan-out 経路は同じ。frontend は payload の
//! `kind` フィールド (or 既存 redact では document_id 一致) で disambiguate する。

use std::time::Duration;

const HEADER_SECRET: &str = "X-Broadcast-Secret";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

/// notify-realtime-bus Worker の `/broadcast` に POST する汎用クライアント。
///
/// 1 instance / app で AppState に Option<Arc<...>> として持つ想定。
pub struct RealtimeBus {
    client: reqwest::Client,
    endpoint: String,
    secret: String,
}

impl RealtimeBus {
    /// Production 用構築。
    pub fn new(endpoint: String, secret: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            endpoint,
            secret,
        }
    }

    /// 環境変数から構築。両方欠けるか空なら `None`。RedactBroadcaster と同 env var を
    /// 共有 (どちらを使っても同 Worker に到達する)。
    pub fn from_env() -> Option<Self> {
        Self::from_env_lookup(|k| std::env::var(k).ok())
    }

    /// env 依存を分離したテスト可能な実装。
    fn from_env_lookup<F: Fn(&str) -> Option<String>>(getter: F) -> Option<Self> {
        let endpoint = getter("NOTIFY_REDACT_BROADCAST_URL").filter(|s| !s.is_empty())?;
        let secret = getter("NOTIFY_REDACT_BROADCAST_SECRET").filter(|s| !s.is_empty())?;
        Some(Self::new(endpoint, secret))
    }

    /// 任意の Serialize 値を Worker `/broadcast` に POST する。
    /// 失敗してもエラーは伝搬しない (warn のみ)。
    ///
    /// 呼び出し側で `tenant_id` / `document_id` / `status` を含めること
    /// (Worker validation 必須)。`kind` 等の追加 field は透過的に流れる。
    pub async fn broadcast<T: serde::Serialize + ?Sized>(&self, payload: &T) {
        let res = self
            .client
            .post(&self.endpoint)
            .header(HEADER_SECRET, &self.secret)
            .header("Content-Type", "application/json")
            .timeout(REQUEST_TIMEOUT)
            .json(payload)
            .send()
            .await;

        // tracing マクロを 1 行に collapse して llvm-cov のマルチライン未到達を回避
        match res {
            Ok(r) if r.status().is_success() => {
                tracing::debug!(endpoint = %self.endpoint, "realtime_bus broadcast ok")
            }
            Ok(r) => {
                tracing::warn!(endpoint = %self.endpoint, http = %r.status(), "realtime_bus broadcast http error")
            }
            Err(e) => {
                tracing::warn!(endpoint = %self.endpoint, error = %e, "realtime_bus broadcast http call failed")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use serde::Serialize;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[derive(Serialize)]
    struct TestEvent {
        tenant_id: &'static str,
        document_id: &'static str,
        status: &'static str,
        kind: &'static str,
    }

    fn ev() -> TestEvent {
        TestEvent {
            tenant_id: "00000000-0000-0000-0000-000000000000",
            document_id: "11111111-1111-1111-1111-111111111111",
            status: "completed",
            kind: "y_time_export",
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
        let b = RealtimeBus::from_env_lookup(make_getter(Some("https://r/broadcast"), Some("s")))
            .unwrap();
        assert_eq!(b.endpoint, "https://r/broadcast");
        assert_eq!(b.secret, "s");
    }

    #[test]
    fn from_env_lookup_none_when_url_missing() {
        assert!(RealtimeBus::from_env_lookup(make_getter(None, Some("s"))).is_none());
    }

    #[test]
    fn from_env_lookup_none_when_secret_missing() {
        assert!(
            RealtimeBus::from_env_lookup(make_getter(Some("https://r/broadcast"), None)).is_none()
        );
    }

    #[test]
    fn from_env_lookup_none_when_url_empty() {
        assert!(RealtimeBus::from_env_lookup(make_getter(Some(""), Some("s"))).is_none());
    }

    #[test]
    fn from_env_lookup_none_when_secret_empty() {
        assert!(
            RealtimeBus::from_env_lookup(make_getter(Some("https://r/broadcast"), Some("")))
                .is_none()
        );
    }

    /// pure restore helper。env mutation を 1 関数に集約し、`Some` / `None` 両分岐を
    /// テスト 2 件で確実にカバーする (CI 環境で saved=None でも Some 分岐が落ちない)。
    fn restore_env(key: &str, saved: Option<String>) {
        match saved {
            Some(v) => std::env::set_var(key, v),
            None => std::env::remove_var(key),
        }
    }

    #[test]
    fn restore_env_some_writes_value() {
        let key = "__REALTIME_BUS_TEST_RESTORE_SOME__";
        restore_env(key, Some("v".into()));
        assert_eq!(std::env::var(key).unwrap(), "v");
        std::env::remove_var(key);
    }

    #[test]
    fn restore_env_none_removes_var() {
        let key = "__REALTIME_BUS_TEST_RESTORE_NONE__";
        std::env::set_var(key, "x");
        restore_env(key, None);
        assert!(std::env::var(key).is_err());
    }

    #[test]
    fn from_env_uses_real_env() {
        let saved_url = std::env::var("NOTIFY_REDACT_BROADCAST_URL").ok();
        let saved_secret = std::env::var("NOTIFY_REDACT_BROADCAST_SECRET").ok();

        std::env::set_var("NOTIFY_REDACT_BROADCAST_URL", "https://test/broadcast");
        std::env::set_var("NOTIFY_REDACT_BROADCAST_SECRET", "test-secret");
        let b = RealtimeBus::from_env().expect("env vars set above");
        assert_eq!(b.endpoint, "https://test/broadcast");
        assert_eq!(b.secret, "test-secret");

        std::env::set_var("NOTIFY_REDACT_BROADCAST_URL", "");
        std::env::set_var("NOTIFY_REDACT_BROADCAST_SECRET", "");
        assert!(RealtimeBus::from_env().is_none());

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
            .and(wiremock::matchers::body_string_contains(
                "\"y_time_export\"",
            ))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;

        let client = RealtimeBus::new(format!("{}/broadcast", server.uri()), "s3cret".into());
        client.broadcast(&ev()).await;
    }

    #[tokio::test]
    async fn broadcast_4xx_does_not_panic() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/broadcast"))
            .respond_with(ResponseTemplate::new(401).set_body_string("unauthorized"))
            .mount(&server)
            .await;

        let client = RealtimeBus::new(format!("{}/broadcast", server.uri()), "wrong".into());
        client.broadcast(&ev()).await;
    }

    #[tokio::test]
    async fn broadcast_unreachable_endpoint_does_not_panic() {
        // 127.0.0.1:1 は接続拒否 → reqwest::Error
        let client = RealtimeBus::new("http://127.0.0.1:1/broadcast".into(), "s".into());
        client.broadcast(&ev()).await;
    }
}
