pub mod gcs;
pub mod r2;

pub use gcs::GcsBackend;
pub use r2::R2Backend;

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("Upload failed: {0}")]
    Upload(String),
    #[error("Config error: {0}")]
    Config(String),
}

#[async_trait::async_trait]
pub trait StorageBackend: Send + Sync {
    /// Upload file and return the public URL.
    async fn upload(
        &self,
        key: &str,
        data: &[u8],
        content_type: &str,
    ) -> Result<String, StorageError>;

    /// Get the public URL for a stored object.
    fn public_url(&self, key: &str) -> String;

    /// Download file and return the bytes.
    async fn download(&self, key: &str) -> Result<Vec<u8>, StorageError>;

    /// Extract the object key from a public URL.
    fn extract_key(&self, url: &str) -> Option<String>;

    /// Bucket name.
    fn bucket(&self) -> &str;
}

/// テスト用インメモリストレージ
#[cfg(test)]
pub fn mock_storage() -> std::sync::Arc<dyn StorageBackend> {
    std::sync::Arc::new(InMemoryStorage::default())
}

#[cfg(test)]
#[derive(Default)]
struct InMemoryStorage {
    files: std::sync::Mutex<std::collections::HashMap<String, Vec<u8>>>,
}

#[cfg(test)]
#[async_trait::async_trait]
impl StorageBackend for InMemoryStorage {
    async fn upload(&self, key: &str, data: &[u8], _ct: &str) -> Result<String, StorageError> {
        self.files.lock().unwrap().insert(key.to_string(), data.to_vec());
        Ok(format!("mock://{key}"))
    }
    fn public_url(&self, key: &str) -> String { format!("mock://{key}") }
    async fn download(&self, key: &str) -> Result<Vec<u8>, StorageError> {
        self.files.lock().unwrap().get(key).cloned()
            .ok_or_else(|| StorageError::Upload(format!("not found: {key}")))
    }
    fn extract_key(&self, url: &str) -> Option<String> { url.strip_prefix("mock://").map(|s| s.to_string()) }
    fn bucket(&self) -> &str { "mock" }
}
