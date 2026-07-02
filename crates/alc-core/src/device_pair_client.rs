//! auth-worker `/device/pair-internal` を叩いて device credential を新規発行する
//! クライアント (Refs #495)。値 (`device_secret`) は rust 側で保持せず、応答を
//! そのまま端末に転送する。
//!
//! ヘッダ名 / role 名は auth-worker 側 PR2 実装待ちの暫定値
//! (`docs/plan-device-repair.md` の「未確定・後続 PR で決める事項」参照)。

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// auth-worker が発行した device credential。
///
/// `Debug` は手書きで `device_secret` を redact する (値を誤って log に出す
/// 事故防止。このリポジトリの「値を log に出さない」方針の防御的担保)。
#[derive(Clone, PartialEq, Eq)]
pub struct PairedCredential {
    pub auth_device_id: String,
    pub device_secret: String,
}

impl std::fmt::Debug for PairedCredential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PairedCredential")
            .field("auth_device_id", &self.auth_device_id)
            .field("device_secret", &"***")
            .finish()
    }
}

#[derive(Debug)]
pub enum DevicePairClientError {
    /// auth-worker への到達失敗 / 非 2xx / body parse 失敗。詳細は log にのみ出す。
    Upstream(String),
}

#[async_trait]
pub trait DevicePairClient: Send + Sync {
    async fn mint(
        &self,
        tenant_id: Uuid,
        label: &str,
    ) -> Result<PairedCredential, DevicePairClientError>;
}

#[derive(Serialize)]
struct PairInternalRequest<'a> {
    tenant_id: Uuid,
    label: &'a str,
    role: &'a str,
}

#[derive(Deserialize)]
struct PairInternalResponse {
    auth_device_id: String,
    device_secret: String,
}

/// role 名は暫定 (auth-worker 側の既存 role 一覧との衝突確認が後続 PR で必要)。
const RE_PAIR_ROLE: &str = "device-alc-kiosk";

/// 実 HTTP 実装。`with_endpoint` はテスト用にエンドポイントを直指定する。
pub struct HttpDevicePairClient {
    client: reqwest::Client,
    pair_internal_url: String,
    shared_secret: String,
}

impl HttpDevicePairClient {
    pub fn new(auth_worker_url: &str, shared_secret: String) -> Self {
        Self::with_endpoint(
            format!(
                "{}/device/pair-internal",
                auth_worker_url.trim_end_matches('/')
            ),
            shared_secret,
        )
    }

    /// テスト用にエンドポイント (wiremock 等) を直指定するコンストラクタ。
    pub fn with_endpoint(pair_internal_url: String, shared_secret: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            pair_internal_url,
            shared_secret,
        }
    }

    /// `AUTH_WORKER_URL` / `RE_PAIR_INTERNAL_SHARED_SECRET` が両方揃っていれば
    /// `Some`。片方でも欠けていれば re-pair 機能は未設定 (`None`) とする。
    pub fn from_env() -> Option<Self> {
        Self::from_env_lookup(|k| std::env::var(k).ok())
    }

    /// env 依存を分離したテスト可能な実装 (`redact_broadcast::from_env_lookup` と同パターン)。
    fn from_env_lookup<F: Fn(&str) -> Option<String>>(getter: F) -> Option<Self> {
        let auth_worker_url = getter("AUTH_WORKER_URL").filter(|s| !s.is_empty())?;
        let shared_secret = getter("RE_PAIR_INTERNAL_SHARED_SECRET").filter(|s| !s.is_empty())?;
        Some(Self::new(&auth_worker_url, shared_secret))
    }
}

#[async_trait]
impl DevicePairClient for HttpDevicePairClient {
    async fn mint(
        &self,
        tenant_id: Uuid,
        label: &str,
    ) -> Result<PairedCredential, DevicePairClientError> {
        let resp = self
            .client
            .post(&self.pair_internal_url)
            .header("X-Internal-Secret", &self.shared_secret)
            .json(&PairInternalRequest {
                tenant_id,
                label,
                role: RE_PAIR_ROLE,
            })
            .send()
            .await
            .map_err(|e| DevicePairClientError::Upstream(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(DevicePairClientError::Upstream(format!(
                "pair-internal status={}",
                resp.status()
            )));
        }

        let body: PairInternalResponse = resp
            .json()
            .await
            .map_err(|e| DevicePairClientError::Upstream(e.to_string()))?;

        Ok(PairedCredential {
            auth_device_id: body.auth_device_id,
            device_secret: body.device_secret,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    // from_env_lookup は注入した getter closure だけで完結するテストで、実
    // プロセス env には触れない (並行実行される他テストとのレース無し)。

    #[test]
    fn from_env_lookup_none_when_url_missing() {
        assert!(HttpDevicePairClient::from_env_lookup(|k| {
            if k == "RE_PAIR_INTERNAL_SHARED_SECRET" {
                Some("secret".to_string())
            } else {
                None
            }
        })
        .is_none());
    }

    #[test]
    fn from_env_lookup_none_when_secret_missing() {
        assert!(HttpDevicePairClient::from_env_lookup(|k| {
            if k == "AUTH_WORKER_URL" {
                Some("https://auth.example.com".to_string())
            } else {
                None
            }
        })
        .is_none());
    }

    #[test]
    fn from_env_lookup_none_when_url_empty() {
        assert!(HttpDevicePairClient::from_env_lookup(|k| {
            match k {
                "AUTH_WORKER_URL" => Some(String::new()),
                "RE_PAIR_INTERNAL_SHARED_SECRET" => Some("secret".to_string()),
                _ => None,
            }
        })
        .is_none());
    }

    #[test]
    fn from_env_lookup_none_when_secret_empty() {
        assert!(HttpDevicePairClient::from_env_lookup(|k| {
            match k {
                "AUTH_WORKER_URL" => Some("https://auth.example.com".to_string()),
                "RE_PAIR_INTERNAL_SHARED_SECRET" => Some(String::new()),
                _ => None,
            }
        })
        .is_none());
    }

    #[test]
    fn from_env_lookup_returns_some_when_both_set() {
        let client = HttpDevicePairClient::from_env_lookup(|k| match k {
            "AUTH_WORKER_URL" => Some("https://auth.example.com".to_string()),
            "RE_PAIR_INTERNAL_SHARED_SECRET" => Some("secret".to_string()),
            _ => None,
        })
        .unwrap();
        assert_eq!(
            client.pair_internal_url,
            "https://auth.example.com/device/pair-internal"
        );
    }

    fn restore_env(key: &str, saved: Option<String>) {
        match saved {
            Some(v) => std::env::set_var(key, v),
            None => std::env::remove_var(key),
        }
    }

    #[test]
    fn from_env_uses_real_env() {
        // 唯一実 env を触るテスト。このキーは他テストで使われないため並行
        // 実行下でも安全 (redact_broadcast::from_env_uses_real_env と同パターン)。
        let saved_url = std::env::var("AUTH_WORKER_URL").ok();
        let saved_secret = std::env::var("RE_PAIR_INTERNAL_SHARED_SECRET").ok();

        std::env::remove_var("AUTH_WORKER_URL");
        std::env::remove_var("RE_PAIR_INTERNAL_SHARED_SECRET");
        assert!(HttpDevicePairClient::from_env().is_none());

        std::env::set_var("AUTH_WORKER_URL", "https://auth.example.com");
        std::env::set_var("RE_PAIR_INTERNAL_SHARED_SECRET", "secret");
        assert!(HttpDevicePairClient::from_env().is_some());

        restore_env("AUTH_WORKER_URL", saved_url);
        restore_env("RE_PAIR_INTERNAL_SHARED_SECRET", saved_secret);
    }

    #[test]
    fn new_builds_pair_internal_url() {
        let client = HttpDevicePairClient::new("https://auth.example.com/", "secret".into());
        assert_eq!(
            client.pair_internal_url,
            "https://auth.example.com/device/pair-internal"
        );
    }

    #[tokio::test]
    async fn mint_success() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/device/pair-internal"))
            .and(header("X-Internal-Secret", "s3cr3t"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "auth_device_id": "dev-1",
                "device_secret": "top-secret"
            })))
            .expect(1)
            .mount(&server)
            .await;

        let client = HttpDevicePairClient::with_endpoint(
            format!("{}/device/pair-internal", server.uri()),
            "s3cr3t".into(),
        );
        let cred = client
            .mint(Uuid::new_v4(), "alc-app:device-1")
            .await
            .unwrap();
        assert_eq!(cred.auth_device_id, "dev-1");
        assert_eq!(cred.device_secret, "top-secret");
    }

    #[tokio::test]
    async fn mint_non_2xx_is_upstream_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/device/pair-internal"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&server)
            .await;

        let client = HttpDevicePairClient::with_endpoint(
            format!("{}/device/pair-internal", server.uri()),
            "s".into(),
        );
        let err = client.mint(Uuid::new_v4(), "label").await.unwrap_err();
        assert!(matches!(err, DevicePairClientError::Upstream(_)));
    }

    #[tokio::test]
    async fn mint_parse_error_is_upstream_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/device/pair-internal"))
            .respond_with(ResponseTemplate::new(200).set_body_string("not json"))
            .mount(&server)
            .await;

        let client = HttpDevicePairClient::with_endpoint(
            format!("{}/device/pair-internal", server.uri()),
            "s".into(),
        );
        let err = client.mint(Uuid::new_v4(), "label").await.unwrap_err();
        assert!(matches!(err, DevicePairClientError::Upstream(_)));
    }

    #[tokio::test]
    async fn mint_unreachable_is_upstream_error() {
        let client = HttpDevicePairClient::with_endpoint(
            "http://127.0.0.1:1/device/pair-internal".into(),
            "s".into(),
        );
        let err = client.mint(Uuid::new_v4(), "label").await.unwrap_err();
        assert!(matches!(err, DevicePairClientError::Upstream(_)));
    }
}
