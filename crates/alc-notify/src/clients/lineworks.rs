//! LINE WORKS Bot API client
//! lineworks-bot-rust から auth + send ロジックをポート

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::{DateTime, FixedOffset, NaiveDateTime, TimeZone, Utc};
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use uuid::Uuid;

const AUTH_TOKEN_ENDPOINT: &str = "https://auth.worksmobile.com/oauth2/v2.0/token";
const BOT_ENDPOINT: &str = "https://www.worksapis.com/v1.0/bots/";
const USERS_ENDPOINT: &str = "https://www.worksapis.com/v1.0/users";
const AUDIT_LOG_DOWNLOAD_ENDPOINT: &str = "https://www.worksapis.com/v1.0/audits/logs/download";

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
    #[serde(rename = "nextCursor")]
    next_cursor: Option<String>,
}

/// マルチテナント対応の LINE WORKS Bot クライアント
/// (config_id, scope) ごとにトークンをキャッシュ
pub struct LineworksBotClient {
    client: reqwest::Client,
    /// 監査ログダウンロード用: リダイレクトを自動追従しない client。
    /// 302 → `Location` へ手動で `Authorization` を付け直して進むために使う
    /// (`get_with_auth_redirect` 参照、Refs #540)。
    no_redirect_client: reqwest::Client,
    cache: Arc<RwLock<HashMap<(Uuid, String), CachedToken>>>,
    bot_endpoint: String,
    auth_token_endpoint: String,
    audit_log_download_endpoint: String,
}

impl Default for LineworksBotClient {
    fn default() -> Self {
        Self::new()
    }
}

impl LineworksBotClient {
    pub fn new() -> Self {
        Self::with_endpoints(BOT_ENDPOINT, AUTH_TOKEN_ENDPOINT)
    }

    /// Test/staging 用にエンドポイントを差し替えるコンストラクタ。
    /// `bot_endpoint` は末尾スラッシュ付き (例: `https://www.worksapis.com/v1.0/bots/`)。
    pub fn with_endpoints(bot_endpoint: &str, auth_token_endpoint: &str) -> Self {
        Self::with_all_endpoints(
            bot_endpoint,
            auth_token_endpoint,
            AUDIT_LOG_DOWNLOAD_ENDPOINT,
        )
    }

    /// `with_endpoints` に加えて監査ログダウンロード endpoint も差し替えるコンストラクタ
    /// (Refs #540、`fetch_login_audit_csv` のテスト用)。
    pub fn with_all_endpoints(
        bot_endpoint: &str,
        auth_token_endpoint: &str,
        audit_log_download_endpoint: &str,
    ) -> Self {
        Self {
            client: reqwest::Client::new(),
            no_redirect_client: reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .expect("build reqwest client with redirect disabled"),
            cache: Arc::new(RwLock::new(HashMap::new())),
            bot_endpoint: bot_endpoint.to_string(),
            auth_token_endpoint: auth_token_endpoint.to_string(),
            audit_log_download_endpoint: audit_log_download_endpoint.to_string(),
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
            .post(&self.auth_token_endpoint)
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
            self.bot_endpoint, config.bot_id, user_id
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

    /// チャネル/グループ (招待された トークルーム) にテキストメッセージを送信。
    /// `POST /v1.0/bots/{botId}/channels/{channelId}/messages` を叩く。
    pub async fn send_text_to_channel(
        &self,
        config_id: Uuid,
        config: &LineworksBotConfig,
        channel_id: &str,
        text: &str,
    ) -> Result<(), LineworksBotError> {
        let token = self.get_access_token(config_id, config, "bot").await?;
        let url = format!(
            "{}{}/channels/{}/messages",
            self.bot_endpoint, config.bot_id, channel_id
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
            tracing::error!("LINE WORKS send_text_to_channel failed: {status} - {body}");
            return Err(LineworksBotError::SendFailed(format!("{status}: {body}")));
        }

        Ok(())
    }

    /// ユーザーに画像メッセージを送信。
    ///
    /// LINE WORKS Bot API は `content.type=image` + originalContentUrl + previewImageUrl
    /// (LINE Messaging API と同じく `previewImageUrl`、age 付き)。
    /// 旧コメントでは `previewImgUrl` (age 無し) と書いていたが、実 API は 400
    /// `INVALID_PARAMETER: content.previewImageUrl/originalContentUrl is required`
    /// を返す。staging ログで判明した実装ミスを修正済 (PR #330 以降)。LINE WORKS サーバー
    /// が URL を fetch して CDN 化、トークルームに inline 表示する。
    /// 仕様: <https://developers.worksmobile.com/jp/docs/bot-send-image>
    pub async fn send_image_to_user(
        &self,
        config_id: Uuid,
        config: &LineworksBotConfig,
        user_id: &str,
        original_url: &str,
        preview_url: &str,
    ) -> Result<(), LineworksBotError> {
        let token = self.get_access_token(config_id, config, "bot").await?;
        let url = format!(
            "{}{}/users/{}/messages",
            self.bot_endpoint, config.bot_id, user_id
        );

        let body = serde_json::json!({
            "content": {
                "type": "image",
                "originalContentUrl": original_url,
                "previewImageUrl": preview_url,
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
            tracing::error!("LINE WORKS send_image_to_user failed: {status} - {body}");
            return Err(LineworksBotError::SendFailed(format!("{status}: {body}")));
        }

        Ok(())
    }

    /// チャネル/グループに画像メッセージを送信。
    pub async fn send_image_to_channel(
        &self,
        config_id: Uuid,
        config: &LineworksBotConfig,
        channel_id: &str,
        original_url: &str,
        preview_url: &str,
    ) -> Result<(), LineworksBotError> {
        let token = self.get_access_token(config_id, config, "bot").await?;
        let url = format!(
            "{}{}/channels/{}/messages",
            self.bot_endpoint, config.bot_id, channel_id
        );

        let body = serde_json::json!({
            "content": {
                "type": "image",
                "originalContentUrl": original_url,
                "previewImageUrl": preview_url,
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
            tracing::error!("LINE WORKS send_image_to_channel failed: {status} - {body}");
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

            cursor = body.response_meta_data.and_then(|m| m.next_cursor);
            if cursor.is_none() {
                break;
            }
        }

        Ok(members)
    }

    /// LINE WORKS 監査ログ (audit.read scope) の CSV をダウンロードする (Refs #540)。
    ///
    /// `GET /v1.0/audits/logs/download?service=...&startTime=...&endTime=...` は 302 で
    /// 実ファイルのダウンロード URL (`Location`) を返す。**この Location 先にも同じ
    /// `Authorization: Bearer` ヘッダを付け直してアクセスする必要がある**
    /// (公式ドキュメント file-upload の注記「ダウンロード URL にアクセスする際にも
    /// Authorization ヘッダを指定する必要があります。利用するライブラリによっては
    /// Authorization ヘッダを指定せずに自動的にリダイレクトされる場合があります」、
    /// 監査ログ API も同仕様。LINE WORKS Developers コミュニティに同一症状の解決事例あり)。
    ///
    /// **旧実装の不具合 (2026-09-11 本番実測)**: `reqwest::Client` の既定リダイレクト追従に
    /// 任せていたところ、Location 先がクロスホスト (`www.worksapis.com` → 別ドメイン) のため
    /// `Authorization` ヘッダが自動で外れ、Location 先が `401 Unauthorized
    /// "Authentication failed."` を返していた。`no_redirect_client` (リダイレクト非追従) で
    /// 明示的に Location を辿り、毎回 `Authorization` を付け直す。
    ///
    /// 期間は最長 31 日 (LINE WORKS API 制限)、呼び出し元で担保すること。
    /// 「並行して呼び出さないでください」という制約への対応 (直列化・キャッシュ) は
    /// このクライアントの責務ではなく呼び出し側 (`lineworks_login_activity.rs`) に置く。
    pub async fn fetch_login_audit_csv(
        &self,
        config_id: Uuid,
        config: &LineworksBotConfig,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<String, LineworksBotError> {
        let token = self
            .get_access_token(config_id, config, "audit.read")
            .await?;

        let url = format!(
            "{}?service=auth&startTime={}&endTime={}&language=ja_JP",
            self.audit_log_download_endpoint,
            urlencoding::encode(&format_audit_time(start)),
            urlencoding::encode(&format_audit_time(end)),
        );

        let bytes = self.get_with_auth_redirect(&url, &token).await?;
        Ok(decode_csv_bytes(&bytes))
    }

    /// リダイレクトを自動追従せず、3xx を見たら `Location` へ同じ `Authorization` を
    /// 付け直して手動で辿る GET。LINE WORKS のダウンロード URL 系 API 共通の作法
    /// (`fetch_login_audit_csv` 参照)。
    async fn get_with_auth_redirect(
        &self,
        url: &str,
        token: &str,
    ) -> Result<Vec<u8>, LineworksBotError> {
        const MAX_HOPS: u8 = 5;
        let mut current = url.to_string();

        for _ in 0..MAX_HOPS {
            let resp = self
                .no_redirect_client
                .get(&current)
                .header("Authorization", format!("Bearer {}", token))
                .send()
                .await?;

            let status = resp.status();
            if status.is_redirection() {
                let location = resp
                    .headers()
                    .get(reqwest::header::LOCATION)
                    .and_then(|v| v.to_str().ok())
                    .map(str::to_string)
                    .ok_or_else(|| {
                        LineworksBotError::SendFailed(format!(
                            "{status}: redirect response without Location header"
                        ))
                    })?;
                current = location;
                continue;
            }

            if !status.is_success() {
                let body = resp.text().await.unwrap_or_default();
                tracing::error!("LINE WORKS audit log download failed: {status} - {body}");
                return Err(LineworksBotError::SendFailed(format!("{status}: {body}")));
            }

            return Ok(resp.bytes().await?.to_vec());
        }

        Err(LineworksBotError::SendFailed(format!(
            "too many redirects (> {MAX_HOPS}) following audit log download URL"
        )))
    }
}

/// 監査ログ API が要求する `YYYY-MM-DDThh:mm:ssTZD` 形式。
fn format_audit_time(dt: DateTime<Utc>) -> String {
    dt.format("%Y-%m-%dT%H:%M:%S%:z").to_string()
}

/// CSV バイト列を文字列にデコードする。公式ドキュメントに文字コードの明記が無いため、
/// まず UTF-8 として読み、失敗したら Shift_JIS にフォールバックする (実データ未確認、Refs #540)。
fn decode_csv_bytes(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(s) => s.trim_start_matches('\u{feff}').to_string(),
        Err(_) => {
            let (decoded, _, _) = encoding_rs::SHIFT_JIS.decode(bytes);
            decoded.into_owned()
        }
    }
}

/// email → 成功ログインの最終日時、の集計結果。
pub type LastLoginByEmail = HashMap<String, DateTime<Utc>>;

/// LINE WORKS 監査ログ CSV (`service=auth`) を解析し、メールアドレスごとの
/// 「成功したログインの最終日時」を集計する。
///
/// ★ 実データで確認済み (2026-09-11、本番テナントの `language=ja_JP` CSV)。
/// ヘッダーは `説明,メンバー,日時,IPアドレス,ログイン方法,サービスタイプ` の6列で、
/// issue #540 本文にあった「結果」「メール」という独立列は無い:
/// - **メンバー**: `"山田太郎 (demo@example.com)"` のように表示名 + `(メールアドレス)`
///   の形式 (`extract_email_from_member_field` で括弧内を抽出。素のメールアドレスが
///   そのまま入っている変種にもフォールバックで対応)。
/// - **説明**: ログインの成否を表す文 (失敗時はエラーコード付きの文言になるらしい)。
///   独立した「結果」列が無いため、この列に対して `is_success_result` の
///   キーワード判定 (「成功」を含むか) を流用する。
///
/// 未知の見出しに遭遇したら黙って握りつぶさず `Err` を返す
/// (誤判定より「気づける失敗」を優先する設計)。
pub fn parse_login_audit_csv(csv_text: &str) -> Result<LastLoginByEmail, String> {
    let mut rdr = csv::ReaderBuilder::new()
        .flexible(true)
        .from_reader(csv_text.as_bytes());

    let headers = rdr
        .headers()
        .map_err(|e| format!("CSV header read error: {e}"))?
        .clone();

    let member_idx = find_column(&headers, &["メンバー", "member", "メール", "email", "mail"])
        .ok_or_else(|| format!("member/email column not found in headers: {headers:?}"))?;
    let datetime_idx = find_column(&headers, &["日時", "datetime", "time", "date"])
        .ok_or_else(|| format!("datetime column not found in headers: {headers:?}"))?;
    // 独立した「結果」列は無く、「説明」列に成否が文章で書かれている (実データ確認済み)。
    let result_idx = find_column(&headers, &["結果", "result", "説明", "description"]);

    let mut last_login: LastLoginByEmail = HashMap::new();

    for row in rdr.records() {
        let row = row.map_err(|e| format!("CSV row read error: {e}"))?;

        if let Some(idx) = result_idx {
            if !row.get(idx).map(is_success_result).unwrap_or(false) {
                continue; // 失敗ログイン行はスキップ
            }
        }

        let Some(email) = row
            .get(member_idx)
            .and_then(extract_email_from_member_field)
        else {
            continue;
        };
        let Some(dt) = row.get(datetime_idx).and_then(parse_audit_datetime) else {
            continue;
        };

        let key = email.to_lowercase();
        last_login
            .entry(key)
            .and_modify(|cur| {
                if dt > *cur {
                    *cur = dt;
                }
            })
            .or_insert(dt);
    }

    Ok(last_login)
}

/// ヘッダ行からキーワード (部分一致・大小無視) を含む列の index を探す。
fn find_column(headers: &csv::StringRecord, keywords: &[&str]) -> Option<usize> {
    headers.iter().position(|h| {
        let h_lower = h.to_lowercase();
        keywords
            .iter()
            .any(|k| h.contains(k) || h_lower.contains(&k.to_lowercase()))
    })
}

/// 「メンバー」列 (`"表示名 (email@example.com)"`) からメールアドレスを取り出す。
/// 括弧が無く生のメールアドレスがそのまま入っている変種にもフォールバックする。
fn extract_email_from_member_field(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    if let Some(open) = raw.rfind('(') {
        if let Some(close_rel) = raw[open + 1..].find(')') {
            let inner = raw[open + 1..open + 1 + close_rel].trim();
            if inner.contains('@') {
                return Some(inner.to_string());
            }
        }
    }
    if raw.contains('@') {
        return Some(raw.to_string());
    }
    None
}

/// 「説明」(または「結果」) 列の値がログイン成功を表すか判定する。
/// 独立した結果列でも、文章まじりの説明列でも「成功」を含むかで判定する
/// (実データ確認済み: 説明列は成否を表す文になっている)。
fn is_success_result(value: &str) -> bool {
    let v = value.trim();
    v.contains("成功") || v.eq_ignore_ascii_case("success") || v.eq_ignore_ascii_case("OK")
}

/// 「日時」列の値を UTC の `DateTime` に変換する。タイムゾーン付き (RFC3339) を優先し、
/// 無ければ `language=ja_JP` を想定して JST (`+09:00`) のローカル時刻として解釈する。
fn parse_audit_datetime(raw: &str) -> Option<DateTime<Utc>> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    if let Ok(dt) = DateTime::parse_from_rfc3339(raw) {
        return Some(dt.with_timezone(&Utc));
    }
    for fmt in [
        "%Y/%m/%d %H:%M:%S",
        "%Y-%m-%d %H:%M:%S",
        "%Y/%m/%d %H:%M",
        "%Y-%m-%d %H:%M",
    ] {
        if let Ok(naive) = NaiveDateTime::parse_from_str(raw, fmt) {
            let jst = FixedOffset::east_opt(9 * 3600)?;
            if let Some(dt) = jst.from_local_datetime(&naive).single() {
                return Some(dt.with_timezone(&Utc));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn response_meta_data_parses_next_cursor_from_camelcase() {
        let json = r#"{"nextCursor":"A3ST6cnnk0ALxM4u2_cvHtrNaZtBs4Bt3gGIWAjzdJ8="}"#;
        let md: ResponseMetaData = serde_json::from_str(json).expect("deserialize");
        assert_eq!(
            md.next_cursor.as_deref(),
            Some("A3ST6cnnk0ALxM4u2_cvHtrNaZtBs4Bt3gGIWAjzdJ8=")
        );
    }

    #[test]
    fn response_meta_data_ignores_legacy_cursor_field_name() {
        // Regression: the prior impl used `cursor:` which silently parsed nothing
        // from real LINE WORKS responses, capping results at one page.
        let json = r#"{"cursor":"should-not-match"}"#;
        let md: ResponseMetaData = serde_json::from_str(json).expect("deserialize");
        assert_eq!(md.next_cursor, None);
    }

    #[test]
    fn users_response_parses_next_page_shape() {
        let json = r#"{
            "users": [
                {"userId": "u1", "userName": {"lastName": "田中", "firstName": "太郎"}, "email": "t@x.com"}
            ],
            "responseMetaData": {"nextCursor": "abc"}
        }"#;
        let body: UsersResponse = serde_json::from_str(json).expect("deserialize");
        assert_eq!(body.users.as_ref().map(|u| u.len()), Some(1));
        assert_eq!(
            body.response_meta_data
                .and_then(|m| m.next_cursor)
                .as_deref(),
            Some("abc")
        );
    }

    #[test]
    fn users_response_parses_final_page_without_cursor() {
        let json = r#"{"users": [], "responseMetaData": {}}"#;
        let body: UsersResponse = serde_json::from_str(json).expect("deserialize");
        assert!(body.response_meta_data.unwrap().next_cursor.is_none());
    }

    // --- login audit CSV (#540) ---
    // ★ 実 CSV 未確認。ここでのヘッダ文字列は issue #540 本文の記述からの推測。

    #[test]
    fn parse_login_audit_csv_keeps_latest_success_per_email() {
        // 実データ確認済みのヘッダー (2026-09-11): 説明,メンバー,日時,IPアドレス,ログイン方法,サービスタイプ
        let csv = "説明,メンバー,日時,IPアドレス,ログイン方法,サービスタイプ\n\
                    ログインに成功しました,山田太郎 (a@x.com),2026/09/01 09:00:00,1.2.3.4,パスワード,ブラウザ\n\
                    ログインに成功しました,山田太郎 (a@x.com),2026/09/03 10:30:00,1.2.3.4,パスワード,モバイルアプリ\n\
                    ログインに失敗しました (エラーコード: 401),鈴木花子 (b@x.com),2026/09/02 08:00:00,5.6.7.8,パスワード,ブラウザ\n";
        let result = parse_login_audit_csv(csv).expect("parse");
        assert_eq!(result.len(), 1);
        let a = result.get("a@x.com").expect("a@x.com present");
        assert_eq!(a.to_rfc3339(), "2026-09-03T01:30:00+00:00"); // JST → UTC
        assert!(!result.contains_key("b@x.com")); // 失敗のみの行は集計しない
    }

    #[test]
    fn parse_login_audit_csv_matches_email_case_insensitively_and_trims() {
        let csv =
            "説明,メンバー,日時\nログインに成功しました, 山田太郎 (A@X.com) ,2026/09/01 09:00:00\n";
        let result = parse_login_audit_csv(csv).expect("parse");
        assert!(result.contains_key("a@x.com"));
    }

    #[test]
    fn parse_login_audit_csv_accepts_bare_email_without_display_name() {
        // 「メンバー」列が名前 (email) 形式ではなく、生のメールアドレスだけの
        // 変種にもフォールバックする。
        let csv = "説明,メンバー,日時\nログインに成功しました,a@x.com,2026/09/01 09:00:00\n";
        let result = parse_login_audit_csv(csv).expect("parse");
        assert!(result.contains_key("a@x.com"));
    }

    #[test]
    fn parse_login_audit_csv_without_result_column_treats_all_rows_as_success() {
        // 「説明」列を見つけられない見出しでも、メンバー/日時さえ拾えれば失敗しない
        // (誤って全件除外するより、失敗ログインを紛れ込ませる方が実害が小さいため)。
        let csv = "メンバー,日時\n山田太郎 (a@x.com),2026/09/01 09:00:00\n";
        let result = parse_login_audit_csv(csv).expect("parse");
        assert!(result.contains_key("a@x.com"));
    }

    #[test]
    fn parse_login_audit_csv_fails_loudly_on_unrecognized_headers() {
        // 列名が全く想定外なら黙って空集計にせず Err を返す (#540 の設計方針)。
        let csv = "col_a,col_b\n1,2\n";
        let err = parse_login_audit_csv(csv).unwrap_err();
        assert!(err.contains("member/email column not found"));
    }

    #[test]
    fn extract_email_from_member_field_prefers_parenthesized_email() {
        assert_eq!(
            extract_email_from_member_field("山田太郎 (demo@example.com)"),
            Some("demo@example.com".to_string())
        );
    }

    #[test]
    fn extract_email_from_member_field_falls_back_to_bare_email() {
        assert_eq!(
            extract_email_from_member_field("demo@example.com"),
            Some("demo@example.com".to_string())
        );
    }

    #[test]
    fn extract_email_from_member_field_returns_none_without_at_sign() {
        // 括弧内に @ が無い (例: 部署名等) 場合は誤抽出せず諦める。
        assert_eq!(extract_email_from_member_field("山田太郎 (営業部)"), None);
        assert_eq!(extract_email_from_member_field("山田太郎"), None);
        assert_eq!(extract_email_from_member_field(""), None);
    }

    #[test]
    fn is_success_result_recognizes_known_variants() {
        assert!(is_success_result("成功"));
        assert!(is_success_result("ログインに成功しました"));
        assert!(is_success_result(" success "));
        assert!(is_success_result("OK"));
        assert!(!is_success_result("失敗"));
        assert!(!is_success_result(
            "ログインに失敗しました (エラーコード: 401)"
        ));
        assert!(!is_success_result("failure"));
    }

    #[test]
    fn parse_audit_datetime_accepts_rfc3339() {
        let dt = parse_audit_datetime("2026-09-01T00:00:00+09:00").expect("parse");
        assert_eq!(dt.to_rfc3339(), "2026-08-31T15:00:00+00:00");
    }

    #[test]
    fn parse_audit_datetime_treats_slash_format_as_jst() {
        let dt = parse_audit_datetime("2026/09/01 09:00:00").expect("parse");
        assert_eq!(dt.to_rfc3339(), "2026-09-01T00:00:00+00:00");
    }

    #[test]
    fn parse_audit_datetime_rejects_garbage() {
        assert!(parse_audit_datetime("not-a-date").is_none());
        assert!(parse_audit_datetime("").is_none());
    }

    #[test]
    fn decode_csv_bytes_strips_utf8_bom() {
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend_from_slice("a,b\n1,2\n".as_bytes());
        assert_eq!(decode_csv_bytes(&bytes), "a,b\n1,2\n");
    }

    #[test]
    fn decode_csv_bytes_falls_back_to_shift_jis() {
        let (encoded, _, had_errors) = encoding_rs::SHIFT_JIS.encode("名前,結果\n田中,成功\n");
        assert!(!had_errors);
        assert_eq!(decode_csv_bytes(&encoded), "名前,結果\n田中,成功\n");
    }

    /// RSA 鍵生成は 2048bit で数百 ms かかるのでテスト間で使い回す
    /// (lineworks_channels.rs のテストヘルパーと同じ方針)。
    fn test_private_pem() -> &'static str {
        use rsa::pkcs1::EncodeRsaPrivateKey;
        use rsa::rand_core::OsRng;
        use rsa::RsaPrivateKey;
        use std::sync::OnceLock;
        static PEM: OnceLock<String> = OnceLock::new();
        PEM.get_or_init(|| {
            RsaPrivateKey::new(&mut OsRng, 2048)
                .unwrap()
                .to_pkcs1_pem(rsa::pkcs1::LineEnding::LF)
                .unwrap()
                .to_string()
        })
    }

    fn test_config() -> LineworksBotConfig {
        LineworksBotConfig {
            client_id: "client".into(),
            client_secret: "secret".into(),
            service_account: "sa@example.com".into(),
            private_key: test_private_pem().to_string(),
            bot_id: "bot1".into(),
        }
    }

    #[tokio::test]
    async fn fetch_login_audit_csv_follows_redirect_and_returns_body() {
        use wiremock::matchers::{header, method, path, query_param};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let auth_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "tok",
                "refresh_token": "r",
                "expires_in": 3600,
            })))
            .mount(&auth_server)
            .await;

        let csv_server = MockServer::start().await;
        // 回帰ガード (2026-09-11 実測のバグ): Location 先は別ホストなので、
        // reqwest の既定リダイレクト追従だと Authorization が落ちる。ここで
        // 明示的に header を要求し、付いていなければ 404 (マッチ無し) で
        // テストが失敗するようにする。
        Mock::given(method("GET"))
            .and(path("/download/csv"))
            .and(header("Authorization", "Bearer tok"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string("日時,メール,結果\n2026/09/01 09:00:00,a@x.com,成功\n"),
            )
            .mount(&csv_server)
            .await;

        let audit_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/audit"))
            .and(query_param("service", "auth"))
            .respond_with(
                ResponseTemplate::new(302)
                    .insert_header("Location", format!("{}/download/csv", csv_server.uri())),
            )
            .mount(&audit_server)
            .await;

        let client = LineworksBotClient::with_all_endpoints(
            "http://unused/",
            &format!("{}/token", auth_server.uri()),
            &format!("{}/audit", audit_server.uri()),
        );
        let config = test_config();
        let start = Utc::now() - chrono::Duration::days(31);
        let end = Utc::now();

        let csv = client
            .fetch_login_audit_csv(Uuid::new_v4(), &config, start, end)
            .await
            .expect("fetch succeeds");

        assert!(csv.contains("a@x.com"));
        let parsed = parse_login_audit_csv(&csv).expect("parse");
        assert!(parsed.contains_key("a@x.com"));
    }

    #[tokio::test]
    async fn fetch_login_audit_csv_maps_403_to_send_failed() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let auth_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "tok",
                "refresh_token": "r",
                "expires_in": 3600,
            })))
            .mount(&auth_server)
            .await;

        let audit_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/audit"))
            .respond_with(ResponseTemplate::new(403).set_body_string("forbidden"))
            .mount(&audit_server)
            .await;

        let client = LineworksBotClient::with_all_endpoints(
            "http://unused/",
            &format!("{}/token", auth_server.uri()),
            &format!("{}/audit", audit_server.uri()),
        );
        let config = test_config();
        let err = client
            .fetch_login_audit_csv(Uuid::new_v4(), &config, Utc::now(), Utc::now())
            .await
            .unwrap_err();
        assert!(matches!(err, LineworksBotError::SendFailed(_)));
        assert!(err.to_string().contains("403"));
    }

    #[tokio::test]
    async fn fetch_login_audit_csv_returns_401_when_redirect_target_rejects_auth() {
        // 2026-09-11 本番実測の再現: リダイレクト先が Authorization を要求するのに
        // 付いていなければ 401 になる (旧実装のバグそのもの)。現行実装は毎回
        // Authorization を付け直すので、この mock (header 無し要求) には
        // 到達せず 200 になることを裏側の別テストで確認済み。ここでは
        // Location 先がヘッダ不備を理由に 401 を返すケースが、握りつぶさず
        // ちゃんとエラーとして伝播することを確認する。
        use wiremock::matchers::{method, path, query_param};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let auth_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "tok",
                "refresh_token": "r",
                "expires_in": 3600,
            })))
            .mount(&auth_server)
            .await;

        let csv_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/download/csv"))
            .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
                "code": "UNAUTHORIZED",
                "description": "Authentication failed.",
            })))
            .mount(&csv_server)
            .await;

        let audit_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/audit"))
            .and(query_param("service", "auth"))
            .respond_with(
                ResponseTemplate::new(302)
                    .insert_header("Location", format!("{}/download/csv", csv_server.uri())),
            )
            .mount(&audit_server)
            .await;

        let client = LineworksBotClient::with_all_endpoints(
            "http://unused/",
            &format!("{}/token", auth_server.uri()),
            &format!("{}/audit", audit_server.uri()),
        );
        let config = test_config();
        let err = client
            .fetch_login_audit_csv(Uuid::new_v4(), &config, Utc::now(), Utc::now())
            .await
            .unwrap_err();
        assert!(err.to_string().contains("401"));
        assert!(err.to_string().contains("UNAUTHORIZED"));
    }

    #[tokio::test]
    async fn fetch_login_audit_csv_follows_multiple_redirect_hops_with_auth() {
        use wiremock::matchers::{header, method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let auth_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "tok",
                "refresh_token": "r",
                "expires_in": 3600,
            })))
            .mount(&auth_server)
            .await;

        let csv_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/final/csv"))
            .and(header("Authorization", "Bearer tok"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string("メール,日時\na@x.com,2026/09/01 09:00:00\n"),
            )
            .mount(&csv_server)
            .await;
        Mock::given(method("GET"))
            .and(path("/hop1"))
            .and(header("Authorization", "Bearer tok"))
            .respond_with(
                ResponseTemplate::new(302)
                    .insert_header("Location", format!("{}/final/csv", csv_server.uri())),
            )
            .mount(&csv_server)
            .await;

        let audit_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/audit"))
            .respond_with(
                ResponseTemplate::new(302)
                    .insert_header("Location", format!("{}/hop1", csv_server.uri())),
            )
            .mount(&audit_server)
            .await;

        let client = LineworksBotClient::with_all_endpoints(
            "http://unused/",
            &format!("{}/token", auth_server.uri()),
            &format!("{}/audit", audit_server.uri()),
        );
        let config = test_config();
        let csv = client
            .fetch_login_audit_csv(Uuid::new_v4(), &config, Utc::now(), Utc::now())
            .await
            .expect("fetch succeeds across 2 redirect hops");
        assert!(csv.contains("a@x.com"));
    }

    #[tokio::test]
    async fn fetch_login_audit_csv_errors_on_redirect_without_location() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let auth_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "tok",
                "refresh_token": "r",
                "expires_in": 3600,
            })))
            .mount(&auth_server)
            .await;

        let audit_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/audit"))
            .respond_with(ResponseTemplate::new(302)) // Location ヘッダ無し
            .mount(&audit_server)
            .await;

        let client = LineworksBotClient::with_all_endpoints(
            "http://unused/",
            &format!("{}/token", auth_server.uri()),
            &format!("{}/audit", audit_server.uri()),
        );
        let config = test_config();
        let err = client
            .fetch_login_audit_csv(Uuid::new_v4(), &config, Utc::now(), Utc::now())
            .await
            .unwrap_err();
        assert!(err.to_string().contains("Location"));
    }
}
