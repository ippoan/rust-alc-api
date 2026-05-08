//! LINE Messaging API client (JWT assertion 方式)
//! https://developers.line.biz/ja/docs/messaging-api/generate-json-web-token/

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

const PROD_TOKEN_ENDPOINT: &str = "https://api.line.me/oauth2/v2.1/token";
const PROD_PUSH_ENDPOINT: &str = "https://api.line.me/v2/bot/message/push";

#[derive(Debug, thiserror::Error)]
pub enum LineError {
    #[error("JWT error: {0}")]
    Jwt(#[from] jsonwebtoken::errors::Error),
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("Token issue failed: {0}")]
    TokenIssueFailed(String),
    #[error("Send failed: {0}")]
    SendFailed(String),
}

/// JWT 方式の LINE 設定 (復号済み)
#[derive(Debug, Clone)]
pub struct LineConfig {
    pub channel_id: String,
    pub channel_secret: String,
    pub key_id: String,
    pub private_key: String, // PEM format
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct JwtClaims {
    pub iss: String,
    pub sub: String,
    pub aud: String,
    pub exp: u64,
    pub token_exp: u64,
    pub token_type: String,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    expires_in: u64,
}

#[derive(Debug)]
struct CachedToken {
    access_token: String,
    expires_at: u64,
}

/// 指定された private key (PEM) と channel_id/key_id で LINE 向け JWT assertion を生成する。
///
/// - `exp`       — JWT assertion の有効期限。UNIX 秒 (max now+30分)
/// - `token_exp` — 発行される access token の有効期間、**秒数** (max 30日 = 2592000)
pub(crate) fn build_jwt_assertion(config: &LineConfig, now: u64) -> Result<String, LineError> {
    let claims = JwtClaims {
        iss: config.channel_id.clone(),
        sub: config.channel_id.clone(),
        aud: "https://api.line.me/".to_string(),
        exp: now + 1800,
        token_exp: 60 * 60 * 24 * 30,
        token_type: "Bearer".to_string(),
    };

    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some(config.key_id.clone());
    header.typ = Some("JWT".to_string());

    let key = EncodingKey::from_rsa_pem(config.private_key.as_bytes())?;
    Ok(encode(&header, &claims, &key)?)
}

pub struct LineClient {
    client: reqwest::Client,
    cache: Arc<RwLock<Option<CachedToken>>>,
    token_endpoint: String,
    push_endpoint: String,
}

impl Default for LineClient {
    fn default() -> Self {
        Self::new()
    }
}

impl LineClient {
    pub fn new() -> Self {
        Self::with_endpoints(
            PROD_TOKEN_ENDPOINT.to_string(),
            PROD_PUSH_ENDPOINT.to_string(),
        )
    }

    pub fn with_endpoints(token_endpoint: String, push_endpoint: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            cache: Arc::new(RwLock::new(None)),
            token_endpoint,
            push_endpoint,
        }
    }

    pub async fn get_access_token(&self, config: &LineConfig) -> Result<String, LineError> {
        // キャッシュチェック
        {
            let cache = self.cache.read().await;
            if let Some(ref cached) = *cache {
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs();
                if cached.expires_at > now + 60 {
                    return Ok(cached.access_token.clone());
                }
            }
        }

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let jwt = build_jwt_assertion(config, now)?;

        // トークン交換
        let params = [
            ("grant_type", "client_credentials"),
            (
                "client_assertion_type",
                "urn:ietf:params:oauth:client-assertion-type:jwt-bearer",
            ),
            ("client_assertion", &jwt),
        ];

        let resp = self
            .client
            .post(&self.token_endpoint)
            .form(&params)
            .send()
            .await?;

        let status = resp.status();
        let body = resp.text().await?;

        if !status.is_success() {
            tracing::error!("LINE token issue failed: {status} - {body}");
            return Err(LineError::TokenIssueFailed(body));
        }

        let token: TokenResponse = serde_json::from_str(&body)
            .map_err(|e| LineError::TokenIssueFailed(format!("parse error: {e} - body: {body}")))?;

        let access_token = token.access_token.clone();

        // キャッシュ
        let mut cache = self.cache.write().await;
        *cache = Some(CachedToken {
            access_token: token.access_token,
            expires_at: now + token.expires_in,
        });

        Ok(access_token)
    }

    /// ユーザーに push メッセージを送信
    pub async fn push_text(
        &self,
        config: &LineConfig,
        user_id: &str,
        text: &str,
    ) -> Result<(), LineError> {
        let access_token = self.get_access_token(config).await?;

        let body = serde_json::json!({
            "to": user_id,
            "messages": [
                {
                    "type": "text",
                    "text": text,
                }
            ]
        });

        let resp = self
            .client
            .post(&self.push_endpoint)
            .header("Authorization", format!("Bearer {}", access_token))
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            tracing::error!("LINE push failed: {status} - {body}");
            return Err(LineError::SendFailed(format!("{status}: {body}")));
        }

        Ok(())
    }

    /// ユーザーに画像メッセージを push する。
    ///
    /// `original_url` は最大 10 MB / 4096x4096 / JPEG-PNG (LINE 仕様)。
    /// `preview_url` は最大 1 MB / 240x240 推奨。両方同じ URL でも構わない (LINE 側が
    /// オリジナルから縮小して preview にも使える)。
    /// 仕様: <https://developers.line.biz/ja/reference/messaging-api/#image-message>
    pub async fn push_image(
        &self,
        config: &LineConfig,
        user_id: &str,
        original_url: &str,
        preview_url: &str,
    ) -> Result<(), LineError> {
        let access_token = self.get_access_token(config).await?;

        let body = serde_json::json!({
            "to": user_id,
            "messages": [
                {
                    "type": "image",
                    "originalContentUrl": original_url,
                    "previewImageUrl": preview_url,
                }
            ]
        });

        let resp = self
            .client
            .post(&self.push_endpoint)
            .header("Authorization", format!("Bearer {}", access_token))
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            tracing::error!("LINE push image failed: {status} - {body}");
            return Err(LineError::SendFailed(format!("{status}: {body}")));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jsonwebtoken::{decode, decode_header, DecodingKey, Validation};
    use rsa::pkcs1::EncodeRsaPrivateKey;
    use rsa::pkcs8::EncodePublicKey;
    use rsa::rand_core::OsRng;
    use rsa::{RsaPrivateKey, RsaPublicKey};
    use std::sync::OnceLock;
    use wiremock::matchers::{body_string_contains, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    struct TestKey {
        private_pem: String,
        public_pem: String,
    }

    fn test_key() -> &'static TestKey {
        static KEY: OnceLock<TestKey> = OnceLock::new();
        KEY.get_or_init(|| {
            let private = RsaPrivateKey::new(&mut OsRng, 2048).unwrap();
            let public = RsaPublicKey::from(&private);
            let private_pem = private
                .to_pkcs1_pem(rsa::pkcs1::LineEnding::LF)
                .unwrap()
                .to_string();
            let public_pem = public
                .to_public_key_pem(rsa::pkcs8::LineEnding::LF)
                .unwrap();
            TestKey {
                private_pem,
                public_pem,
            }
        })
    }

    fn test_config() -> LineConfig {
        LineConfig {
            channel_id: "test-channel-id".into(),
            channel_secret: "test-channel-secret".into(),
            key_id: "test-key-id".into(),
            private_key: test_key().private_pem.clone(),
        }
    }

    // --- build_jwt_assertion ---

    #[test]
    fn build_jwt_assertion_sets_line_spec_claims() {
        let now = 1_700_000_000;
        let jwt = build_jwt_assertion(&test_config(), now).unwrap();

        // Header
        let header = decode_header(&jwt).unwrap();
        assert_eq!(header.alg, Algorithm::RS256);
        assert_eq!(header.kid.as_deref(), Some("test-key-id"));
        assert_eq!(header.typ.as_deref(), Some("JWT"));

        // Payload — decode with public key
        let decoding_key = DecodingKey::from_rsa_pem(test_key().public_pem.as_bytes()).unwrap();
        let mut validation = Validation::new(Algorithm::RS256);
        validation.set_audience(&["https://api.line.me/"]);
        validation.validate_exp = false; // test-fixed time
        let data = decode::<JwtClaims>(&jwt, &decoding_key, &validation).unwrap();
        let c = data.claims;

        assert_eq!(c.iss, "test-channel-id");
        assert_eq!(c.sub, "test-channel-id");
        assert_eq!(c.aud, "https://api.line.me/");
        assert_eq!(c.token_type, "Bearer");

        // exp: absolute timestamp, now + 30 min
        assert_eq!(c.exp, now + 1800);

        // token_exp: duration in seconds, must satisfy LINE's 30s ≤ x ≤ 2592000
        assert!((30..=2_592_000).contains(&c.token_exp));
    }

    #[test]
    fn build_jwt_assertion_rejects_invalid_pem() {
        let config = LineConfig {
            channel_id: "x".into(),
            channel_secret: "x".into(),
            key_id: "x".into(),
            private_key: "not a pem".into(),
        };
        let err = build_jwt_assertion(&config, 0).unwrap_err();
        assert!(matches!(err, LineError::Jwt(_)));
    }

    // --- get_access_token ---

    #[tokio::test]
    async fn get_access_token_success_caches() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/oauth2/v2.1/token"))
            .and(body_string_contains("grant_type=client_credentials"))
            .and(body_string_contains("client_assertion="))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "cached-token",
                "expires_in": 2_592_000u64,
                "token_type": "Bearer",
            })))
            .expect(1) // 2回目はキャッシュで mock 呼ばれない
            .mount(&server)
            .await;

        let client = LineClient::with_endpoints(
            format!("{}/oauth2/v2.1/token", server.uri()),
            format!("{}/v2/bot/message/push", server.uri()),
        );

        let t1 = client.get_access_token(&test_config()).await.unwrap();
        let t2 = client.get_access_token(&test_config()).await.unwrap();
        assert_eq!(t1, "cached-token");
        assert_eq!(t2, "cached-token");
    }

    #[tokio::test]
    async fn get_access_token_refreshes_expired_cache() {
        // expires_in=0 → 直後に再呼び出しすると (expires_at > now+60) が false になり、
        // キャッシュがあっても fall-through して新規発行する
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/oauth2/v2.1/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "short-lived",
                "expires_in": 0u64,
                "token_type": "Bearer",
            })))
            .expect(2) // キャッシュが期限切れ扱いなので 2 回とも endpoint が呼ばれる
            .mount(&server)
            .await;

        let client = LineClient::with_endpoints(
            format!("{}/oauth2/v2.1/token", server.uri()),
            "unused".into(),
        );

        let t1 = client.get_access_token(&test_config()).await.unwrap();
        let t2 = client.get_access_token(&test_config()).await.unwrap();
        assert_eq!(t1, "short-lived");
        assert_eq!(t2, "short-lived");
    }

    #[tokio::test]
    async fn get_access_token_400_returns_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/oauth2/v2.1/token"))
            .respond_with(ResponseTemplate::new(400).set_body_string(
                r#"{"error":"invalid_client","error_description":"Invalid token_exp"}"#,
            ))
            .mount(&server)
            .await;

        let client = LineClient::with_endpoints(
            format!("{}/oauth2/v2.1/token", server.uri()),
            "unused".into(),
        );

        let err = client.get_access_token(&test_config()).await.unwrap_err();
        match err {
            LineError::TokenIssueFailed(body) => assert!(body.contains("invalid_client")),
            e => panic!("expected TokenIssueFailed, got {e:?}"),
        }
    }

    #[tokio::test]
    async fn get_access_token_parse_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/oauth2/v2.1/token"))
            .respond_with(ResponseTemplate::new(200).set_body_string("not json"))
            .mount(&server)
            .await;

        let client = LineClient::with_endpoints(
            format!("{}/oauth2/v2.1/token", server.uri()),
            "unused".into(),
        );

        let err = client.get_access_token(&test_config()).await.unwrap_err();
        match err {
            LineError::TokenIssueFailed(body) => assert!(body.contains("parse error")),
            e => panic!("expected TokenIssueFailed parse error, got {e:?}"),
        }
    }

    #[tokio::test]
    async fn get_access_token_http_error() {
        // Unreachable URL → reqwest error
        let client = LineClient::with_endpoints(
            "http://127.0.0.1:1/oauth2/v2.1/token".into(),
            "unused".into(),
        );
        let err = client.get_access_token(&test_config()).await.unwrap_err();
        assert!(matches!(err, LineError::Http(_)));
    }

    #[tokio::test]
    async fn get_access_token_jwt_error_propagates() {
        let client = LineClient::with_endpoints("unused".into(), "unused".into());
        let bad_config = LineConfig {
            channel_id: "x".into(),
            channel_secret: "x".into(),
            key_id: "x".into(),
            private_key: "not a pem".into(),
        };
        let err = client.get_access_token(&bad_config).await.unwrap_err();
        assert!(matches!(err, LineError::Jwt(_)));
    }

    // --- push_text ---

    #[tokio::test]
    async fn push_text_success() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/oauth2/v2.1/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "tok",
                "expires_in": 86400u64,
                "token_type": "Bearer",
            })))
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(path("/v2/bot/message/push"))
            .and(header("Authorization", "Bearer tok"))
            .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
            .mount(&server)
            .await;

        let client = LineClient::with_endpoints(
            format!("{}/oauth2/v2.1/token", server.uri()),
            format!("{}/v2/bot/message/push", server.uri()),
        );

        client
            .push_text(&test_config(), "U123", "hello")
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn push_text_failure_returns_send_failed() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/oauth2/v2.1/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "tok",
                "expires_in": 86400u64,
                "token_type": "Bearer",
            })))
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(path("/v2/bot/message/push"))
            .respond_with(ResponseTemplate::new(400).set_body_string("bad request"))
            .mount(&server)
            .await;

        let client = LineClient::with_endpoints(
            format!("{}/oauth2/v2.1/token", server.uri()),
            format!("{}/v2/bot/message/push", server.uri()),
        );

        let err = client
            .push_text(&test_config(), "U123", "hello")
            .await
            .unwrap_err();
        match err {
            LineError::SendFailed(msg) => {
                assert!(msg.contains("400"));
                assert!(msg.contains("bad request"));
            }
            e => panic!("expected SendFailed, got {e:?}"),
        }
    }

    #[tokio::test]
    async fn push_text_http_error_on_push() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/oauth2/v2.1/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "tok",
                "expires_in": 86400u64,
                "token_type": "Bearer",
            })))
            .mount(&server)
            .await;

        // push endpoint is unreachable → reqwest error
        let client = LineClient::with_endpoints(
            format!("{}/oauth2/v2.1/token", server.uri()),
            "http://127.0.0.1:1/v2/bot/message/push".into(),
        );
        let err = client
            .push_text(&test_config(), "U123", "hello")
            .await
            .unwrap_err();
        assert!(matches!(err, LineError::Http(_)));
    }

    // --- push_image tests (push_text と同じパターン) ---

    #[tokio::test]
    async fn push_image_success() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/oauth2/v2.1/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "tok",
                "expires_in": 86400u64,
                "token_type": "Bearer",
            })))
            .mount(&server)
            .await;

        // request body に image / originalContentUrl / previewImageUrl が乗っている
        // ことを直接 wiremock matcher で検証 (regression guard)
        Mock::given(method("POST"))
            .and(path("/v2/bot/message/push"))
            .and(header("Authorization", "Bearer tok"))
            .and(wiremock::matchers::body_string_contains("\"image\""))
            .and(wiremock::matchers::body_string_contains(
                "\"originalContentUrl\":\"https://example.com/img.jpg\"",
            ))
            .and(wiremock::matchers::body_string_contains(
                "\"previewImageUrl\":\"https://example.com/preview.jpg\"",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
            .expect(1)
            .mount(&server)
            .await;

        let client = LineClient::with_endpoints(
            format!("{}/oauth2/v2.1/token", server.uri()),
            format!("{}/v2/bot/message/push", server.uri()),
        );

        client
            .push_image(
                &test_config(),
                "U123",
                "https://example.com/img.jpg",
                "https://example.com/preview.jpg",
            )
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn push_image_failure_returns_send_failed() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/oauth2/v2.1/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "tok",
                "expires_in": 86400u64,
                "token_type": "Bearer",
            })))
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(path("/v2/bot/message/push"))
            .respond_with(ResponseTemplate::new(400).set_body_string("invalid url"))
            .mount(&server)
            .await;

        let client = LineClient::with_endpoints(
            format!("{}/oauth2/v2.1/token", server.uri()),
            format!("{}/v2/bot/message/push", server.uri()),
        );

        let err = client
            .push_image(&test_config(), "U123", "https://x/o.jpg", "https://x/p.jpg")
            .await
            .unwrap_err();
        match err {
            LineError::SendFailed(msg) => {
                assert!(msg.contains("400"));
                assert!(msg.contains("invalid url"));
            }
            e => panic!("expected SendFailed, got {e:?}"),
        }
    }

    #[tokio::test]
    async fn push_image_http_error_on_push() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/oauth2/v2.1/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "tok",
                "expires_in": 86400u64,
                "token_type": "Bearer",
            })))
            .mount(&server)
            .await;

        let client = LineClient::with_endpoints(
            format!("{}/oauth2/v2.1/token", server.uri()),
            "http://127.0.0.1:1/v2/bot/message/push".into(),
        );
        let err = client
            .push_image(&test_config(), "U123", "https://x/o.jpg", "https://x/p.jpg")
            .await
            .unwrap_err();
        assert!(matches!(err, LineError::Http(_)));
    }

    // --- LineError Display ---

    #[test]
    fn line_error_display() {
        assert_eq!(
            LineError::TokenIssueFailed("x".into()).to_string(),
            "Token issue failed: x"
        );
        assert_eq!(
            LineError::SendFailed("y".into()).to_string(),
            "Send failed: y"
        );
    }

    // --- LineClient constructors ---

    #[test]
    fn new_uses_prod_endpoints() {
        let client = LineClient::new();
        assert_eq!(client.token_endpoint, PROD_TOKEN_ENDPOINT);
        assert_eq!(client.push_endpoint, PROD_PUSH_ENDPOINT);
    }

    #[test]
    fn default_matches_new() {
        let c = LineClient::default();
        assert_eq!(c.token_endpoint, PROD_TOKEN_ENDPOINT);
        assert_eq!(c.push_endpoint, PROD_PUSH_ENDPOINT);
    }
}
