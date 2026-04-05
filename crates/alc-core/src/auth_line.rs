//! LINE Login OAuth2 helpers
//!
//! Global channel (env vars only, no per-tenant SSO config).

use serde::Deserialize;

/// LINE Login authorize URL を構築
///
/// `redirect_uri` と `state` は呼び出し元で URL エンコード済みの前提。
pub fn authorize_url(channel_id: &str, encoded_redirect_uri: &str, encoded_state: &str) -> String {
    format!(
        "https://access-line.me/oauth2/v2.1/authorize?\
         response_type=code\
         &client_id={channel_id}\
         &redirect_uri={encoded_redirect_uri}\
         &state={encoded_state}\
         &scope=profile%20openid",
    )
}

#[derive(Debug, Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
}

/// Code → Token 交換
pub async fn exchange_code(
    client: &reqwest::Client,
    channel_id: &str,
    channel_secret: &str,
    code: &str,
    redirect_uri: &str,
) -> Result<TokenResponse, String> {
    let resp = client
        .post("https://api.line.me/oauth2/v2.1/token")
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", redirect_uri),
            ("client_id", channel_id),
            ("client_secret", channel_secret),
        ])
        .send()
        .await
        .map_err(|e| format!("LINE token request failed: {e}"))?;

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("LINE token exchange failed: {body}"));
    }

    resp.json::<TokenResponse>()
        .await
        .map_err(|e| format!("LINE token parse failed: {e}"))
}

#[derive(Debug, Deserialize)]
pub struct LineProfile {
    #[serde(rename = "userId")]
    pub user_id: String,
    #[serde(rename = "displayName")]
    pub display_name: String,
    #[serde(rename = "pictureUrl")]
    pub picture_url: Option<String>,
}

/// LINE Profile 取得
pub async fn fetch_profile(
    client: &reqwest::Client,
    access_token: &str,
) -> Result<LineProfile, String> {
    let resp = client
        .get("https://api.line.me/v2/profile")
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|e| format!("LINE profile request failed: {e}"))?;

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("LINE profile fetch failed: {body}"));
    }

    resp.json::<LineProfile>()
        .await
        .map_err(|e| format!("LINE profile parse failed: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_authorize_url() {
        let url = authorize_url(
            "123456",
            "https%3A%2F%2Fexample.com%2Fcallback",
            "state-token",
        );
        assert!(url.starts_with("https://access-line.me/oauth2/v2.1/authorize?"));
        assert!(url.contains("client_id=123456"));
        assert!(url.contains("scope=profile%20openid"));
        assert!(url.contains("state=state-token"));
    }
}
