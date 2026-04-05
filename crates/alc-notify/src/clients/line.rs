//! LINE Messaging API client
//! Push message を送信する

#[derive(Debug, thiserror::Error)]
pub enum LineError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("Send failed: {0}")]
    SendFailed(String),
}

/// 復号済み LINE 設定
#[derive(Debug, Clone)]
pub struct LineConfig {
    pub channel_access_token: String,
    pub channel_secret: String,
}

pub struct LineClient {
    client: reqwest::Client,
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
        }
    }

    /// ユーザーに push メッセージを送信
    pub async fn push_text(
        &self,
        config: &LineConfig,
        user_id: &str,
        text: &str,
    ) -> Result<(), LineError> {
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
            .header(
                "Authorization",
                format!("Bearer {}", config.channel_access_token),
            )
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
