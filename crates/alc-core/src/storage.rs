#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("Upload failed: {0}")]
    Upload(String),
    #[error("Config error: {0}")]
    Config(String),
}

/// 1 件の LIST 結果。`etag` は囲みの `"` を剥がした生の ETag
/// (S3 系 LIST は `"abc123"` のようにリテラルの引用符付きで返す)。
///
/// 単一パート PUT の ETag はオブジェクトの content MD5 と一致する
/// (Refs ohishi-exp/rust-ichibanboshi#205 実装計画 13)。この repo の dtako CSV
/// upload 経路 (`crates/alc-dtako/src/dtako_upload.rs` → `StorageBackend::upload`
/// → `put_object_with_content_type`) は常に `multipart: None` の単発 PUT なので、
/// この前提が成り立ち、ETag を「内容が変わったか」の安価な指紋として使える。
/// マルチパート PUT の ETag は MD5 ではなくパートハッシュの連結ハッシュになるため、
/// この前提が壊れる — dtako CSV 以外の用途にこの `list` を転用する際は要確認。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListedObject {
    pub key: String,
    pub etag: Option<String>,
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

    /// Check if an object exists in storage (HEAD request).
    async fn exists(&self, key: &str) -> Result<bool, StorageError>;

    /// Delete an object. Returns Ok(()) even if the object did not exist (idempotent).
    async fn delete(&self, key: &str) -> Result<(), StorageError>;

    /// Extract the object key from a public URL.
    fn extract_key(&self, url: &str) -> Option<String>;

    /// Bucket name.
    fn bucket(&self) -> &str;

    /// Generate a presigned GET URL for the given key.
    ///
    /// `expiry_seconds` controls how long the signed URL is valid (max 604800 = 7 days for S3 spec).
    /// The URL grants read access without further authentication; treat it as a bearer token.
    async fn presign_get(&self, key: &str, expiry_seconds: u32) -> Result<String, StorageError>;

    /// List objects under `prefix` via a single (backend-side auto-paginated) LIST call.
    /// Returns key + ETag only — never downloads object content. Intended for cheap
    /// "did anything change" fingerprints (Refs ohishi-exp/rust-ichibanboshi#205 実装計画 13).
    ///
    /// Default: unsupported. Override only in backends that actually implement it
    /// (currently `R2Backend` for real use, `MockStorage` for tests).
    async fn list(&self, _prefix: &str) -> Result<Vec<ListedObject>, StorageError> {
        Err(StorageError::Config(
            "list not supported by this backend".to_string(),
        ))
    }
}
