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
const USERS_ENDPOINT: &str = "https://www.worksapis.com/v1.0/users";

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

/// LINE WORKS API レスポンスから変換した組織メンバー
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineworksMember {
    pub user_id: String,
    pub user_name: Option<String>,
    pub email: Option<String>,
}

/// LINE WORKS Users API レスポンス
#[derive(Debug, Deserialize)]
struct UsersResponse {
    users: Option<Vec<UserEntry>>,
    #[serde(rename = "responseMetaData")]
    response_meta_data: Option<ResponseMetaData>,
}

#[derive(Debug, Deserialize)]
struct UserEntry {
    #[serde(rename = "userId")]
    user_id: String,
    #[serde(rename = "userName")]
    user_name: Option<UserName>,
    email: Option<String>,
}

#[derive(Debug, Deserialize)]
struct UserName {
    #[serde(rename = "lastName")]
    last_name: Option<String>,
    #[serde(rename = "firstName")]
    first_name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ResponseMetaData {
    cursor: Option<String>,
}

/// マルチテナント対応の LINE WORKS Bot クライアント
/// (config_id, scope) ごとにトークンをキャッシュ
pub struct LineworksBotClient {
    client: reqwest::Client,
    cache: Arc<RwLock<HashMap<(Uuid, String), CachedToken>>>,
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
        scope: &str,
    ) -> Result<String, LineworksBotError> {
        let cache_key = (config_id, scope.to_string());
        // キャッシュチェック
        {
            let cache = self.cache.read().await;
            if let Some(cached) = cache.get(&cache_key) {
                if !cached.is_expired() {
                    return Ok(cached.token.access_token.clone());
                }
            }
        }

        // 新規トークン発行
        let token = self.issue_token(config, scope).await?;
        let access_token = token.access_token.clone();

        let mut cache = self.cache.write().await;
        cache.insert(
            cache_key,
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
        scope: &str,
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
            ("scope", scope.to_string()),
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
        let token = self.get_access_token(config_id, config, "bot").await?;
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

    /// 組織メンバー一覧を取得 (directory.read scope)
    pub async fn list_org_users(
        &self,
        config_id: Uuid,
        config: &LineworksBotConfig,
    ) -> Result<Vec<LineworksMember>, LineworksBotError> {
        let token = self
            .get_access_token(config_id, config, "directory.read")
            .await?;

        let mut members = Vec::new();
        let mut cursor: Option<String> = None;

        loop {
            let mut url = USERS_ENDPOINT.to_string();
            if let Some(ref c) = cursor {
                url = format!("{}?cursor={}", url, c);
            }

            let resp = self
                .client
                .get(&url)
                .header("Authorization", format!("Bearer {}", token))
                .send()
                .await?;

            if !resp.status().is_success() {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                tracing::error!("LINE WORKS list users failed: {status} - {body}");
                return Err(LineworksBotError::SendFailed(format!("{status}: {body}")));
            }

            let body: UsersResponse = resp
                .json()
                .await
                .map_err(|e| LineworksBotError::SendFailed(format!("parse users response: {e}")))?;

            if let Some(users) = body.users {
                for u in users {
                    let display_name = u.user_name.map(|n| {
                        let last = n.last_name.unwrap_or_default();
                        let first = n.first_name.unwrap_or_default();
                        format!("{} {}", last, first).trim().to_string()
                    });
                    members.push(LineworksMember {
                        user_id: u.user_id,
                        user_name: display_name,
                        email: u.email,
                    });
                }
            }

            cursor = body.response_meta_data.and_then(|m| m.cursor);
            if cursor.is_none() {
                break;
            }
        }

        Ok(members)
    }
}
