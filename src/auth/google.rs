use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation};
use reqwest::Client;
use serde::Deserialize;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Google JWKS キャッシュの有効期限 (秒)
fn jwks_cache_ttl_secs() -> u64 {
    std::env::var("GOOGLE_JWKS_CACHE_TTL_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(3600)
}
fn google_jwks_url() -> String {
    std::env::var("GOOGLE_JWKS_URL")
        .unwrap_or_else(|_| "https://www.googleapis.com/oauth2/v3/certs".to_string())
}
const GOOGLE_ISSUER: &str = "https://accounts.google.com";

/// Google ID トークンから抽出するクレーム
#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct GoogleClaims {
    pub sub: String,
    pub email: String,
    #[serde(default)]
    pub name: String,
    pub picture: Option<String>,
    #[serde(default)]
    pub email_verified: bool,
    pub aud: String,
    pub iss: String,
    pub exp: u64,
}

/// Google JWKS のキー
#[derive(Debug, Deserialize, Clone)]
struct JwkKey {
    kid: String,
    n: String,
    e: String,
    #[allow(dead_code)]
    #[serde(default)]
    kty: String,
    #[allow(dead_code)]
    #[serde(default)]
    alg: String,
}

#[derive(Debug, Deserialize)]
struct JwksResponse {
    keys: Vec<JwkKey>,
}

struct CachedJwks {
    keys: Vec<JwkKey>,
    fetched_at: std::time::Instant,
}

/// Google OAuth token endpoint
fn google_token_url() -> String {
    std::env::var("GOOGLE_TOKEN_URL")
        .unwrap_or_else(|_| "https://oauth2.googleapis.com/token".to_string())
}

/// Google token endpoint のレスポンス
#[derive(Debug, Deserialize)]
struct GoogleTokenResponse {
    id_token: String,
}

/// Google ID トークン検証器
#[derive(Clone)]
pub struct GoogleTokenVerifier {
    client_id: String,
    client_secret: String,
    http_client: Client,
    jwks_cache: Arc<RwLock<Option<CachedJwks>>>,
    /// テスト用: Some の場合、verify/exchange_code で固定 claims を返す
    test_claims: Option<Arc<GoogleClaims>>,
}

impl GoogleTokenVerifier {
    pub fn new(client_id: String, client_secret: String) -> Self {
        Self {
            client_id,
            client_secret,
            http_client: Client::new(),
            jwks_cache: Arc::new(RwLock::new(None)),
            test_claims: None,
        }
    }

    /// テスト用: verify/exchange_code で固定 claims を返す verifier を作成
    pub fn with_test_claims(client_id: String, claims: GoogleClaims) -> Self {
        Self {
            client_id,
            client_secret: String::new(),
            http_client: Client::new(),
            jwks_cache: Arc::new(RwLock::new(None)),
            test_claims: Some(Arc::new(claims)),
        }
    }

    pub fn client_id(&self) -> &str {
        &self.client_id
    }

    /// Authorization code を Google token endpoint で交換し、ID token を検証して claims を返す
    pub async fn exchange_code(
        &self,
        code: &str,
        redirect_uri: &str,
    ) -> Result<GoogleClaims, VerifyError> {
        // テストモード
        if let Some(ref claims) = self.test_claims {
            if code == "test-valid-code" {
                return Ok((**claims).clone());
            }
            return Err(VerifyError::TokenExchangeFailed);
        }

        let resp = self
            .http_client
            .post(google_token_url())
            .form(&[
                ("code", code),
                ("client_id", &self.client_id),
                ("client_secret", &self.client_secret),
                ("redirect_uri", redirect_uri),
                ("grant_type", "authorization_code"),
            ])
            .send()
            .await
            .map_err(|e| {
                tracing::warn!("Google token exchange request failed: {e}");
                VerifyError::TokenExchangeFailed
            })?;

        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            tracing::warn!("Google token exchange failed: {body}");
            return Err(VerifyError::TokenExchangeFailed);
        }

        let token_resp: GoogleTokenResponse = resp.json().await.map_err(|e| {
            tracing::warn!("Failed to parse Google token response: {e}");
            VerifyError::TokenExchangeFailed
        })?;

        self.verify(&token_resp.id_token).await
    }

    /// Google ID トークンを検証し、クレームを返す
    pub async fn verify(&self, id_token: &str) -> Result<GoogleClaims, VerifyError> {
        // テストモード: 固定 claims を返す
        if let Some(ref claims) = self.test_claims {
            if id_token == "test-valid-token" {
                return Ok((**claims).clone());
            }
            return Err(VerifyError::InvalidToken);
        }

        // ヘッダーから kid を取得
        let header = decode_header(id_token).map_err(|_| VerifyError::InvalidToken)?;
        let kid = header.kid.ok_or(VerifyError::InvalidToken)?;

        // JWKS からマッチするキーを取得
        let key = self.get_key(&kid).await?;

        // デコードキーを構築
        let decoding_key = DecodingKey::from_rsa_components(&key.n, &key.e)
            .map_err(|_| VerifyError::InvalidKey)?;

        // 検証パラメータ
        let mut validation = Validation::new(Algorithm::RS256);
        validation.set_issuer(&[GOOGLE_ISSUER, "accounts.google.com"]);
        validation.set_audience(&[&self.client_id]);

        // デコード + 検証
        let token_data =
            decode::<GoogleClaims>(id_token, &decoding_key, &validation).map_err(|e| {
                tracing::warn!("Google ID token verification failed: {e}");
                VerifyError::InvalidToken
            })?;

        let claims = token_data.claims;

        // email_verified チェック
        if !claims.email_verified {
            return Err(VerifyError::EmailNotVerified);
        }

        Ok(claims)
    }

    /// キャッシュから kid に一致するキーを検索
    async fn find_cached_key(&self, kid: &str) -> Option<JwkKey> {
        let cache = self.jwks_cache.read().await;
        let cached = cache.as_ref()?;
        if cached.fetched_at.elapsed().as_secs() >= jwks_cache_ttl_secs() {
            return None;
        }
        cached.keys.iter().find(|k| k.kid == kid).cloned()
    }

    /// JWKS から kid に一致するキーを取得 (キャッシュ付き)
    async fn get_key(&self, kid: &str) -> Result<JwkKey, VerifyError> {
        // キャッシュ確認
        if let Some(key) = self.find_cached_key(kid).await {
            return Ok(key);
        }

        // キャッシュミスまたは期限切れ — JWKS を取得
        let resp = self
            .http_client
            .get(google_jwks_url())
            .send()
            .await
            .map_err(|_| VerifyError::JwksFetchFailed)?;

        let jwks: JwksResponse = resp
            .json()
            .await
            .map_err(|_| VerifyError::JwksFetchFailed)?;

        let key = jwks
            .keys
            .iter()
            .find(|k| k.kid == kid)
            .cloned()
            .ok_or(VerifyError::KeyNotFound)?;

        // キャッシュ更新
        {
            let mut cache = self.jwks_cache.write().await;
            *cache = Some(CachedJwks {
                keys: jwks.keys,
                fetched_at: std::time::Instant::now(),
            });
        }

        Ok(key)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum VerifyError {
    #[error("invalid token")]
    InvalidToken,
    #[error("invalid key")]
    InvalidKey,
    #[error("email not verified")]
    EmailNotVerified,
    #[error("failed to fetch JWKS")]
    JwksFetchFailed,
    #[error("key not found in JWKS")]
    KeyNotFound,
    #[error("failed to exchange authorization code")]
    TokenExchangeFailed,
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    use jsonwebtoken::{encode, EncodingKey, Header};
    use rsa::pkcs1::EncodeRsaPrivateKey;
    use rsa::rand_core::OsRng;
    use rsa::traits::PublicKeyParts;
    use rsa::{RsaPrivateKey, RsaPublicKey};
    use serde_json::json;
    use std::sync::{Mutex, OnceLock};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    static ENV_LOCK: Mutex<()> = Mutex::new(());
    const TEST_KID: &str = "test-kid-1";

    /// 鍵ペア + JWK コンポーネント (n, e) + PEM をキャッシュ
    struct TestKeyMaterial {
        pem: Vec<u8>,
        n: String,
        e: String,
    }

    fn test_key() -> &'static TestKeyMaterial {
        static KEY: OnceLock<TestKeyMaterial> = OnceLock::new();
        KEY.get_or_init(|| {
            let private_key = RsaPrivateKey::new(&mut OsRng, 2048).unwrap();
            let public_key = RsaPublicKey::from(&private_key);
            let pem = private_key
                .to_pkcs1_pem(rsa::pkcs1::LineEnding::LF)
                .unwrap()
                .as_bytes()
                .to_vec();
            let n = URL_SAFE_NO_PAD.encode(public_key.n().to_bytes_be());
            let e = URL_SAFE_NO_PAD.encode(public_key.e().to_bytes_be());
            TestKeyMaterial { pem, n, e }
        })
    }

    fn build_jwks_response(n: &str, e: &str) -> serde_json::Value {
        json!({"keys": [{"kid": TEST_KID, "kty": "RSA", "alg": "RS256", "use": "sig", "n": n, "e": e}]})
    }

    fn create_test_jwt(claims: &GoogleClaims, kid: &str) -> String {
        let key = test_key();
        let encoding_key = EncodingKey::from_rsa_pem(&key.pem).unwrap();
        let mut header = Header::new(Algorithm::RS256);
        header.kid = Some(kid.to_string());
        encode(&header, claims, &encoding_key).unwrap()
    }

    fn test_claims(client_id: &str) -> GoogleClaims {
        GoogleClaims {
            sub: "12345".to_string(),
            email: "test@example.com".to_string(),
            name: "Test User".to_string(),
            picture: None,
            email_verified: true,
            aud: client_id.to_string(),
            iss: GOOGLE_ISSUER.to_string(),
            exp: (chrono::Utc::now().timestamp() + 3600) as u64,
        }
    }

    // --- env var helpers ---

    #[test]
    fn test_jwks_cache_ttl_default() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::remove_var("GOOGLE_JWKS_CACHE_TTL_SECS");
        assert_eq!(jwks_cache_ttl_secs(), 3600);
    }

    #[test]
    fn test_jwks_cache_ttl_custom() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("GOOGLE_JWKS_CACHE_TTL_SECS", "60");
        assert_eq!(jwks_cache_ttl_secs(), 60);
        std::env::remove_var("GOOGLE_JWKS_CACHE_TTL_SECS");
    }

    #[test]
    fn test_google_jwks_url_default() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::remove_var("GOOGLE_JWKS_URL");
        assert_eq!(
            google_jwks_url(),
            "https://www.googleapis.com/oauth2/v3/certs"
        );
    }

    #[test]
    fn test_google_jwks_url_custom() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("GOOGLE_JWKS_URL", "http://custom/jwks");
        assert_eq!(google_jwks_url(), "http://custom/jwks");
        std::env::remove_var("GOOGLE_JWKS_URL");
    }

    #[test]
    fn test_google_token_url_default() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::remove_var("GOOGLE_TOKEN_URL");
        assert_eq!(google_token_url(), "https://oauth2.googleapis.com/token");
    }

    #[test]
    fn test_google_token_url_custom() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("GOOGLE_TOKEN_URL", "http://custom/token");
        assert_eq!(google_token_url(), "http://custom/token");
        std::env::remove_var("GOOGLE_TOKEN_URL");
    }

    // --- GoogleTokenVerifier::new ---

    #[test]
    fn test_verifier_new() {
        let v = GoogleTokenVerifier::new("cid".into(), "csec".into());
        assert_eq!(v.client_id(), "cid");
        assert!(v.test_claims.is_none());
    }

    // --- verify with wiremock ---

    #[tokio::test]
    async fn test_verify_success() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let server = MockServer::start().await;
        std::env::set_var("GOOGLE_JWKS_URL", format!("{}/jwks", server.uri()));

        let client_id = "test-client-id";
        let key = test_key();
        let (n, e) = (&key.n, &key.e);
        let claims = test_claims(client_id);
        let token = create_test_jwt(&claims, TEST_KID);

        Mock::given(method("GET"))
            .and(path("/jwks"))
            .respond_with(ResponseTemplate::new(200).set_body_json(build_jwks_response(&n, &e)))
            .mount(&server)
            .await;

        let verifier = GoogleTokenVerifier::new(client_id.into(), "secret".into());
        let result = verifier.verify(&token).await;
        assert!(result.is_ok());
        let c = result.unwrap();
        assert_eq!(c.email, "test@example.com");

        std::env::remove_var("GOOGLE_JWKS_URL");
    }

    #[tokio::test]
    async fn test_verify_invalid_token() {
        let verifier = GoogleTokenVerifier::new("cid".into(), "csec".into());
        let result = verifier.verify("not-a-jwt").await;
        assert!(matches!(result, Err(VerifyError::InvalidToken)));
    }

    #[tokio::test]
    async fn test_verify_email_not_verified() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let server = MockServer::start().await;
        std::env::set_var("GOOGLE_JWKS_URL", format!("{}/jwks", server.uri()));

        let client_id = "test-client-id";
        let key = test_key();
        let (n, e) = (&key.n, &key.e);
        let mut claims = test_claims(client_id);
        claims.email_verified = false;
        let token = create_test_jwt(&claims, TEST_KID);

        Mock::given(method("GET"))
            .and(path("/jwks"))
            .respond_with(ResponseTemplate::new(200).set_body_json(build_jwks_response(&n, &e)))
            .mount(&server)
            .await;

        let verifier = GoogleTokenVerifier::new(client_id.into(), "secret".into());
        let result = verifier.verify(&token).await;
        assert!(matches!(result, Err(VerifyError::EmailNotVerified)));

        std::env::remove_var("GOOGLE_JWKS_URL");
    }

    #[tokio::test]
    async fn test_verify_kid_not_found() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let server = MockServer::start().await;
        std::env::set_var("GOOGLE_JWKS_URL", format!("{}/jwks", server.uri()));

        let client_id = "test-client-id";
        let claims = test_claims(client_id);
        let token = create_test_jwt(&claims, "wrong-kid");

        // JWKS with different kid
        Mock::given(method("GET"))
            .and(path("/jwks"))
            .respond_with(ResponseTemplate::new(200).set_body_json(
                json!({"keys": [{"kid": "other-kid", "n": "abc", "e": "AQAB", "kty": "RSA"}]}),
            ))
            .mount(&server)
            .await;

        let verifier = GoogleTokenVerifier::new(client_id.into(), "secret".into());
        let result = verifier.verify(&token).await;
        assert!(matches!(result, Err(VerifyError::KeyNotFound)));

        std::env::remove_var("GOOGLE_JWKS_URL");
    }

    #[tokio::test]
    async fn test_verify_invalid_key_components() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let server = MockServer::start().await;
        std::env::set_var("GOOGLE_JWKS_URL", format!("{}/jwks", server.uri()));

        let client_id = "test-client-id";
        let claims = test_claims(client_id);
        let token = create_test_jwt(&claims, TEST_KID);

        // JWKS with invalid n/e
        Mock::given(method("GET"))
            .and(path("/jwks"))
            .respond_with(ResponseTemplate::new(200).set_body_json(
                json!({"keys": [{"kid": TEST_KID, "n": "", "e": "", "kty": "RSA"}]}),
            ))
            .mount(&server)
            .await;

        let verifier = GoogleTokenVerifier::new(client_id.into(), "secret".into());
        let result = verifier.verify(&token).await;
        assert!(matches!(
            result,
            Err(VerifyError::InvalidKey | VerifyError::InvalidToken)
        ));

        std::env::remove_var("GOOGLE_JWKS_URL");
    }

    // --- JWKS cache ---

    #[tokio::test]
    async fn test_jwks_cache_hit() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let server = MockServer::start().await;
        std::env::set_var("GOOGLE_JWKS_URL", format!("{}/jwks", server.uri()));
        std::env::set_var("GOOGLE_JWKS_CACHE_TTL_SECS", "3600");

        let client_id = "test-client-id";
        let key = test_key();
        let (n, e) = (&key.n, &key.e);
        let claims = test_claims(client_id);
        let token = create_test_jwt(&claims, TEST_KID);

        Mock::given(method("GET"))
            .and(path("/jwks"))
            .respond_with(ResponseTemplate::new(200).set_body_json(build_jwks_response(&n, &e)))
            .expect(1) // JWKS should be fetched only once
            .mount(&server)
            .await;

        let verifier = GoogleTokenVerifier::new(client_id.into(), "secret".into());
        // First call — cache miss, fetches JWKS
        let r1 = verifier.verify(&token).await;
        assert!(r1.is_ok());
        // Second call — cache hit, no fetch
        let token2 = create_test_jwt(&test_claims(client_id), TEST_KID);
        let r2 = verifier.verify(&token2).await;
        assert!(r2.is_ok());

        std::env::remove_var("GOOGLE_JWKS_URL");
        std::env::remove_var("GOOGLE_JWKS_CACHE_TTL_SECS");
    }

    #[tokio::test]
    async fn test_jwks_cache_expired() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let server = MockServer::start().await;
        std::env::set_var("GOOGLE_JWKS_URL", format!("{}/jwks", server.uri()));
        std::env::set_var("GOOGLE_JWKS_CACHE_TTL_SECS", "0"); // immediate expiry

        let client_id = "test-client-id";
        let key = test_key();
        let (n, e) = (&key.n, &key.e);

        Mock::given(method("GET"))
            .and(path("/jwks"))
            .respond_with(ResponseTemplate::new(200).set_body_json(build_jwks_response(&n, &e)))
            .expect(2) // Should fetch twice due to 0 TTL
            .mount(&server)
            .await;

        let verifier = GoogleTokenVerifier::new(client_id.into(), "secret".into());
        let token1 = create_test_jwt(&test_claims(client_id), TEST_KID);
        let r1 = verifier.verify(&token1).await;
        assert!(r1.is_ok());
        // Cache expired immediately, will fetch again
        let token2 = create_test_jwt(&test_claims(client_id), TEST_KID);
        let r2 = verifier.verify(&token2).await;
        assert!(r2.is_ok());

        std::env::remove_var("GOOGLE_JWKS_URL");
        std::env::remove_var("GOOGLE_JWKS_CACHE_TTL_SECS");
    }

    // --- exchange_code with wiremock ---

    #[tokio::test]
    async fn test_exchange_code_success() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let server = MockServer::start().await;
        std::env::set_var("GOOGLE_TOKEN_URL", format!("{}/token", server.uri()));
        std::env::set_var("GOOGLE_JWKS_URL", format!("{}/jwks", server.uri()));

        let client_id = "test-client-id";
        let key = test_key();
        let (n, e) = (&key.n, &key.e);
        let claims = test_claims(client_id);
        let id_token = create_test_jwt(&claims, TEST_KID);

        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"id_token": id_token})))
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path("/jwks"))
            .respond_with(ResponseTemplate::new(200).set_body_json(build_jwks_response(&n, &e)))
            .mount(&server)
            .await;

        let verifier = GoogleTokenVerifier::new(client_id.into(), "secret".into());
        let result = verifier.exchange_code("auth-code", "http://redirect").await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().email, "test@example.com");

        std::env::remove_var("GOOGLE_TOKEN_URL");
        std::env::remove_var("GOOGLE_JWKS_URL");
    }

    #[tokio::test]
    async fn test_exchange_code_token_endpoint_error() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let server = MockServer::start().await;
        std::env::set_var("GOOGLE_TOKEN_URL", format!("{}/token", server.uri()));

        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(400).set_body_string("bad request"))
            .mount(&server)
            .await;

        let verifier = GoogleTokenVerifier::new("cid".into(), "csec".into());
        let result = verifier.exchange_code("bad-code", "http://redirect").await;
        assert!(matches!(result, Err(VerifyError::TokenExchangeFailed)));

        std::env::remove_var("GOOGLE_TOKEN_URL");
    }

    #[tokio::test]
    async fn test_exchange_code_invalid_json_response() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let server = MockServer::start().await;
        std::env::set_var("GOOGLE_TOKEN_URL", format!("{}/token", server.uri()));

        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(200).set_body_string("not json"))
            .mount(&server)
            .await;

        let verifier = GoogleTokenVerifier::new("cid".into(), "csec".into());
        let result = verifier.exchange_code("code", "http://redirect").await;
        assert!(matches!(result, Err(VerifyError::TokenExchangeFailed)));

        std::env::remove_var("GOOGLE_TOKEN_URL");
    }

    #[tokio::test]
    async fn test_exchange_code_connection_error() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("GOOGLE_TOKEN_URL", "http://127.0.0.1:1/token");

        let verifier = GoogleTokenVerifier::new("cid".into(), "csec".into());
        let result = verifier.exchange_code("code", "http://redirect").await;
        assert!(matches!(result, Err(VerifyError::TokenExchangeFailed)));

        std::env::remove_var("GOOGLE_TOKEN_URL");
    }

    #[tokio::test]
    async fn test_jwks_fetch_failed() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("GOOGLE_JWKS_URL", "http://127.0.0.1:1/jwks");

        let verifier = GoogleTokenVerifier::new("cid".into(), "csec".into());
        let claims = test_claims("cid");
        let token = create_test_jwt(&claims, TEST_KID);
        let result = verifier.verify(&token).await;
        assert!(matches!(result, Err(VerifyError::JwksFetchFailed)));

        std::env::remove_var("GOOGLE_JWKS_URL");
    }

    #[test]
    fn test_verify_error_display() {
        assert_eq!(VerifyError::InvalidToken.to_string(), "invalid token");
        assert_eq!(VerifyError::InvalidKey.to_string(), "invalid key");
        assert_eq!(
            VerifyError::EmailNotVerified.to_string(),
            "email not verified"
        );
        assert_eq!(
            VerifyError::JwksFetchFailed.to_string(),
            "failed to fetch JWKS"
        );
        assert_eq!(
            VerifyError::KeyNotFound.to_string(),
            "key not found in JWKS"
        );
        assert_eq!(
            VerifyError::TokenExchangeFailed.to_string(),
            "failed to exchange authorization code"
        );
    }
}
