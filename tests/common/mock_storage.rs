use std::collections::HashMap;
use std::sync::Mutex;

use rust_alc_api::storage::{StorageBackend, StorageError};

/// テスト用インメモリストレージ
pub struct MockStorage {
    bucket_name: String,
    files: Mutex<HashMap<String, Vec<u8>>>,
    pub fail_upload: std::sync::atomic::AtomicBool,
}

impl MockStorage {
    pub fn new(bucket_name: &str) -> Self {
        Self {
            bucket_name: bucket_name.to_string(),
            files: Mutex::new(HashMap::new()),
            fail_upload: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// Pre-populate a file in the mock storage (for download tests).
    /// Returns the public URL for the inserted file.
    pub fn insert_file(&self, key: &str, data: Vec<u8>) -> String {
        self.files.lock().unwrap().insert(key.to_string(), data);
        self.public_url(key)
    }
}

#[async_trait::async_trait]
impl StorageBackend for MockStorage {
    async fn upload(
        &self,
        key: &str,
        data: &[u8],
        _content_type: &str,
    ) -> Result<String, StorageError> {
        if self.fail_upload.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(StorageError::Upload("mock upload failure".to_string()));
        }
        self.files
            .lock()
            .unwrap()
            .insert(key.to_string(), data.to_vec());
        Ok(self.public_url(key))
    }

    fn public_url(&self, key: &str) -> String {
        format!("https://mock-storage/{}/{}", self.bucket_name, key)
    }

    async fn download(&self, key: &str) -> Result<Vec<u8>, StorageError> {
        self.files
            .lock()
            .unwrap()
            .get(key)
            .cloned()
            .ok_or_else(|| StorageError::Upload(format!("Not found: {key}")))
    }

    fn extract_key(&self, url: &str) -> Option<String> {
        let prefix = format!("https://mock-storage/{}/", self.bucket_name);
        url.strip_prefix(&prefix).map(|s| s.to_string())
    }

    fn bucket(&self) -> &str {
        &self.bucket_name
    }
}
