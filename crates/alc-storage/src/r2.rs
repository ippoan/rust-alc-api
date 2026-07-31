use alc_core::storage::{ListedObject, StorageBackend, StorageError};
use s3::bucket::Bucket;
use s3::creds::Credentials;
use s3::Region;
use std::collections::HashMap;

pub struct R2Backend {
    bucket: Box<Bucket>,
    bucket_name: String,
    public_url_base: String,
}

impl R2Backend {
    pub fn new(
        bucket_name: String,
        account_id: String,
        access_key: String,
        secret_key: String,
        public_url_base: Option<String>,
    ) -> Result<Self, StorageError> {
        let endpoint = std::env::var("R2_ENDPOINT")
            .unwrap_or_else(|_| format!("https://{}.r2.cloudflarestorage.com", account_id));
        let region = Region::Custom {
            region: "auto".to_string(),
            endpoint,
        };

        let credentials = Credentials::new(Some(&access_key), Some(&secret_key), None, None, None)
            .map_err(|e| StorageError::Config(format!("R2 credentials: {e}")))?;

        let mut bucket = Bucket::new(&bucket_name, region, credentials)
            .map_err(|e| StorageError::Config(format!("R2 bucket: {e}")))?;
        if std::env::var("R2_PATH_STYLE").is_ok() {
            bucket = bucket.with_path_style();
        }

        let public_url_base = public_url_base
            .unwrap_or_else(|| format!("https://{}.r2.dev/{}", account_id, bucket_name));

        Ok(Self {
            bucket,
            bucket_name,
            public_url_base,
        })
    }
}

#[async_trait::async_trait]
impl StorageBackend for R2Backend {
    async fn upload(
        &self,
        key: &str,
        data: &[u8],
        content_type: &str,
    ) -> Result<String, StorageError> {
        let response = self
            .bucket
            .put_object_with_content_type(key, data, content_type)
            .await
            .map_err(|e| StorageError::Upload(format!("R2 upload: {e}")))?;

        let status = response.status_code();
        if !(200..300).contains(&status) {
            return Err(StorageError::Upload(format!(
                "R2 upload status {}: {}",
                status,
                String::from_utf8_lossy(response.as_slice())
            )));
        }

        tracing::info!("R2 upload: bucket={}, key={}", self.bucket_name, key);
        Ok(self.public_url(key))
    }

    fn public_url(&self, key: &str) -> String {
        format!("{}/{}", self.public_url_base, key)
    }

    async fn exists(&self, key: &str) -> Result<bool, StorageError> {
        match self.bucket.head_object(key).await {
            Ok((_, status)) => Ok((200..300).contains(&status)),
            Err(s3::error::S3Error::HttpFailWithBody(404, _))
            | Err(s3::error::S3Error::HttpFail) => Ok(false),
            Err(e) => Err(StorageError::Upload(format!("R2 head: {e}"))),
        }
    }

    async fn download(&self, key: &str) -> Result<Vec<u8>, StorageError> {
        let response = self
            .bucket
            .get_object(key)
            .await
            .map_err(|e| StorageError::Upload(format!("R2 download: {e}")))?;

        let status = response.status_code();
        if !(200..300).contains(&status) {
            return Err(StorageError::Upload(format!(
                "R2 download status {}: {}",
                status,
                String::from_utf8_lossy(response.as_slice())
            )));
        }

        Ok(response.to_vec())
    }

    async fn delete(&self, key: &str) -> Result<(), StorageError> {
        let response = self
            .bucket
            .delete_object(key)
            .await
            .map_err(|e| StorageError::Upload(format!("R2 delete: {e}")))?;

        let status = response.status_code();
        if !(200..300).contains(&status) && status != 404 {
            return Err(StorageError::Upload(format!(
                "R2 delete status {}: {}",
                status,
                String::from_utf8_lossy(response.as_slice())
            )));
        }

        tracing::info!("R2 delete: bucket={}, key={}", self.bucket_name, key);
        Ok(())
    }

    fn extract_key(&self, url: &str) -> Option<String> {
        let prefix = format!("{}/", self.public_url_base);
        url.strip_prefix(&prefix).map(|s| s.to_string())
    }

    fn bucket(&self) -> &str {
        &self.bucket_name
    }

    async fn list(&self, prefix: &str) -> Result<Vec<ListedObject>, StorageError> {
        // `Bucket::list` は ListObjectsV2 を発行し、continuation token を内部で
        // 辿って全ページ分の `Vec<ListBucketResult>` を返す (1 論理呼び出しで完結)。
        // per-object HEAD は一切行わない — 各ページの `Contents[].ETag` を使う。
        let pages = self
            .bucket
            .list(prefix.to_string(), None)
            .await
            .map_err(|e| StorageError::Upload(format!("R2 list: {e}")))?;

        Ok(to_listed_objects(
            pages.into_iter().flat_map(|page| page.contents).collect(),
        ))
    }

    async fn presign_get(&self, key: &str, expiry_seconds: u32) -> Result<String, StorageError> {
        // response-content-disposition=inline でブラウザ・webview に強制 inline 表示。
        // 添付ダウンロードに行かず、PDF/画像が webview 内で開く。
        let mut queries = HashMap::new();
        queries.insert(
            "response-content-disposition".to_string(),
            "inline".to_string(),
        );
        self.bucket
            .presign_get(key, expiry_seconds, Some(queries))
            .await
            .map_err(|e| StorageError::Upload(format!("R2 presign_get: {e}")))
    }
}

/// `s3::serde_types::Object` (LIST の 1 件) → `ListedObject`。ネットワークを
/// 挟まない純粋関数として切り出してあるので、実 R2 に繋がず ETag のクォート剥がし
/// だけを単体テストできる (このクレートに R2 互換モックの前例が無いため)。
fn to_listed_objects(contents: Vec<s3::serde_types::Object>) -> Vec<ListedObject> {
    contents
        .into_iter()
        .map(|obj| ListedObject {
            key: obj.key,
            // S3 系 LIST の ETag は `"abc123"` のように引用符付きで返る。
            // 呼び出し側が生の digest として比較できるよう剥がしておく。
            etag: obj.e_tag.map(|e| e.trim_matches('"').to_string()),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn object(key: &str, e_tag: Option<&str>) -> s3::serde_types::Object {
        s3::serde_types::Object {
            last_modified: "2026-06-15T00:00:00.000Z".to_string(),
            e_tag: e_tag.map(|s| s.to_string()),
            storage_class: Some("STANDARD".to_string()),
            key: key.to_string(),
            owner: None,
            size: 123,
        }
    }

    #[test]
    fn strips_surrounding_quotes_from_etag() {
        let listed = to_listed_objects(vec![object("t/unko/U1/KUDGIVT.csv", Some("\"abc123\""))]);
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].key, "t/unko/U1/KUDGIVT.csv");
        assert_eq!(listed[0].etag.as_deref(), Some("abc123"));
    }

    #[test]
    fn passes_through_missing_etag_as_none() {
        let listed = to_listed_objects(vec![object("t/unko/U1/KUDGIVT.csv", None)]);
        assert_eq!(listed[0].etag, None);
    }

    #[test]
    fn maps_multiple_objects_in_order() {
        let listed = to_listed_objects(vec![
            object("t/unko/U1/KUDGIVT.csv", Some("\"a\"")),
            object("t/unko/U2/KUDGIVT.csv", Some("\"b\"")),
        ]);
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].key, "t/unko/U1/KUDGIVT.csv");
        assert_eq!(listed[1].key, "t/unko/U2/KUDGIVT.csv");
    }
}
