use alc_core::storage::{StorageBackend, StorageError};
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use std::io::{Read, Write};

pub fn gzip_compress(data: &[u8]) -> anyhow::Result<Vec<u8>> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(data)?;
    Ok(encoder.finish()?)
}

pub fn gzip_decompress(data: &[u8]) -> anyhow::Result<Vec<u8>> {
    let mut decoder = GzDecoder::new(data);
    let mut buf = Vec::new();
    decoder.read_to_end(&mut buf)?;
    Ok(buf)
}

pub async fn upload_compressed(
    storage: &dyn StorageBackend,
    key: &str,
    data: &[u8],
) -> Result<(), StorageError> {
    let compressed =
        gzip_compress(data).map_err(|e| StorageError::Upload(format!("gzip compress: {e}")))?;
    storage.upload(key, &compressed, "application/gzip").await?;
    Ok(())
}

pub async fn upload_json(
    storage: &dyn StorageBackend,
    key: &str,
    data: &[u8],
) -> Result<(), StorageError> {
    storage.upload(key, data, "application/json").await?;
    Ok(())
}

pub async fn download_decompressed(
    storage: &dyn StorageBackend,
    key: &str,
) -> anyhow::Result<Vec<u8>> {
    let compressed = storage
        .download(key)
        .await
        .map_err(|e| anyhow::anyhow!("download {key}: {e}"))?;
    gzip_decompress(&compressed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::archive::test_helpers::TestStorage;
    use std::sync::atomic::Ordering;

    #[test]
    fn test_gzip_roundtrip() {
        let data = b"hello world, test data for gzip";
        let compressed = gzip_compress(data).unwrap();
        assert_ne!(compressed, data.as_slice());
        let decompressed = gzip_decompress(&compressed).unwrap();
        assert_eq!(decompressed, data);
    }

    #[test]
    fn test_gzip_empty() {
        let data = b"";
        let compressed = gzip_compress(data).unwrap();
        let decompressed = gzip_decompress(&compressed).unwrap();
        assert_eq!(decompressed, data);
    }

    #[test]
    fn test_gzip_large_data() {
        let data: Vec<u8> = (0..10_000).map(|i| (i % 256) as u8).collect();
        let compressed = gzip_compress(&data).unwrap();
        assert!(compressed.len() < data.len());
        let decompressed = gzip_decompress(&compressed).unwrap();
        assert_eq!(decompressed, data);
    }

    #[test]
    fn test_gzip_decompress_invalid() {
        let result = gzip_decompress(b"not valid gzip data");
        assert!(result.is_err());
    }

    // async function tests
    #[tokio::test]
    async fn test_upload_compressed() {
        let storage = TestStorage::new();
        upload_compressed(&storage, "test.gz", b"hello")
            .await
            .unwrap();

        let uploaded = storage.get("test.gz").unwrap();
        let decompressed = gzip_decompress(&uploaded).unwrap();
        assert_eq!(decompressed, b"hello");
    }

    #[tokio::test]
    async fn test_upload_compressed_storage_error() {
        let storage = TestStorage::new();
        storage.fail_upload.store(true, Ordering::Relaxed);

        let result = upload_compressed(&storage, "test.gz", b"data").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_upload_json() {
        let storage = TestStorage::new();
        let data = b"{\"key\":\"value\"}";
        upload_json(&storage, "meta.json", data).await.unwrap();

        let uploaded = storage.get("meta.json").unwrap();
        assert_eq!(uploaded, data);
    }

    #[tokio::test]
    async fn test_upload_json_storage_error() {
        let storage = TestStorage::new();
        storage.fail_upload.store(true, Ordering::Relaxed);

        let result = upload_json(&storage, "meta.json", b"data").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_download_decompressed() {
        let storage = TestStorage::new();
        let original = b"decompressed content";
        let compressed = gzip_compress(original).unwrap();
        storage.put("data.gz", compressed);

        let result = download_decompressed(&storage, "data.gz").await.unwrap();
        assert_eq!(result, original);
    }

    #[tokio::test]
    async fn test_download_decompressed_not_found() {
        let storage = TestStorage::new();
        let result = download_decompressed(&storage, "missing.gz").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_download_decompressed_invalid_gzip() {
        let storage = TestStorage::new();
        storage.put("bad.gz", b"not gzip".to_vec());
        let result = download_decompressed(&storage, "bad.gz").await;
        assert!(result.is_err());
    }
}
