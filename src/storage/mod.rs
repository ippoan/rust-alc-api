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
