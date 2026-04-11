use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum CloudTasksError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("API error: {status} {body}")]
    Api { status: u16, body: String },
    #[error("Config missing: {0}")]
    Config(String),
    #[error("Auth error: {0}")]
    Auth(String),
}

#[async_trait]
pub trait CloudTasksClient: Send + Sync {
    /// Create a Cloud Task that will call POST /api/trouble/schedules/{schedule_id}/fire
    async fn create_task(
        &self,
        schedule_id: Uuid,
        scheduled_at: DateTime<Utc>,
    ) -> Result<String, CloudTasksError>;

    /// Delete a Cloud Task by its full resource name
    async fn delete_task(&self, task_name: &str) -> Result<(), CloudTasksError>;
}

/// GCP Cloud Tasks implementation using REST API + metadata server auth
pub struct GcpCloudTasksClient {
    client: reqwest::Client,
    project: String,
    location: String,
    queue: String,
    api_origin: String,
    service_account_email: String,
}

impl GcpCloudTasksClient {
    pub fn from_env() -> Result<Self, CloudTasksError> {
        Ok(Self {
            client: reqwest::Client::new(),
            project: std::env::var("CLOUD_TASKS_PROJECT")
                .map_err(|_| CloudTasksError::Config("CLOUD_TASKS_PROJECT".into()))?,
            location: std::env::var("CLOUD_TASKS_LOCATION")
                .unwrap_or_else(|_| "asia-northeast1".into()),
            queue: std::env::var("CLOUD_TASKS_QUEUE")
                .map_err(|_| CloudTasksError::Config("CLOUD_TASKS_QUEUE".into()))?,
            api_origin: std::env::var("API_ORIGIN")
                .map_err(|_| CloudTasksError::Config("API_ORIGIN".into()))?,
            service_account_email: std::env::var("CLOUD_TASKS_SA_EMAIL").unwrap_or_default(),
        })
    }

    async fn get_access_token(&self) -> Result<String, CloudTasksError> {
        let url = "http://metadata.google.internal/computeMetadata/v1/instance/service-accounts/default/token";
        let resp = self
            .client
            .get(url)
            .header("Metadata-Flavor", "Google")
            .send()
            .await
            .map_err(CloudTasksError::Http)?;
        let body: serde_json::Value = resp.json().await.map_err(CloudTasksError::Http)?;
        body["access_token"]
            .as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| CloudTasksError::Auth("No access_token in metadata response".into()))
    }
}

#[async_trait]
impl CloudTasksClient for GcpCloudTasksClient {
    async fn create_task(
        &self,
        schedule_id: Uuid,
        scheduled_at: DateTime<Utc>,
    ) -> Result<String, CloudTasksError> {
        let token = self.get_access_token().await?;
        let parent = format!(
            "projects/{}/locations/{}/queues/{}",
            self.project, self.location, self.queue
        );
        let url = format!("https://cloudtasks.googleapis.com/v2/{parent}/tasks");

        let target_url = format!(
            "{}/api/trouble/schedules/{}/fire",
            self.api_origin, schedule_id
        );

        let mut http_request = serde_json::json!({
            "url": target_url,
            "httpMethod": "POST",
            "headers": { "Content-Type": "application/json" },
        });

        if !self.service_account_email.is_empty() {
            http_request["oidcToken"] = serde_json::json!({
                "serviceAccountEmail": self.service_account_email,
                "audience": self.api_origin,
            });
        }

        let task_body = serde_json::json!({
            "task": {
                "scheduleTime": scheduled_at.to_rfc3339(),
                "httpRequest": http_request,
            }
        });

        let resp = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {token}"))
            .json(&task_body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            return Err(CloudTasksError::Api { status, body });
        }

        let result: serde_json::Value = resp.json().await?;
        Ok(result["name"].as_str().unwrap_or_default().to_string())
    }

    async fn delete_task(&self, task_name: &str) -> Result<(), CloudTasksError> {
        let token = self.get_access_token().await?;
        let url = format!("https://cloudtasks.googleapis.com/v2/{task_name}");

        let resp = self
            .client
            .delete(&url)
            .header("Authorization", format!("Bearer {token}"))
            .send()
            .await?;

        // 404 is OK (task already deleted)
        if !resp.status().is_success() && resp.status().as_u16() != 404 {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            return Err(CloudTasksError::Api { status, body });
        }

        Ok(())
    }
}
