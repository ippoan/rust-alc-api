//! auth-worker `/auth/introspect` client (Refs #479 Stage 2 PR-2)。
//!
//! 旧実装は共有 `JWT_SECRET` でユーザー JWT を gateway 内で HS256 検証していた
//! (#434 の「proxy が introspect で検証して注入」モデルの旧形)。JWT の署名・
//! 検証を auth-worker に集約するため、検証を auth-worker `/auth/introspect`
//! (server-to-server、`INTERNAL_SHARED_SECRET` 認証) への委譲に置換した。
//! これにより gateway は `JWT_SECRET` を持たない。

use serde::Deserialize;
use uuid::Uuid;

/// auth-worker `/auth/introspect` の 200 response (RFC 7662 風)。
/// `active: false` の時は他 field は載らない (情報リーク回避)。
#[derive(Debug, Deserialize)]
pub struct IntrospectResponse {
    pub active: bool,
    pub tenant_id: Option<Uuid>,
    pub sub: Option<Uuid>,
    pub email: Option<String>,
    pub role: Option<String>,
}

/// introspect が返した検証済み identity。backend へ `X-Tenant-ID` /
/// `X-User-*` として注入する。
#[derive(Debug, Clone)]
pub struct Identity {
    pub tenant_id: Uuid,
    pub user_id: Uuid,
    pub email: String,
    pub role: String,
}

/// auth-worker introspect の呼び出し client。
///
/// 認証は `Authorization: <INTERNAL_SHARED_SECRET>` (生の値、Bearer prefix
/// なし = auth-worker 側契約)。`origin` は per-app テナント ACL
/// (`APP_TENANT_ACL`) の判定キーで、auth-worker は origin 欠落を
/// fail-closed (`active:false`) にする。
pub struct IntrospectClient {
    client: reqwest::Client,
    endpoint: String,
    shared_secret: String,
}

impl IntrospectClient {
    pub fn new(client: reqwest::Client, auth_worker_url: &str, shared_secret: String) -> Self {
        Self {
            client,
            endpoint: format!("{}/auth/introspect", auth_worker_url.trim_end_matches('/')),
            shared_secret,
        }
    }

    /// token + origin を auth-worker で検証する。
    ///
    /// 失敗 (署名不正 / exp 切れ / ACL 不許可 / auth-worker 不達 / parse 不能)
    /// は全て `None` = 未認証扱い (identity 注入なし)。旧 `verify_jwt` 失敗時と
    /// 同じ扱いで、proxy 自体は 5xx にしない (認可判定は backend 側)。
    pub async fn introspect(&self, token: &str, origin: &str) -> Option<Identity> {
        let resp = self
            .client
            .post(&self.endpoint)
            .header("Authorization", &self.shared_secret)
            .json(&serde_json::json!({ "token": token, "origin": origin }))
            .send()
            .await
            .map_err(|e| tracing::warn!("introspect unreachable: {e}"))
            .ok()?;
        let status = resp.status();
        if !status.is_success() {
            tracing::warn!("introspect returned {status}");
            return None;
        }
        let body: IntrospectResponse = resp
            .json()
            .await
            .map_err(|e| tracing::warn!("introspect parse error: {e}"))
            .ok()?;
        if !body.active {
            return None;
        }
        Some(Identity {
            tenant_id: body.tenant_id?,
            user_id: body.sub?,
            email: body.email?,
            role: body.role?,
        })
    }
}

/// Authorization ヘッダーから Bearer トークンを抽出
pub fn extract_bearer_token(header_value: &str) -> Option<&str> {
    header_value.strip_prefix("Bearer ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{body_partial_json, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn client(server_uri: &str) -> IntrospectClient {
        IntrospectClient::new(
            reqwest::Client::new(),
            server_uri,
            "test-shared-secret".to_string(),
        )
    }

    #[tokio::test]
    async fn introspect_active_true_returns_identity() {
        let server = MockServer::start().await;
        let tenant = Uuid::new_v4();
        let sub = Uuid::new_v4();
        Mock::given(method("POST"))
            .and(path("/auth/introspect"))
            .and(header("Authorization", "test-shared-secret"))
            .and(body_partial_json(serde_json::json!({
                "token": "tok",
                "origin": "https://alc.ippoan.org"
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "active": true,
                "tenant_id": tenant,
                "sub": sub,
                "email": "u@example.com",
                "role": "admin",
                "exp": 9999999999u64
            })))
            .expect(1)
            .mount(&server)
            .await;

        let id = client(&server.uri())
            .introspect("tok", "https://alc.ippoan.org")
            .await
            .expect("identity");
        assert_eq!(id.tenant_id, tenant);
        assert_eq!(id.user_id, sub);
        assert_eq!(id.email, "u@example.com");
        assert_eq!(id.role, "admin");
    }

    #[tokio::test]
    async fn introspect_active_false_returns_none() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/auth/introspect"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({ "active": false })),
            )
            .mount(&server)
            .await;
        assert!(client(&server.uri()).introspect("bad", "o").await.is_none());
    }

    #[tokio::test]
    async fn introspect_non_200_returns_none() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/auth/introspect"))
            .respond_with(
                ResponseTemplate::new(401)
                    .set_body_json(serde_json::json!({ "error": "unauthorized" })),
            )
            .mount(&server)
            .await;
        assert!(client(&server.uri()).introspect("tok", "o").await.is_none());
    }

    #[tokio::test]
    async fn introspect_malformed_body_returns_none() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/auth/introspect"))
            .respond_with(ResponseTemplate::new(200).set_body_string("not-json"))
            .mount(&server)
            .await;
        assert!(client(&server.uri()).introspect("tok", "o").await.is_none());
    }

    #[tokio::test]
    async fn introspect_active_true_but_missing_fields_returns_none() {
        // active:true なのに identity field が欠ける異常応答は fail-closed。
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/auth/introspect"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({ "active": true })),
            )
            .mount(&server)
            .await;
        assert!(client(&server.uri()).introspect("tok", "o").await.is_none());
    }

    #[tokio::test]
    async fn introspect_unreachable_returns_none() {
        // 存在しない port へ接続 → network error → None。
        let c = IntrospectClient::new(
            reqwest::Client::new(),
            "http://127.0.0.1:1",
            "s".to_string(),
        );
        assert!(c.introspect("tok", "o").await.is_none());
    }

    #[test]
    fn test_extract_bearer_token() {
        assert_eq!(extract_bearer_token("Bearer abc123"), Some("abc123"));
        assert_eq!(extract_bearer_token("Basic abc123"), None);
        assert_eq!(extract_bearer_token("abc123"), None);
    }
}
