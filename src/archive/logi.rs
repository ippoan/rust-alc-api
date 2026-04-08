use crate::archive::compress::{upload_compressed, upload_json};
use crate::archive::repo::ArchiveDb;
use crate::archive::schema_meta::fetch_schema_metadata;
use alc_core::storage::StorageBackend;

const BATCH_SIZE: i64 = 10_000;
const MAX_ROWS_PER_FILE: usize = 100_000;

pub async fn logi_dump(
    db: &dyn ArchiveDb,
    storage: &dyn StorageBackend,
    dry_run: bool,
) -> anyhow::Result<()> {
    logi_dump_inner(db, storage, dry_run, MAX_ROWS_PER_FILE).await
}

async fn logi_dump_inner(
    db: &dyn ArchiveDb,
    storage: &dyn StorageBackend,
    dry_run: bool,
    max_rows_per_file: usize,
) -> anyhow::Result<()> {
    let tables = db.list_tables("logi").await?;

    if tables.is_empty() {
        println!("No tables found in logi schema");
        return Ok(());
    }

    println!("Found {} tables in logi schema:", tables.len());
    for name in &tables {
        let count = db.count_rows("logi", name).await?;
        println!("  {} — {} rows", name, count);
    }

    if dry_run {
        println!("\n[dry-run] Would archive all tables above to R2");
        return Ok(());
    }

    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();

    for table_name in &tables {
        println!("\nArchiving logi.{} ...", table_name);

        let meta = fetch_schema_metadata(db, "logi", table_name, 1, vec![]).await?;
        let meta_json = serde_json::to_vec_pretty(&meta)?;
        let meta_key = format!("archive/logi/{}/schema_v1.json", table_name);
        upload_json(storage, &meta_key, &meta_json).await?;
        println!("  schema → {}", meta_key);

        let total = db.count_rows("logi", table_name).await? as usize;

        let mut offset: i64 = 0;
        let mut file_idx = 0u32;
        let mut file_rows = 0usize;
        let mut buffer = Vec::new();

        let header = serde_json::json!({
            "_archive_header": true,
            "schema_version": 1,
            "table": format!("logi.{}", table_name),
            "archived_at": chrono::Utc::now().to_rfc3339(),
        });
        buffer.extend_from_slice(serde_json::to_string(&header)?.as_bytes());
        buffer.push(b'\n');

        loop {
            let rows = db
                .fetch_rows_json("logi", table_name, BATCH_SIZE, offset)
                .await?;

            if rows.is_empty() {
                break;
            }

            for row in &rows {
                buffer.extend_from_slice(serde_json::to_string(row)?.as_bytes());
                buffer.push(b'\n');
                file_rows += 1;

                if file_rows >= max_rows_per_file {
                    let suffix = if total > max_rows_per_file {
                        format!("_part{}", file_idx)
                    } else {
                        String::new()
                    };
                    let key = format!(
                        "archive/logi/{}/full_{}{}.jsonl.gz",
                        table_name, today, suffix
                    );
                    upload_compressed(storage, &key, &buffer).await?;
                    println!("  {} rows → {}", file_rows, key);

                    buffer.clear();
                    buffer.extend_from_slice(serde_json::to_string(&header)?.as_bytes());
                    buffer.push(b'\n');
                    file_rows = 0;
                    file_idx += 1;
                }
            }

            offset += rows.len() as i64;
        }

        if file_rows > 0 {
            let suffix = if file_idx > 0 {
                format!("_part{}", file_idx)
            } else {
                String::new()
            };
            let key = format!(
                "archive/logi/{}/full_{}{}.jsonl.gz",
                table_name, today, suffix
            );
            upload_compressed(storage, &key, &buffer).await?;
            println!("  {} rows → {}", file_rows, key);
        }

        println!("  ✓ logi.{} archived ({} rows total)", table_name, total);
    }

    println!("\n=== logi schema dump complete ===");
    println!("To drop the logi schema, run:");
    println!("  DROP SCHEMA logi CASCADE;");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::archive::compress::gzip_decompress;
    use crate::archive::repo::mock::MockArchiveDb;
    use crate::archive::test_helpers::TestStorage;

    #[tokio::test]
    async fn test_logi_dump_no_tables() {
        let db = MockArchiveDb::default();
        let storage = TestStorage::new();
        let result = logi_dump(&db, &storage, false).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_logi_dump_dry_run() {
        let db = MockArchiveDb::default();
        *db.tables.lock().unwrap() = vec!["dtakologs".into()];
        db.row_counts
            .lock()
            .unwrap()
            .insert("dtakologs".into(), 100);
        let storage = TestStorage::new();

        let result = logi_dump(&db, &storage, true).await;
        assert!(result.is_ok());
        // Dry run should not upload anything
        assert!(storage.keys().is_empty());
    }

    #[tokio::test]
    async fn test_logi_dump_single_table() {
        let db = MockArchiveDb::default();
        *db.tables.lock().unwrap() = vec!["test_table".into()];
        db.row_counts.lock().unwrap().insert("test_table".into(), 2);
        *db.columns.lock().unwrap() = vec![("id".into(), "INTEGER".into(), "NO".into(), None)];
        *db.primary_key.lock().unwrap() = vec!["id".into()];
        *db.rows.lock().unwrap() = vec![
            serde_json::json!({"id": 1, "name": "a"}),
            serde_json::json!({"id": 2, "name": "b"}),
        ];
        let storage = TestStorage::new();

        let result = logi_dump(&db, &storage, false).await;
        assert!(result.is_ok());

        // Schema file uploaded
        assert!(storage
            .get("archive/logi/test_table/schema_v1.json")
            .is_some());

        // Data file uploaded (gzipped)
        let keys: Vec<String> = storage
            .keys()
            .into_iter()
            .filter(|k| k.ends_with(".jsonl.gz"))
            .collect();
        assert_eq!(keys.len(), 1);

        // Verify content
        let compressed = storage.get(&keys[0]).unwrap();
        let data = gzip_decompress(&compressed).unwrap();
        let lines: Vec<&str> = std::str::from_utf8(&data).unwrap().lines().collect();
        assert_eq!(lines.len(), 3); // header + 2 rows
    }

    #[tokio::test]
    async fn test_logi_dump_db_error() {
        use std::sync::atomic::Ordering;
        let db = MockArchiveDb::default();
        db.fail_next.store(true, Ordering::SeqCst);
        let storage = TestStorage::new();

        let result = logi_dump(&db, &storage, false).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_logi_dump_split_files() {
        let db = MockArchiveDb::default();
        *db.tables.lock().unwrap() = vec!["big_table".into()];
        db.row_counts.lock().unwrap().insert("big_table".into(), 5);
        *db.columns.lock().unwrap() = vec![("id".into(), "INTEGER".into(), "NO".into(), None)];
        *db.primary_key.lock().unwrap() = vec!["id".into()];
        *db.rows.lock().unwrap() = (1..=5).map(|i| serde_json::json!({"id": i})).collect();
        let storage = TestStorage::new();

        // max_rows_per_file=2 → 3 files: part0(2), part1(2), part2(1)
        let result = logi_dump_inner(&db, &storage, false, 2).await;
        assert!(result.is_ok());

        let data_keys: Vec<String> = storage
            .keys()
            .into_iter()
            .filter(|k| k.ends_with(".jsonl.gz"))
            .collect();
        assert_eq!(data_keys.len(), 3);

        assert!(data_keys.iter().any(|k| k.contains("_part0")));
        assert!(data_keys.iter().any(|k| k.contains("_part1")));
        assert!(data_keys.iter().any(|k| k.contains("_part2")));

        // Verify each file has header + data rows
        for key in &data_keys {
            let compressed = storage.get(key).unwrap();
            let data = gzip_decompress(&compressed).unwrap();
            let lines: Vec<&str> = std::str::from_utf8(&data).unwrap().lines().collect();
            assert!(lines.len() >= 2); // header + at least 1 row
        }
    }

    #[tokio::test]
    async fn test_logi_dump_exact_boundary() {
        let db = MockArchiveDb::default();
        *db.tables.lock().unwrap() = vec!["t".into()];
        db.row_counts.lock().unwrap().insert("t".into(), 2);
        *db.columns.lock().unwrap() = vec![("id".into(), "INTEGER".into(), "NO".into(), None)];
        *db.primary_key.lock().unwrap() = vec!["id".into()];
        *db.rows.lock().unwrap() = vec![serde_json::json!({"id": 1}), serde_json::json!({"id": 2})];
        let storage = TestStorage::new();

        // total == max_rows_per_file: split triggers but no _part suffix
        let result = logi_dump_inner(&db, &storage, false, 2).await;
        assert!(result.is_ok());

        let data_keys: Vec<String> = storage
            .keys()
            .into_iter()
            .filter(|k| k.ends_with(".jsonl.gz"))
            .collect();
        assert_eq!(data_keys.len(), 1);
        assert!(!data_keys[0].contains("_part"));
    }

    #[tokio::test]
    async fn test_logi_dump_multiple_tables() {
        let db = MockArchiveDb::default();
        *db.tables.lock().unwrap() = vec!["table_a".into(), "table_b".into()];
        db.row_counts.lock().unwrap().insert("table_a".into(), 1);
        db.row_counts.lock().unwrap().insert("table_b".into(), 1);
        *db.columns.lock().unwrap() = vec![("x".into(), "TEXT".into(), "NO".into(), None)];
        *db.rows.lock().unwrap() = vec![serde_json::json!({"x": "val"})];
        let storage = TestStorage::new();

        let result = logi_dump(&db, &storage, false).await;
        assert!(result.is_ok());

        // 2 tables × (schema + data) = 4 files
        assert_eq!(storage.keys().len(), 4);
    }
}
