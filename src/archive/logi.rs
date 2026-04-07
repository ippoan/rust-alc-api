use crate::archive::compress::{upload_compressed, upload_json};
use crate::archive::schema_meta::fetch_schema_metadata;
use alc_core::storage::StorageBackend;
use sqlx::PgPool;

const BATCH_SIZE: i64 = 10_000;
const MAX_ROWS_PER_FILE: usize = 100_000;

pub async fn logi_dump(
    pool: &PgPool,
    storage: &dyn StorageBackend,
    dry_run: bool,
) -> anyhow::Result<()> {
    // Discover all tables in logi schema
    let tables = sqlx::query_as::<_, (String,)>(
        "SELECT tablename FROM pg_tables WHERE schemaname = 'logi' ORDER BY tablename",
    )
    .fetch_all(pool)
    .await?;

    if tables.is_empty() {
        println!("No tables found in logi schema");
        return Ok(());
    }

    println!("Found {} tables in logi schema:", tables.len());
    for (name,) in &tables {
        let count: (i64,) = sqlx::query_as(&format!("SELECT count(*) FROM logi.\"{}\"", name))
            .fetch_one(pool)
            .await?;
        println!("  {} — {} rows", name, count.0);
    }

    if dry_run {
        println!("\n[dry-run] Would archive all tables above to R2");
        return Ok(());
    }

    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();

    for (table_name,) in &tables {
        println!("\nArchiving logi.{} ...", table_name);

        // Upload schema metadata
        let meta = fetch_schema_metadata(pool, "logi", table_name, 1, vec![]).await?;
        let meta_json = serde_json::to_vec_pretty(&meta)?;
        let meta_key = format!("archive/logi/{}/schema_v1.json", table_name);
        upload_json(storage, &meta_key, &meta_json).await?;
        println!("  schema → {}", meta_key);

        // Stream rows in batches, split into files of MAX_ROWS_PER_FILE
        let total_count: (i64,) =
            sqlx::query_as(&format!("SELECT count(*) FROM logi.\"{}\"", table_name))
                .fetch_one(pool)
                .await?;
        let total = total_count.0 as usize;

        let mut offset: i64 = 0;
        let mut file_idx = 0u32;
        let mut file_rows = 0usize;
        let mut buffer = Vec::new();

        // Write archive header as first line
        let header = serde_json::json!({
            "_archive_header": true,
            "schema_version": 1,
            "table": format!("logi.{}", table_name),
            "archived_at": chrono::Utc::now().to_rfc3339(),
        });
        buffer.extend_from_slice(serde_json::to_string(&header)?.as_bytes());
        buffer.push(b'\n');

        loop {
            let rows: Vec<(serde_json::Value,)> = sqlx::query_as(&format!(
                "SELECT row_to_json(t) FROM logi.\"{}\" t ORDER BY ctid LIMIT {} OFFSET {}",
                table_name, BATCH_SIZE, offset
            ))
            .fetch_all(pool)
            .await?;

            if rows.is_empty() {
                break;
            }

            for (row,) in &rows {
                buffer.extend_from_slice(serde_json::to_string(row)?.as_bytes());
                buffer.push(b'\n');
                file_rows += 1;

                if file_rows >= MAX_ROWS_PER_FILE {
                    let suffix = if total > MAX_ROWS_PER_FILE {
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
                    // New file header
                    buffer.extend_from_slice(serde_json::to_string(&header)?.as_bytes());
                    buffer.push(b'\n');
                    file_rows = 0;
                    file_idx += 1;
                }
            }

            offset += rows.len() as i64;
        }

        // Flush remaining
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
