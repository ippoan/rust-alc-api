//! LINE WORKS Bot API client
//! lineworks-bot-rust から auth + send ロジックをポート

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use uuid::Uuid;

const AUTH_TOKEN_ENDPOINT: &str = "https://auth.worksmobile.com/oauth2/v2.0/token";
const BOT_ENDPOINT: &str = "https://www.worksapis.com/v1.0/bots/";

#[derive(Debug, thiserror::Error)]
pub enum LineworksBotError {
    #[error("JWT error: {0}")]
    Jwt(#[from] jsonwebtoken::errors::Error),
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("Token issue failed: {0}")]
    TokenIssueFailed(String),
    #[error("Send failed: {0}")]
    SendFailed(String),
}

/// bot_configs テーブルから取得した設定 (復号済み)
#[derive(Debug, Clone)]
pub struct LineworksBotConfig {
    pub client_id: String,
    pub client_secret: String,
    pub service_account: String,
    pub private_key: String, // PEM format
    pub bot_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct JwtClaims {
    iss: String,
    sub: String,
    iat: u64,
    exp: u64,
}

fn deserialize_expires_in<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::{self, Visitor};
    struct V;
    impl<'de> Visitor<'de> for V {
        type Value = u64;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("string or integer")
        }
        fn visit_u64<E>(self, v: u64) -> Result<u64, E> {
            Ok(v)
        }
        fn visit_str<E: de::Error>(self, v: &str) -> Result<u64, E> {
            v.parse().map_err(de::Error::custom)
        }
    }
    deserializer.deserialize_any(V)
}

#[derive(Debug, Clone, Deserialize)]
struct TokenResponse {
    access_token: String,
    #[allow(dead_code)]
    refresh_token: String,
    #[serde(deserialize_with = "deserialize_expires_in")]
    expires_in: u64,
}

#[derive(Debug)]
struct CachedToken {
    token: TokenResponse,
    issued_at: u64,
}

impl CachedToken {
    fn is_expired(&self) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        self.issued_at + self.token.expires_in <= now
    }
}

/// マルチテナント対応の LINE WORKS Bot クライアント
/// bot_config_id ごとにトークンをキャッシュ
pub struct LineworksBotClient {
    client: reqwest::Client,
    cache: Arc<RwLock<HashMap<Uuid, CachedToken>>>,
}

impl Default for LineworksBotClient {
    fn default() -> Self {
        Self::new()
    }
}

impl LineworksBotClient {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
            cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    async fn get_access_token(
        &self,
        config_id: Uuid,
        config: &LineworksBotConfig,
    ) -> Result<String, LineworksBotError> {
        // キャッシュチェック
        {
            let cache = self.cache.read().await;
            if let Some(cached) = cache.get(&config_id) {
                if !cached.is_expired() {
                    return Ok(cached.token.access_token.clone());
                }
            }
        }

        // 新規トークン発行
        let token = self.issue_token(config).await?;
        let access_token = token.access_token.clone();

        let mut cache = self.cache.write().await;
        cache.insert(
            config_id,
            CachedToken {
                token,
                issued_at: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
            },
        );

        Ok(access_token)
    }

    async fn issue_token(
        &self,
        config: &LineworksBotConfig,
    ) -> Result<TokenResponse, LineworksBotError> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let claims = JwtClaims {
            iss: config.client_id.clone(),
            sub: config.service_account.clone(),
            iat: now,
            exp: now + 60,
        };

        let key = EncodingKey::from_rsa_pem(config.private_key.as_bytes())?;
        let jwt = encode(&Header::new(Algorithm::RS256), &claims, &key)?;

        let params = [
            ("assertion", jwt),
            (
                "grant_type",
                "urn:ietf:params:oauth:grant-type:jwt-bearer".to_string(),
            ),
            ("client_id", config.client_id.clone()),
            ("client_secret", config.client_secret.clone()),
            ("scope", "bot".to_string()),
        ];

        let resp = self
            .client
            .post(AUTH_TOKEN_ENDPOINT)
            .form(&params)
            .send()
            .await?;

        let status = resp.status();
        let body = resp.text().await?;

        if !status.is_success() {
            tracing::error!("LINE WORKS token issue failed: {status} - {body}");
            return Err(LineworksBotError::TokenIssueFailed(body));
        }

        serde_json::from_str(&body).map_err(|e| {
            LineworksBotError::TokenIssueFailed(format!("parse error: {e} - body: {body}"))
        })
    }

    /// ユーザーにテキストメッセージを送信
    pub async fn send_text_to_user(
        &self,
        config_id: Uuid,
        config: &LineworksBotConfig,
        user_id: &str,
        text: &str,
    ) -> Result<(), LineworksBotError> {
        let token = self.get_access_token(config_id, config).await?;
        let url = format!(
            "{}{}/users/{}/messages",
            BOT_ENDPOINT, config.bot_id, user_id
        );

        let body = serde_json::json!({
            "content": {
                "type": "text",
                "text": text,
            }
        });

        let resp = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", token))
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            tracing::error!("LINE WORKS send failed: {status} - {body}");
            return Err(LineworksBotError::SendFailed(format!("{status}: {body}")));
        }

        Ok(())
    }
}
