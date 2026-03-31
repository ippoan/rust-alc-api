use alc_core::storage::{StorageBackend, StorageError};
use reqwest::Client;

pub struct GcsBackend {
    client: Client,
    bucket: String,
}

impl GcsBackend {
    pub fn new(bucket: String) -> Self {
        Self {
            client: Client::new(),
            bucket,
        }
    }

    async fn get_access_token(&self) -> Result<String, StorageError> {
        let url = "http://metadata.google.internal/computeMetadata/v1/instance/service-accounts/default/token";
        let resp: serde_json::Value = self
            .client
            .get(url)
            .header("Metadata-Flavor", "Google")
            .send()
            .await
            .map_err(|e| StorageError::Upload(format!("metadata server: {e}")))?
            .json()
            .await
            .map_err(|e| StorageError::Upload(format!("metadata parse: {e}")))?;

        resp["access_token"]
            .as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| StorageError::Upload("no access_token in metadata response".into()))
    }
}

#[async_trait::async_trait]
impl StorageBackend for GcsBackend {
    async fn upload(
        &self,
        key: &str,
        data: &[u8],
        content_type: &str,
    ) -> Result<String, StorageError> {
        let token = self.get_access_token().await?;
        let url = format!(
            "https://storage.googleapis.com/upload/storage/v1/b/{}/o?uploadType=media&name={}",
            self.bucket, key
        );

        self.client
            .post(&url)
            .bearer_auth(&token)
            .header("Content-Type", content_type)
            .body(data.to_vec())
            .send()
            .await
            .map_err(|e| StorageError::Upload(e.to_string()))?
            .error_for_status()
            .map_err(|e| StorageError::Upload(e.to_string()))?;

        Ok(self.public_url(key))
    }

    fn public_url(&self, key: &str) -> String {
        format!("https://storage.googleapis.com/{}/{}", self.bucket, key)
    }

    async fn download(&self, key: &str) -> Result<Vec<u8>, StorageError> {
        let token = self.get_access_token().await?;
        // GCS JSON API requires URL-encoded object name (slashes → %2F)
        let encoded_key = key.replace('/', "%2F");
        let url = format!(
            "https://storage.googleapis.com/storage/v1/b/{}/o/{}?alt=media",
            self.bucket, encoded_key
        );

        let bytes = self
            .client
            .get(&url)
            .bearer_auth(&token)
            .send()
            .await
            .map_err(|e| StorageError::Upload(format!("GCS download: {e}")))?
            .error_for_status()
            .map_err(|e| StorageError::Upload(format!("GCS download status: {e}")))?
            .bytes()
            .await
            .map_err(|e| StorageError::Upload(format!("GCS download bytes: {e}")))?;

        Ok(bytes.to_vec())
    }

    fn extract_key(&self, url: &str) -> Option<String> {
        let prefix = format!("https://storage.googleapis.com/{}/", self.bucket);
        url.strip_prefix(&prefix).map(|s| s.to_string())
    }

    fn bucket(&self) -> &str {
        &self.bucket
    }
}
