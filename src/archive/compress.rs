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
