//! LINE Messaging API client (JWT assertion 方式)
//! https://developers.line.biz/ja/docs/messaging-api/generate-json-web-token/

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

const TOKEN_ENDPOINT: &str = "https://api.line.me/oauth2/v2.1/token";

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

#[derive(Debug, Serialize)]
struct JwtClaims {
    iss: String,
    sub: String,
    aud: String,
    exp: u64,
    token_type: String,
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

pub struct LineClient {
    client: reqwest::Client,
    cache: Arc<RwLock<Option<CachedToken>>>,
}

impl Default for LineClient {
    fn default() -> Self {
        Self::new()
    }
}

impl LineClient {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
            cache: Arc::new(RwLock::new(None)),
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

        // JWT 生成
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let claims = JwtClaims {
            iss: config.channel_id.clone(),
            sub: config.channel_id.clone(),
            aud: "https://api.line.me/".to_string(),
            exp: now + 1800, // 30分
            token_type: "Bearer".to_string(),
        };

        let mut header = Header::new(Algorithm::RS256);
        header.kid = Some(config.key_id.clone());
        header.typ = Some("JWT".to_string());

        let key = EncodingKey::from_rsa_pem(config.private_key.as_bytes())?;
        let jwt = encode(&header, &claims, &key)?;

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
            .post(TOKEN_ENDPOINT)
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
            .post("https://api.line.me/v2/bot/message/push")
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
}
