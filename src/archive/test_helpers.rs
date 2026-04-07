#![cfg(test)]

use alc_core::storage::{StorageBackend, StorageError};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

pub struct TestStorage {
    files: Mutex<HashMap<String, Vec<u8>>>,
    pub fail_upload: AtomicBool,
}

impl TestStorage {
    pub fn new() -> Self {
        Self {
            files: Mutex::new(HashMap::new()),
            fail_upload: AtomicBool::new(false),
        }
    }
    pub fn put(&self, key: &str, data: Vec<u8>) {
        self.files.lock().unwrap().insert(key.to_string(), data);
    }
    pub fn get(&self, key: &str) -> Option<Vec<u8>> {
        self.files.lock().unwrap().get(key).cloned()
    }
    pub fn keys(&self) -> Vec<String> {
        self.files.lock().unwrap().keys().cloned().collect()
    }
}

#[async_trait::async_trait]
impl StorageBackend for TestStorage {
    async fn upload(&self, key: &str, data: &[u8], _ct: &str) -> Result<String, StorageError> {
        if self.fail_upload.load(Ordering::Relaxed) {
            return Err(StorageError::Upload("mock fail".into()));
        }
        self.files
            .lock()
            .unwrap()
            .insert(key.to_string(), data.to_vec());
        Ok(self.public_url(key))
    }

    fn public_url(&self, key: &str) -> String {
        format!("https://test-storage/{}", key)
    }

    async fn download(&self, key: &str) -> Result<Vec<u8>, StorageError> {
        self.files
            .lock()
            .unwrap()
            .get(key)
            .cloned()
            .ok_or_else(|| StorageError::Upload(format!("not found: {key}")))
    }

    fn extract_key(&self, url: &str) -> Option<String> {
        url.strip_prefix("https://test-storage/")
            .map(|s| s.to_string())
    }

    fn bucket(&self) -> &str {
        "test-bucket"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_storage_upload_download() {
        let s = TestStorage::new();
        s.upload("k", b"data", "text/plain").await.unwrap();
        assert_eq!(s.download("k").await.unwrap(), b"data");
    }

    #[test]
    fn test_storage_public_url() {
        let s = TestStorage::new();
        assert_eq!(s.public_url("foo/bar"), "https://test-storage/foo/bar");
    }

    #[test]
    fn test_storage_extract_key() {
        let s = TestStorage::new();
        assert_eq!(
            s.extract_key("https://test-storage/foo/bar"),
            Some("foo/bar".into())
        );
        assert_eq!(s.extract_key("https://other/foo"), None);
    }

    #[test]
    fn test_storage_bucket() {
        let s = TestStorage::new();
        assert_eq!(s.bucket(), "test-bucket");
    }

    #[tokio::test]
    async fn test_storage_download_not_found() {
        let s = TestStorage::new();
        assert!(s.download("missing").await.is_err());
    }

    #[tokio::test]
    async fn test_storage_fail_upload() {
        let s = TestStorage::new();
        s.fail_upload.store(true, Ordering::Relaxed);
        assert!(s.upload("k", b"data", "text/plain").await.is_err());
    }

    #[test]
    fn test_storage_put_get_keys() {
        let s = TestStorage::new();
        s.put("a", b"1".to_vec());
        s.put("b", b"2".to_vec());
        assert_eq!(s.get("a"), Some(b"1".to_vec()));
        assert_eq!(s.get("c"), None);
        assert_eq!(s.keys().len(), 2);
    }
}
