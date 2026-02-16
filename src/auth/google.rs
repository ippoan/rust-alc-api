use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation};
use reqwest::Client;
use serde::Deserialize;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Google JWKS キャッシュの有効期限 (秒)
const JWKS_CACHE_TTL_SECS: u64 = 3600;
const GOOGLE_JWKS_URL: &str = "https://www.googleapis.com/oauth2/v3/certs";
const GOOGLE_ISSUER: &str = "https://accounts.google.com";

/// Google ID トークンから抽出するクレーム
#[derive(Debug, Deserialize)]
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
    #[serde(default)]
    kty: String,
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

/// Google ID トークン検証器
#[derive(Clone)]
pub struct GoogleTokenVerifier {
    client_id: String,
    http_client: Client,
    jwks_cache: Arc<RwLock<Option<CachedJwks>>>,
}

impl GoogleTokenVerifier {
    pub fn new(client_id: String) -> Self {
        Self {
            client_id,
            http_client: Client::new(),
            jwks_cache: Arc::new(RwLock::new(None)),
        }
    }

    /// Google ID トークンを検証し、クレームを返す
    pub async fn verify(&self, id_token: &str) -> Result<GoogleClaims, VerifyError> {
        // ヘッダーから kid を取得
        let header = decode_header(id_token).map_err(|_| VerifyError::InvalidToken)?;
        let kid = header.kid.ok_or(VerifyError::InvalidToken)?;

        // JWKS からマッチするキーを取得
        let key = self.get_key(&kid).await?;

        // デコードキーを構築
        let decoding_key =
            DecodingKey::from_rsa_components(&key.n, &key.e).map_err(|_| VerifyError::InvalidKey)?;

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

    /// JWKS から kid に一致するキーを取得 (キャッシュ付き)
    async fn get_key(&self, kid: &str) -> Result<JwkKey, VerifyError> {
        // キャッシュ確認
        {
            let cache = self.jwks_cache.read().await;
            if let Some(cached) = cache.as_ref() {
                if cached.fetched_at.elapsed().as_secs() < JWKS_CACHE_TTL_SECS {
                    if let Some(key) = cached.keys.iter().find(|k| k.kid == kid) {
                        return Ok(key.clone());
                    }
                }
            }
        }

        // キャッシュミスまたは期限切れ — JWKS を取得
        let resp = self
            .http_client
            .get(GOOGLE_JWKS_URL)
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
}
