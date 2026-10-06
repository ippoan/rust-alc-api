use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Mutex;

use rust_alc_api::storage::{ListedObject, StorageBackend, StorageError};

/// テスト用インメモリストレージ
pub struct MockStorage {
    bucket_name: String,
    files: Mutex<HashMap<String, Vec<u8>>>,
    pub fail_upload: std::sync::atomic::AtomicBool,
    /// `list()` だけを失敗させる (R2 LIST がエラーになるケースの単体テスト用)。
    pub fail_list: std::sync::atomic::AtomicBool,
    /// `list()` に渡された `prefix` を呼ばれた順に記録する。「LIST が本当に絞られて
    /// いるか」(呼び出し回数・prefix そのもの) をテストから検査するためのもの
    /// (Refs ohishi-exp/rust-ichibanboshi#205 comment 205-22)。
    list_calls: Mutex<Vec<String>>,
}

impl MockStorage {
    pub fn new(bucket_name: &str) -> Self {
        Self {
            bucket_name: bucket_name.to_string(),
            files: Mutex::new(HashMap::new()),
            fail_upload: std::sync::atomic::AtomicBool::new(false),
            fail_list: std::sync::atomic::AtomicBool::new(false),
            list_calls: Mutex::new(Vec::new()),
        }
    }

    /// `list()` が呼ばれた prefix を呼び出し順に返す。
    pub fn list_calls(&self) -> Vec<String> {
        self.list_calls.lock().unwrap().clone()
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

    async fn exists(&self, key: &str) -> Result<bool, StorageError> {
        Ok(self.files.lock().unwrap().contains_key(key))
    }

    async fn download(&self, key: &str) -> Result<Vec<u8>, StorageError> {
        self.files
            .lock()
            .unwrap()
            .get(key)
            .cloned()
            .ok_or_else(|| StorageError::Upload(format!("Not found: {key}")))
    }

    async fn delete(&self, key: &str) -> Result<(), StorageError> {
        self.files.lock().unwrap().remove(key);
        Ok(())
    }

    fn extract_key(&self, url: &str) -> Option<String> {
        let prefix = format!("https://mock-storage/{}/", self.bucket_name);
        url.strip_prefix(&prefix).map(|s| s.to_string())
    }

    fn bucket(&self) -> &str {
        &self.bucket_name
    }

    async fn presign_get(
        &self,
        key: &str,
        expiry_seconds: u32,
    ) -> Result<String, alc_core::storage::StorageError> {
        Ok(format!(
            "https://mock-storage/{}/{}?expires={}",
            self.bucket_name, key, expiry_seconds
        ))
    }

    async fn list(&self, prefix: &str) -> Result<Vec<ListedObject>, StorageError> {
        self.list_calls.lock().unwrap().push(prefix.to_string());
        if self.fail_list.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(StorageError::Upload("mock list failure".to_string()));
        }
        // R2 の LIST は content を返さないので、テストでも決定的な擬似 ETag を
        // content から導く (中身が変われば ETag も変わる、が本物の MD5 である必要は無い)。
        Ok(self
            .files
            .lock()
            .unwrap()
            .iter()
            .filter(|(key, _)| key.starts_with(prefix))
            .map(|(key, data)| {
                let mut hasher = std::collections::hash_map::DefaultHasher::new();
                data.hash(&mut hasher);
                ListedObject {
                    key: key.clone(),
                    etag: Some(format!("mock-etag-{:016x}", hasher.finish())),
                }
            })
            .collect())
    }
}
