//! `HttpProxyBackend` — Incus dev サンドボックス用の R2 アクセス backend。
//!
//! ホスト側で wrangler dev に R2 binding を持つ極小 worker を立て (例: `~/js/.dev-proxy/r2-proxy/`)、
//! Incus 内の rust-alc-api は本 backend 経由でその HTTP サーバー越しに R2 を読み書きする。
//! これにより本番 R2 keys を Incus に投入することなく、ホストの wrangler CLI 認証だけで
//! R2 アクセスが完結する。
//!
//! GET / HEAD / PUT / DELETE をサポート。dev 環境専用。`presign_get` は未対応 (proxy では
//! 署名 URL を発行できないため、必要なら別途公開 URL を組む)。

use alc_core::storage::{StorageBackend, StorageError};

#[derive(Clone)]
pub struct HttpProxyBackend {
    base: String,
    client: reqwest::Client,
    // public_url で返す bucket 名 (情報用、本番動作には使われない)
    bucket: String,
}

impl HttpProxyBackend {
    /// `base` は `http://10.10.10.1:8788` 等のプロキシ URL (末尾 `/` 任意)。
    /// `bucket` はログ表示用 (例: `dtako-uploads`)。
    pub fn new(base: impl Into<String>, bucket: impl Into<String>) -> Self {
        Self {
            base: base.into().trim_end_matches('/').to_string(),
            client: reqwest::Client::new(),
            bucket: bucket.into(),
        }
    }

    fn url(&self, key: &str) -> String {
        // key 自体に `/` が含まれるので、URL エンコードは行わずそのまま path に流す。
        format!("{}/{}", self.base, key)
    }
}

#[async_trait::async_trait]
impl StorageBackend for HttpProxyBackend {
    async fn upload(
        &self,
        key: &str,
        data: &[u8],
        content_type: &str,
    ) -> Result<String, StorageError> {
        let resp = self
            .client
            .put(self.url(key))
            .header("content-type", content_type)
            .body(data.to_vec())
            .send()
            .await
            .map_err(|e| StorageError::Upload(format!("proxy PUT: {e}")))?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(StorageError::Upload(format!(
                "proxy PUT status {status}: {body}"
            )));
        }
        Ok(self.public_url(key))
    }

    fn public_url(&self, key: &str) -> String {
        format!("{}/{}", self.base, key)
    }

    async fn exists(&self, key: &str) -> Result<bool, StorageError> {
        let resp = self
            .client
            .head(self.url(key))
            .send()
            .await
            .map_err(|e| StorageError::Upload(format!("proxy HEAD: {e}")))?;
        Ok(resp.status().is_success())
    }

    async fn download(&self, key: &str) -> Result<Vec<u8>, StorageError> {
        let resp = self
            .client
            .get(self.url(key))
            .send()
            .await
            .map_err(|e| StorageError::Upload(format!("proxy GET: {e}")))?;
        let status = resp.status();
        if !status.is_success() {
            return Err(StorageError::Upload(format!(
                "proxy GET status {status}: {key}"
            )));
        }
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| StorageError::Upload(format!("proxy body: {e}")))?;
        Ok(bytes.to_vec())
    }

    async fn delete(&self, key: &str) -> Result<(), StorageError> {
        let resp = self
            .client
            .delete(self.url(key))
            .send()
            .await
            .map_err(|e| StorageError::Upload(format!("proxy DELETE: {e}")))?;
        let status = resp.status();
        if !status.is_success() && status.as_u16() != 404 {
            let body = resp.text().await.unwrap_or_default();
            return Err(StorageError::Upload(format!(
                "proxy DELETE status {status}: {body}"
            )));
        }
        Ok(())
    }

    fn extract_key(&self, url: &str) -> Option<String> {
        let prefix = format!("{}/", self.base);
        url.strip_prefix(&prefix).map(|s| s.to_string())
    }

    fn bucket(&self) -> &str {
        &self.bucket
    }

    async fn presign_get(&self, _key: &str, _expiry_seconds: u32) -> Result<String, StorageError> {
        Err(StorageError::Config(
            "HttpProxyBackend does not support presign_get".to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_strips_trailing_slash_in_base() {
        let b = HttpProxyBackend::new("http://host:8788/", "dtako-uploads");
        assert_eq!(b.url("foo/bar.csv"), "http://host:8788/foo/bar.csv");
    }

    #[test]
    fn public_url_concatenates_base_and_key() {
        let b = HttpProxyBackend::new("http://host:8788", "dtako-uploads");
        assert_eq!(b.public_url("foo/bar.csv"), "http://host:8788/foo/bar.csv");
    }

    #[test]
    fn extract_key_strips_base_prefix() {
        let b = HttpProxyBackend::new("http://host:8788", "dtako-uploads");
        assert_eq!(
            b.extract_key("http://host:8788/foo/bar.csv"),
            Some("foo/bar.csv".to_string())
        );
        assert_eq!(b.extract_key("http://other/foo"), None);
    }

    #[test]
    fn bucket_returns_label() {
        let b = HttpProxyBackend::new("http://host:8788", "my-bucket");
        assert_eq!(b.bucket(), "my-bucket");
    }
}
