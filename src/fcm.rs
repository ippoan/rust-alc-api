pub use alc_core::fcm::{FcmError, FcmSenderTrait};

use reqwest::Client;
use serde::Serialize;

#[derive(Clone)]
pub struct FcmSender {
    client: Client,
    project_id: String,
}

#[derive(Serialize)]
struct FcmRequest {
    message: FcmMessage,
}

#[derive(Serialize)]
struct FcmMessage {
    token: String,
    data: std::collections::HashMap<String, String>,
    android: AndroidConfig,
}

#[derive(Serialize)]
struct AndroidConfig {
    priority: String,
}

impl FcmSender {
    pub fn new(project_id: String) -> Self {
        Self {
            client: Client::new(),
            project_id,
        }
    }
}

#[async_trait::async_trait]
impl FcmSenderTrait for FcmSender {
    async fn send_data_message(
        &self,
        fcm_token: &str,
        data: std::collections::HashMap<String, String>,
    ) -> Result<(), FcmError> {
        let access_token = self.get_access_token().await?;

        let url = format!(
            "https://fcm.googleapis.com/v1/projects/{}/messages:send",
            self.project_id
        );

        let body = FcmRequest {
            message: FcmMessage {
                token: fcm_token.to_string(),
                data,
                android: AndroidConfig {
                    priority: "high".to_string(),
                },
            },
        };

        let resp = self
            .client
            .post(&url)
            .bearer_auth(&access_token)
            .json(&body)
            .send()
            .await
            .map_err(|e| FcmError::Send(format!("HTTP request failed: {e}")))?;

        let status = resp.status();
        if !status.is_success() {
            let body_text = resp.text().await.unwrap_or_default();
            return Err(FcmError::Send(format!(
                "FCM API returned {status}: {body_text}"
            )));
        }

        Ok(())
    }
}

impl FcmSender {
    /// Get OAuth2 access token from Cloud Run metadata server
    async fn get_access_token(&self) -> Result<String, FcmError> {
        let url = "http://metadata.google.internal/computeMetadata/v1/instance/service-accounts/default/token";
        let resp: serde_json::Value = self
            .client
            .get(url)
            .header("Metadata-Flavor", "Google")
            .send()
            .await
            .map_err(|e| FcmError::Auth(format!("metadata server: {e}")))?
            .json()
            .await
            .map_err(|e| FcmError::Auth(format!("metadata parse: {e}")))?;

        resp["access_token"]
            .as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| FcmError::Auth("no access_token in metadata response".into()))
    }
}
