use crate::archive::compress::{download_decompressed, upload_compressed, upload_json};
use crate::archive::repo::ArchiveDb;
use crate::archive::schema_meta::{fetch_schema_metadata, SchemaMetadata};
use alc_core::storage::StorageBackend;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

const BATCH_SIZE: i64 = 10_000;

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Manifest {
    pub updated_at: String,
    pub archived_dates: HashMap<String, HashMap<String, ArchivedDateInfo>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ArchivedDateInfo {
    pub row_count: usize,
    pub archived_at: String,
    pub r2_key: String,
}

const MANIFEST_KEY: &str = "archive/alc_api/dtakologs/_manifest.json";
const SCHEMA_KEY: &str = "archive/alc_api/dtakologs/schema_v1.json";

pub async fn load_manifest(storage: &dyn StorageBackend) -> Manifest {
    match storage.download(MANIFEST_KEY).await {
        Ok(data) => serde_json::from_slice(&data).unwrap_or_default(),
        Err(_) => Manifest::default(),
    }
}

async fn save_manifest(storage: &dyn StorageBackend, manifest: &Manifest) -> anyhow::Result<()> {
    let json = serde_json::to_vec_pretty(manifest)?;
    upload_json(storage, MANIFEST_KEY, &json).await?;
    Ok(())
}

pub fn make_r2_key(tenant_id: &str, date_str: &str) -> String {
    format!(
        "archive/alc_api/dtakologs/{}/{}/{}/{}.jsonl.gz",
        tenant_id,
        &date_str[..4],
        &date_str[5..7],
        &date_str[8..10]
    )
}

pub fn build_jsonl_buffer(
    rows: &[serde_json::Value],
    header: &serde_json::Value,
) -> anyhow::Result<Vec<u8>> {
    let mut buffer = Vec::new();
    buffer.extend_from_slice(serde_json::to_string(header)?.as_bytes());
    buffer.push(b'\n');
    for row in rows {
        buffer.extend_from_slice(serde_json::to_string(row)?.as_bytes());
        buffer.push(b'\n');
    }
    Ok(buffer)
}

pub async fn dtako_archive(
    db: &dyn ArchiveDb,
    storage: &dyn StorageBackend,
    dry_run: bool,
) -> anyhow::Result<()> {
    dtako_archive_range(db, storage, dry_run, None, None).await
}

pub async fn dtako_archive_range(
    db: &dyn ArchiveDb,
    storage: &dyn StorageBackend,
    dry_run: bool,
    start_date: Option<&str>,
    end_date: Option<&str>,
) -> anyhow::Result<()> {
    let mut manifest = load_manifest(storage).await;

    if !dry_run {
        let meta = fetch_schema_metadata(
            db,
            "alc_api",
            "dtakologs",
            1,
            vec![
                "066_create_dtakologs.sql".to_string(),
                "068_dtakologs_gps_to_float.sql".to_string(),
            ],
        )
        .await?;
        let meta_json = serde_json::to_vec_pretty(&meta)?;
        upload_json(storage, SCHEMA_KEY, &meta_json).await?;
    }

    let dates_to_archive = db.list_dtako_dates().await?;

    println!("=== Phase A: Archive to R2 ===");

    let mut archived_count = 0usize;
    for (tenant_id, date_str, count) in &dates_to_archive {
        if let Some(sd) = start_date {
            if date_str.as_str() < sd {
                continue;
            }
        }
        if let Some(ed) = end_date {
            if date_str.as_str() > ed {
                continue;
            }
        }

        let already_archived = manifest
            .archived_dates
            .get(tenant_id)
            .and_then(|dates| dates.get(date_str))
            .is_some();

        if already_archived {
            continue;
        }

        let r2_key = make_r2_key(tenant_id, date_str);

        if dry_run {
            println!(
                "  [dry-run] Would archive {} rows for tenant={} date={} → {}",
                count, tenant_id, date_str, r2_key
            );
            archived_count += *count as usize;
            continue;
        }

        let mut all_rows = Vec::new();
        let mut offset: i64 = 0;
        loop {
            let rows = db
                .fetch_dtako_rows_json(tenant_id, date_str, BATCH_SIZE, offset)
                .await?;
            if rows.is_empty() {
                break;
            }
            offset += rows.len() as i64;
            all_rows.extend(rows);
        }

        let header = serde_json::json!({
            "_archive_header": true,
            "schema_version": 1,
            "table": "alc_api.dtakologs",
            "tenant_id": tenant_id,
            "date": date_str,
            "archived_at": chrono::Utc::now().to_rfc3339(),
        });
        let buffer = build_jsonl_buffer(&all_rows, &header)?;
        let row_count = all_rows.len();

        upload_compressed(storage, &r2_key, &buffer).await?;

        manifest
            .archived_dates
            .entry(tenant_id.clone())
            .or_default()
            .insert(
                date_str.clone(),
                ArchivedDateInfo {
                    row_count,
                    archived_at: chrono::Utc::now().to_rfc3339(),
                    r2_key: r2_key.clone(),
                },
            );

        println!("  {} rows → {}", row_count, r2_key);
        archived_count += row_count;

        // Save manifest after each date to avoid losing progress on timeout
        manifest.updated_at = chrono::Utc::now().to_rfc3339();
        save_manifest(storage, &manifest).await?;
    }
    println!("Phase A: {} rows archived", archived_count);

    println!("\n=== Phase B: DELETE old rows from DB ===");

    let cutoff = (chrono::Utc::now() - chrono::Duration::days(7))
        .format("%Y-%m-%d")
        .to_string();

    let old_dates = db.list_old_dtako_dates(&cutoff).await?;

    let mut deleted_count = 0i64;
    for (tenant_id, date_str, count) in &old_dates {
        if let Some(sd) = start_date {
            if date_str.as_str() < sd {
                continue;
            }
        }
        if let Some(ed) = end_date {
            if date_str.as_str() > ed {
                continue;
            }
        }

        let in_manifest = manifest
            .archived_dates
            .get(tenant_id)
            .and_then(|dates| dates.get(date_str))
            .is_some();

        if !in_manifest {
            println!(
                "  SKIP tenant={} date={} ({} rows) — not in manifest",
                tenant_id, date_str, count
            );
            continue;
        }

        if dry_run {
            println!(
                "  [dry-run] Would DELETE {} rows for tenant={} date={}",
                count, tenant_id, date_str
            );
            deleted_count += count;
            continue;
        }

        let affected = db.delete_dtako_date(tenant_id, date_str).await?;
        println!(
            "  DELETE {} rows for tenant={} date={}",
            affected, tenant_id, date_str
        );
        deleted_count += affected as i64;
    }

    println!(
        "Phase B: {} rows deleted (cutoff={})",
        deleted_count, cutoff
    );

    Ok(())
}

pub async fn dtako_restore(
    db: &dyn ArchiveDb,
    storage: &dyn StorageBackend,
    tenant_id: &str,
    date: &str,
) -> anyhow::Result<()> {
    let current_meta = fetch_schema_metadata(db, "alc_api", "dtakologs", 1, vec![]).await?;

    match storage.download(SCHEMA_KEY).await {
        Ok(data) => {
            let archived_meta: SchemaMetadata = serde_json::from_slice(&data)?;
            compare_schemas(&archived_meta, &current_meta);
        }
        Err(_) => {
            println!("WARNING: No archived schema metadata found, proceeding anyway");
        }
    }

    let r2_key = make_r2_key(tenant_id, date);

    println!("Downloading {} ...", r2_key);
    let data = download_decompressed(storage, &r2_key).await?;
    let content = String::from_utf8(data)?;

    let mut restored = 0usize;
    let mut batch_values: Vec<String> = Vec::new();

    for line in content.lines() {
        if line.is_empty() {
            continue;
        }

        let value: serde_json::Value = serde_json::from_str(line)?;

        if value.get("_archive_header").is_some() {
            continue;
        }

        batch_values.push(line.to_string());
        restored += 1;

        if batch_values.len() >= 500 {
            db.upsert_dtako_batch(&batch_values).await?;
            batch_values.clear();
        }
    }

    if !batch_values.is_empty() {
        db.upsert_dtako_batch(&batch_values).await?;
    }

    println!("Restored {} rows from {}", restored, r2_key);
    Ok(())
}

pub async fn fetch_from_r2(
    storage: &dyn StorageBackend,
    tenant_id: &str,
    start_date: &str,
    end_date: &str,
    vehicle_cd: Option<i32>,
) -> anyhow::Result<Vec<alc_core::models::DtakologRow>> {
    let manifest = load_manifest(storage).await;
    let tenant_dates = match manifest.archived_dates.get(tenant_id) {
        Some(dates) => dates,
        None => return Ok(vec![]),
    };

    let mut all_rows = Vec::new();

    for (date_str, info) in tenant_dates {
        if date_str.as_str() < start_date || date_str.as_str() > end_date {
            continue;
        }

        let data = match download_decompressed(storage, &info.r2_key).await {
            Ok(d) => d,
            Err(e) => {
                tracing::warn!("Failed to download archive {}: {}", info.r2_key, e);
                continue;
            }
        };

        let content = String::from_utf8_lossy(&data);
        for line in content.lines() {
            if line.is_empty() {
                continue;
            }
            let value: serde_json::Value = match serde_json::from_str(line) {
                Ok(v) => v,
                Err(_) => continue,
            };
            if value.get("_archive_header").is_some() {
                continue;
            }

            if let Some(vc) = vehicle_cd {
                if value.get("vehicle_cd").and_then(|v| v.as_i64()) != Some(vc as i64) {
                    continue;
                }
            }

            all_rows.push(json_to_dtakolog_row(&value));
        }
    }

    all_rows.sort_by(|a, b| a.data_date_time.cmp(&b.data_date_time));
    Ok(all_rows)
}

pub fn json_to_dtakolog_row(v: &serde_json::Value) -> alc_core::models::DtakologRow {
    alc_core::models::DtakologRow {
        gps_direction: v
            .get("gps_direction")
            .and_then(|x| x.as_f64())
            .unwrap_or(0.0),
        gps_latitude: v
            .get("gps_latitude")
            .and_then(|x| x.as_f64())
            .unwrap_or(0.0),
        gps_longitude: v
            .get("gps_longitude")
            .and_then(|x| x.as_f64())
            .unwrap_or(0.0),
        vehicle_cd: v.get("vehicle_cd").and_then(|x| x.as_i64()).unwrap_or(0) as i32,
        vehicle_name: v
            .get("vehicle_name")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string(),
        driver_name: v
            .get("driver_name")
            .and_then(|x| x.as_str())
            .map(|s| s.to_string()),
        address_disp_c: v
            .get("address_disp_c")
            .and_then(|x| x.as_str())
            .map(|s| s.to_string()),
        data_date_time: v
            .get("data_date_time")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string(),
        address_disp_p: v
            .get("address_disp_p")
            .and_then(|x| x.as_str())
            .map(|s| s.to_string()),
        sub_driver_cd: v.get("sub_driver_cd").and_then(|x| x.as_i64()).unwrap_or(0) as i32,
        all_state: v
            .get("all_state")
            .and_then(|x| x.as_str())
            .map(|s| s.to_string()),
        recive_type_color_name: v
            .get("recive_type_color_name")
            .and_then(|x| x.as_str())
            .map(|s| s.to_string()),
        all_state_ex: v
            .get("all_state_ex")
            .and_then(|x| x.as_str())
            .map(|s| s.to_string()),
        state2: v
            .get("state2")
            .and_then(|x| x.as_str())
            .map(|s| s.to_string()),
        all_state_font_color: v
            .get("all_state_font_color")
            .and_then(|x| x.as_str())
            .map(|s| s.to_string()),
        speed: v.get("speed").and_then(|x| x.as_f64()).unwrap_or(0.0) as f32,
    }
}

pub fn compare_schemas(archived: &SchemaMetadata, current: &SchemaMetadata) {
    let archived_cols: std::collections::HashSet<&str> =
        archived.columns.iter().map(|c| c.name.as_str()).collect();
    let current_cols: std::collections::HashSet<&str> =
        current.columns.iter().map(|c| c.name.as_str()).collect();

    let added: Vec<&&str> = current_cols.difference(&archived_cols).collect();
    let removed: Vec<&&str> = archived_cols.difference(&current_cols).collect();

    if added.is_empty() && removed.is_empty() {
        println!(
            "Schema: OK (archived v{} matches current)",
            archived.version
        );
    } else {
        println!("Schema diff detected (archived v{}):", archived.version);
        for col in &added {
            println!("  + {} (new in current, will use default)", col);
        }
        for col in &removed {
            println!("  - {} (removed from current, will be ignored)", col);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::archive::compress::gzip_compress;
    use crate::archive::repo::mock::MockArchiveDb;
    use crate::archive::schema_meta::ColumnInfo;
    use crate::archive::test_helpers::TestStorage;

    #[test]
    fn test_make_r2_key() {
        assert_eq!(
            make_r2_key("t1", "2026-04-07"),
            "archive/alc_api/dtakologs/t1/2026/04/07.jsonl.gz"
        );
    }

    #[test]
    fn test_build_jsonl_buffer() {
        let header = serde_json::json!({"_archive_header": true});
        let rows = vec![serde_json::json!({"id": 1}), serde_json::json!({"id": 2})];
        let buf = build_jsonl_buffer(&rows, &header).unwrap();
        let s = String::from_utf8(buf).unwrap();
        let lines: Vec<&str> = s.lines().collect();
        assert_eq!(lines.len(), 3);
        assert!(lines[0].contains("_archive_header"));
    }

    #[test]
    fn test_json_to_dtakolog_row_defaults() {
        let v = serde_json::json!({});
        let row = json_to_dtakolog_row(&v);
        assert_eq!(row.vehicle_cd, 0);
        assert_eq!(row.speed, 0.0);
        assert!(row.driver_name.is_none());
    }

    #[test]
    fn test_json_to_dtakolog_row_full() {
        let v = serde_json::json!({
            "gps_direction": 90.0, "gps_latitude": 35.68, "gps_longitude": 139.69,
            "vehicle_cd": 10, "vehicle_name": "truck",
            "driver_name": "tanaka", "data_date_time": "2026-04-07",
            "speed": 60.5, "sub_driver_cd": 2,
            "all_state": "Drive", "state2": "running",
            "all_state_font_color": "red", "address_disp_p": "Tokyo",
            "address_disp_c": "Chiyoda", "recive_type_color_name": "blue",
            "all_state_ex": "extra"
        });
        let row = json_to_dtakolog_row(&v);
        assert_eq!(row.vehicle_cd, 10);
        assert_eq!(row.speed, 60.5);
        assert_eq!(row.driver_name.as_deref(), Some("tanaka"));
    }

    #[test]
    fn test_compare_schemas_identical() {
        let meta = SchemaMetadata {
            version: 1,
            created_at: String::new(),
            table_name: "t".into(),
            schema_name: "s".into(),
            primary_key: vec![],
            columns: vec![ColumnInfo {
                name: "a".into(),
                data_type: "TEXT".into(),
                is_nullable: false,
                column_default: None,
            }],
            migration_files: vec![],
        };
        compare_schemas(&meta, &meta); // should not panic
    }

    #[test]
    fn test_compare_schemas_diff() {
        let a = SchemaMetadata {
            version: 1,
            created_at: String::new(),
            table_name: "t".into(),
            schema_name: "s".into(),
            primary_key: vec![],
            columns: vec![ColumnInfo {
                name: "old".into(),
                data_type: "TEXT".into(),
                is_nullable: false,
                column_default: None,
            }],
            migration_files: vec![],
        };
        let b = SchemaMetadata {
            version: 1,
            created_at: String::new(),
            table_name: "t".into(),
            schema_name: "s".into(),
            primary_key: vec![],
            columns: vec![ColumnInfo {
                name: "new".into(),
                data_type: "TEXT".into(),
                is_nullable: false,
                column_default: None,
            }],
            migration_files: vec![],
        };
        compare_schemas(&a, &b);
    }

    #[test]
    fn test_manifest_serde() {
        let mut m = Manifest::default();
        m.updated_at = "2026-04-07".into();
        m.archived_dates.entry("t1".into()).or_default().insert(
            "2026-04-01".into(),
            ArchivedDateInfo {
                row_count: 10,
                archived_at: "now".into(),
                r2_key: "key".into(),
            },
        );
        let json = serde_json::to_string(&m).unwrap();
        let parsed: Manifest = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.archived_dates["t1"]["2026-04-01"].row_count, 10);
    }

    #[tokio::test]
    async fn test_load_manifest_empty() {
        let storage = TestStorage::new();
        let m = load_manifest(&storage).await;
        assert!(m.archived_dates.is_empty());
    }

    #[tokio::test]
    async fn test_load_manifest_existing() {
        let storage = TestStorage::new();
        let m = Manifest {
            updated_at: "now".into(),
            archived_dates: HashMap::new(),
        };
        storage.put(MANIFEST_KEY, serde_json::to_vec(&m).unwrap());
        let loaded = load_manifest(&storage).await;
        assert_eq!(loaded.updated_at, "now");
    }

    #[tokio::test]
    async fn test_dtako_archive_dry_run() {
        let db = MockArchiveDb::default();
        *db.dtako_dates.lock().unwrap() = vec![("t1".into(), "2026-04-01".into(), 100)];
        let storage = TestStorage::new();

        dtako_archive(&db, &storage, true).await.unwrap();
        // Dry run: no files uploaded (except no manifest/schema)
        assert!(storage.keys().is_empty());
    }

    #[tokio::test]
    async fn test_dtako_archive_success() {
        let db = MockArchiveDb::default();
        *db.dtako_dates.lock().unwrap() = vec![("t1".into(), "2026-04-01".into(), 2)];
        *db.columns.lock().unwrap() = vec![("col".into(), "TEXT".into(), "NO".into(), None)];
        *db.rows.lock().unwrap() = vec![
            serde_json::json!({"vehicle_cd": 1, "data_date_time": "2026-04-01 10:00:00"}),
            serde_json::json!({"vehicle_cd": 2, "data_date_time": "2026-04-01 11:00:00"}),
        ];
        let storage = TestStorage::new();

        dtako_archive(&db, &storage, false).await.unwrap();

        // Schema, manifest, and data file uploaded
        assert!(storage.get(SCHEMA_KEY).is_some());
        assert!(storage.get(MANIFEST_KEY).is_some());
        let data_key = "archive/alc_api/dtakologs/t1/2026/04/01.jsonl.gz";
        assert!(storage.get(data_key).is_some());

        // Verify manifest
        let manifest: Manifest =
            serde_json::from_slice(&storage.get(MANIFEST_KEY).unwrap()).unwrap();
        assert_eq!(manifest.archived_dates["t1"]["2026-04-01"].row_count, 2);
    }

    #[tokio::test]
    async fn test_dtako_archive_skip_already_archived() {
        let db = MockArchiveDb::default();
        *db.dtako_dates.lock().unwrap() = vec![("t1".into(), "2026-04-01".into(), 100)];
        *db.columns.lock().unwrap() = vec![("col".into(), "TEXT".into(), "NO".into(), None)];

        // Pre-populate manifest
        let mut manifest = Manifest::default();
        manifest
            .archived_dates
            .entry("t1".into())
            .or_default()
            .insert(
                "2026-04-01".into(),
                ArchivedDateInfo {
                    row_count: 100,
                    archived_at: "prev".into(),
                    r2_key: "old_key".into(),
                },
            );
        let storage = TestStorage::new();
        storage.put(MANIFEST_KEY, serde_json::to_vec(&manifest).unwrap());

        dtako_archive(&db, &storage, false).await.unwrap();
        // No data file created (already archived)
        let data_keys: Vec<_> = storage
            .keys()
            .into_iter()
            .filter(|k| k.ends_with(".jsonl.gz"))
            .collect();
        assert!(data_keys.is_empty());
    }

    #[tokio::test]
    async fn test_dtako_archive_phase_b_delete() {
        let db = MockArchiveDb::default();
        // No new dates to archive
        *db.dtako_dates.lock().unwrap() = vec![];
        // Old dates to delete
        *db.old_dates.lock().unwrap() = vec![("t1".into(), "2026-03-01".into(), 50)];
        *db.deleted_count.lock().unwrap() = 50;
        *db.columns.lock().unwrap() = vec![("col".into(), "TEXT".into(), "NO".into(), None)];

        // Manifest has the old date
        let mut manifest = Manifest::default();
        manifest
            .archived_dates
            .entry("t1".into())
            .or_default()
            .insert(
                "2026-03-01".into(),
                ArchivedDateInfo {
                    row_count: 50,
                    archived_at: "prev".into(),
                    r2_key: "key".into(),
                },
            );
        let storage = TestStorage::new();
        storage.put(MANIFEST_KEY, serde_json::to_vec(&manifest).unwrap());

        dtako_archive(&db, &storage, false).await.unwrap();
    }

    #[tokio::test]
    async fn test_dtako_archive_phase_b_skip_not_in_manifest() {
        let db = MockArchiveDb::default();
        *db.dtako_dates.lock().unwrap() = vec![];
        *db.old_dates.lock().unwrap() = vec![("t1".into(), "2026-03-01".into(), 50)];
        *db.columns.lock().unwrap() = vec![("col".into(), "TEXT".into(), "NO".into(), None)];
        // No manifest → should skip delete
        let storage = TestStorage::new();

        dtako_archive(&db, &storage, false).await.unwrap();
    }

    #[tokio::test]
    async fn test_dtako_restore_success() {
        let db = MockArchiveDb::default();
        *db.columns.lock().unwrap() = vec![("col".into(), "TEXT".into(), "NO".into(), None)];

        let storage = TestStorage::new();
        // Create archived data
        let header = serde_json::json!({"_archive_header": true, "schema_version": 1});
        let row = serde_json::json!({"vehicle_cd": 1, "data_date_time": "2026-04-01 10:00"});
        let mut content = serde_json::to_string(&header).unwrap();
        content.push('\n');
        content.push_str(&serde_json::to_string(&row).unwrap());
        content.push('\n');
        let compressed = gzip_compress(content.as_bytes()).unwrap();
        let key = make_r2_key("t1", "2026-04-01");
        storage.put(&key, compressed);

        dtako_restore(&db, &storage, "t1", "2026-04-01")
            .await
            .unwrap();

        let upserted = db.upserted.lock().unwrap();
        assert_eq!(upserted.len(), 1);
    }

    #[tokio::test]
    async fn test_dtako_restore_no_schema_warning() {
        let db = MockArchiveDb::default();
        *db.columns.lock().unwrap() = vec![("col".into(), "TEXT".into(), "NO".into(), None)];

        let storage = TestStorage::new();
        let content = "{\"_archive_header\":true}\n{\"vehicle_cd\":1}\n";
        let compressed = gzip_compress(content.as_bytes()).unwrap();
        storage.put(&make_r2_key("t1", "2026-04-01"), compressed);

        // No SCHEMA_KEY in storage → prints warning but succeeds
        dtako_restore(&db, &storage, "t1", "2026-04-01")
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_dtako_restore_with_schema_comparison() {
        let db = MockArchiveDb::default();
        *db.columns.lock().unwrap() = vec![("col".into(), "TEXT".into(), "NO".into(), None)];

        let storage = TestStorage::new();
        // Upload schema metadata
        let meta = SchemaMetadata {
            version: 1,
            created_at: "now".into(),
            table_name: "dtakologs".into(),
            schema_name: "alc_api".into(),
            primary_key: vec![],
            columns: vec![ColumnInfo {
                name: "col".into(),
                data_type: "TEXT".into(),
                is_nullable: false,
                column_default: None,
            }],
            migration_files: vec![],
        };
        storage.put(SCHEMA_KEY, serde_json::to_vec(&meta).unwrap());

        let content = "{\"_archive_header\":true}\n{\"vehicle_cd\":1}\n";
        let compressed = gzip_compress(content.as_bytes()).unwrap();
        storage.put(&make_r2_key("t1", "2026-04-01"), compressed);

        dtako_restore(&db, &storage, "t1", "2026-04-01")
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_dtako_archive_db_error() {
        use std::sync::atomic::Ordering;
        let db = MockArchiveDb::default();
        db.fail_next.store(true, Ordering::SeqCst);
        let storage = TestStorage::new();

        // fetch_schema_metadata will fail (dry_run=false triggers it)
        let result = dtako_archive(&db, &storage, false).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_fetch_from_r2_empty() {
        let storage = TestStorage::new();
        let rows = fetch_from_r2(&storage, "t1", "2026-04-01", "2026-04-07", None)
            .await
            .unwrap();
        assert!(rows.is_empty());
    }

    #[tokio::test]
    async fn test_fetch_from_r2_with_data() {
        let storage = TestStorage::new();
        let r2_key = "archive/alc_api/dtakologs/t1/2026/04/01.jsonl.gz";
        let mut manifest = Manifest::default();
        manifest
            .archived_dates
            .entry("t1".into())
            .or_default()
            .insert(
                "2026-04-01".into(),
                ArchivedDateInfo {
                    row_count: 1,
                    archived_at: "now".into(),
                    r2_key: r2_key.into(),
                },
            );
        storage.put(MANIFEST_KEY, serde_json::to_vec(&manifest).unwrap());

        let content = "{\"_archive_header\":true}\n{\"vehicle_cd\":1,\"data_date_time\":\"2026-04-01 10:00\",\"vehicle_name\":\"t\",\"speed\":50.0,\"gps_direction\":0,\"gps_latitude\":35,\"gps_longitude\":139,\"sub_driver_cd\":0}\n";
        storage.put(r2_key, gzip_compress(content.as_bytes()).unwrap());

        let rows = fetch_from_r2(&storage, "t1", "2026-04-01", "2026-04-01", None)
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].vehicle_cd, 1);
    }

    #[tokio::test]
    async fn test_fetch_from_r2_vehicle_filter() {
        let storage = TestStorage::new();
        let r2_key = "k.jsonl.gz";
        let mut manifest = Manifest::default();
        manifest
            .archived_dates
            .entry("t1".into())
            .or_default()
            .insert(
                "2026-04-01".into(),
                ArchivedDateInfo {
                    row_count: 2,
                    archived_at: "now".into(),
                    r2_key: r2_key.into(),
                },
            );
        storage.put(MANIFEST_KEY, serde_json::to_vec(&manifest).unwrap());

        let content = "{\"_archive_header\":true}\n{\"vehicle_cd\":1,\"data_date_time\":\"a\",\"vehicle_name\":\"\",\"speed\":0,\"gps_direction\":0,\"gps_latitude\":0,\"gps_longitude\":0,\"sub_driver_cd\":0}\n{\"vehicle_cd\":2,\"data_date_time\":\"b\",\"vehicle_name\":\"\",\"speed\":0,\"gps_direction\":0,\"gps_latitude\":0,\"gps_longitude\":0,\"sub_driver_cd\":0}\n";
        storage.put(r2_key, gzip_compress(content.as_bytes()).unwrap());

        let rows = fetch_from_r2(&storage, "t1", "2026-04-01", "2026-04-01", Some(2))
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].data_date_time, "b");
    }

    #[tokio::test]
    async fn test_fetch_from_r2_download_fail() {
        let storage = TestStorage::new();
        let mut manifest = Manifest::default();
        manifest
            .archived_dates
            .entry("t1".into())
            .or_default()
            .insert(
                "2026-04-01".into(),
                ArchivedDateInfo {
                    row_count: 1,
                    archived_at: "now".into(),
                    r2_key: "missing.jsonl.gz".into(),
                },
            );
        storage.put(MANIFEST_KEY, serde_json::to_vec(&manifest).unwrap());

        let rows = fetch_from_r2(&storage, "t1", "2026-04-01", "2026-04-01", None)
            .await
            .unwrap();
        assert!(rows.is_empty()); // graceful degradation
    }

    #[tokio::test]
    async fn test_fetch_from_r2_date_range_filter() {
        let storage = TestStorage::new();
        let mut manifest = Manifest::default();
        manifest
            .archived_dates
            .entry("t1".into())
            .or_default()
            .insert(
                "2026-03-15".into(),
                ArchivedDateInfo {
                    row_count: 1,
                    archived_at: "now".into(),
                    r2_key: "k.jsonl.gz".into(),
                },
            );
        storage.put(MANIFEST_KEY, serde_json::to_vec(&manifest).unwrap());

        let content = "{\"_archive_header\":true}\n{\"vehicle_cd\":1,\"data_date_time\":\"2026-03-15\",\"vehicle_name\":\"\",\"speed\":0,\"gps_direction\":0,\"gps_latitude\":0,\"gps_longitude\":0,\"sub_driver_cd\":0}\n";
        storage.put("k.jsonl.gz", gzip_compress(content.as_bytes()).unwrap());

        // Out of range
        let rows = fetch_from_r2(&storage, "t1", "2026-04-01", "2026-04-30", None)
            .await
            .unwrap();
        assert!(rows.is_empty());

        // In range
        let rows = fetch_from_r2(&storage, "t1", "2026-03-01", "2026-03-31", None)
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
    }

    #[tokio::test]
    async fn test_dtako_archive_dry_run_with_old_dates() {
        // Covers: dry_run DELETE path (lines 187-192)
        let db = MockArchiveDb::default();
        *db.dtako_dates.lock().unwrap() = vec![("t1".into(), "2026-04-01".into(), 10)];
        *db.old_dates.lock().unwrap() = vec![("t1".into(), "2026-03-01".into(), 50)];

        let mut manifest = Manifest::default();
        manifest
            .archived_dates
            .entry("t1".into())
            .or_default()
            .insert(
                "2026-03-01".into(),
                ArchivedDateInfo {
                    row_count: 50,
                    archived_at: "prev".into(),
                    r2_key: "key".into(),
                },
            );
        let storage = TestStorage::new();
        storage.put(MANIFEST_KEY, serde_json::to_vec(&manifest).unwrap());

        dtako_archive(&db, &storage, true).await.unwrap();
    }

    #[tokio::test]
    async fn test_dtako_restore_empty_lines_and_batch() {
        // Covers: empty lines skip + batch < 500 flush
        let db = MockArchiveDb::default();
        *db.columns.lock().unwrap() = vec![("col".into(), "TEXT".into(), "NO".into(), None)];

        let storage = TestStorage::new();
        // Content with empty lines between data
        let mut content = String::from("{\"_archive_header\":true}\n\n");
        content.push_str("{\"vehicle_cd\":1}\n\n{\"vehicle_cd\":2}\n");
        let compressed = gzip_compress(content.as_bytes()).unwrap();
        storage.put(&make_r2_key("t1", "2026-04-01"), compressed);

        dtako_restore(&db, &storage, "t1", "2026-04-01")
            .await
            .unwrap();
        assert_eq!(db.upserted.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn test_fetch_from_r2_invalid_json_skipped() {
        // Covers: serde parse error continue (line 297, 301)
        let storage = TestStorage::new();
        let r2_key = "k.jsonl.gz";
        let mut manifest = Manifest::default();
        manifest
            .archived_dates
            .entry("t1".into())
            .or_default()
            .insert(
                "2026-04-01".into(),
                ArchivedDateInfo {
                    row_count: 1,
                    archived_at: "now".into(),
                    r2_key: r2_key.into(),
                },
            );
        storage.put(MANIFEST_KEY, serde_json::to_vec(&manifest).unwrap());

        // Mix of valid JSON, invalid JSON, and empty lines
        let content = "{\"_archive_header\":true}\nnot-valid-json\n{\"vehicle_cd\":1,\"data_date_time\":\"d\",\"vehicle_name\":\"\",\"speed\":0,\"gps_direction\":0,\"gps_latitude\":0,\"gps_longitude\":0,\"sub_driver_cd\":0}\n";
        storage.put(r2_key, gzip_compress(content.as_bytes()).unwrap());

        let rows = fetch_from_r2(&storage, "t1", "2026-04-01", "2026-04-01", None)
            .await
            .unwrap();
        assert_eq!(rows.len(), 1); // invalid JSON skipped, valid row parsed
    }

    #[tokio::test]
    async fn test_dtako_restore_large_batch_flush() {
        // Covers: batch_values.len() >= 500 flush path (lines 253-254)
        // and batch_values empty after exact multiple (line 260 false branch)
        let db = MockArchiveDb::default();
        *db.columns.lock().unwrap() = vec![("col".into(), "TEXT".into(), "NO".into(), None)];

        let storage = TestStorage::new();
        let mut content = String::from("{\"_archive_header\":true}\n");
        // 500 rows exactly → triggers batch flush, then empty remainder
        for i in 0..500 {
            content.push_str(&format!("{{\"vehicle_cd\":{}}}\n", i));
        }
        let compressed = gzip_compress(content.as_bytes()).unwrap();
        storage.put(&make_r2_key("t1", "2026-04-01"), compressed);

        dtako_restore(&db, &storage, "t1", "2026-04-01")
            .await
            .unwrap();
        assert_eq!(db.upserted.lock().unwrap().len(), 500);
    }

    #[tokio::test]
    async fn test_fetch_from_r2_empty_lines_in_archive() {
        // Covers: empty line continue in fetch_from_r2 (line 297)
        let storage = TestStorage::new();
        let r2_key = "k.jsonl.gz";
        let mut manifest = Manifest::default();
        manifest
            .archived_dates
            .entry("t1".into())
            .or_default()
            .insert(
                "2026-04-01".into(),
                ArchivedDateInfo {
                    row_count: 1,
                    archived_at: "now".into(),
                    r2_key: r2_key.into(),
                },
            );
        storage.put(MANIFEST_KEY, serde_json::to_vec(&manifest).unwrap());

        // Archive data with empty lines between rows
        let content = "{\"_archive_header\":true}\n\n{\"vehicle_cd\":1,\"data_date_time\":\"d\",\"vehicle_name\":\"\",\"speed\":0,\"gps_direction\":0,\"gps_latitude\":0,\"gps_longitude\":0,\"sub_driver_cd\":0}\n\n";
        storage.put(r2_key, gzip_compress(content.as_bytes()).unwrap());

        let rows = fetch_from_r2(&storage, "t1", "2026-04-01", "2026-04-01", None)
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
    }

    #[tokio::test]
    async fn test_dtako_archive_range_filters() {
        let db = MockArchiveDb::default();
        *db.dtako_dates.lock().unwrap() = vec![
            ("t1".into(), "2026-03-01".into(), 10),
            ("t1".into(), "2026-04-01".into(), 20),
            ("t1".into(), "2026-05-01".into(), 30),
        ];
        *db.columns.lock().unwrap() = vec![("col".into(), "TEXT".into(), "NO".into(), None)];
        *db.rows.lock().unwrap() =
            vec![serde_json::json!({"vehicle_cd": 1, "data_date_time": "2026-04-01 10:00:00"})];
        let storage = TestStorage::new();

        // Only archive April
        dtako_archive_range(&db, &storage, true, Some("2026-04-01"), Some("2026-04-01"))
            .await
            .unwrap();
        // dry-run: nothing uploaded, but March and May should be skipped
        assert!(storage.keys().is_empty());
    }

    #[tokio::test]
    async fn test_dtako_archive_range_start_only() {
        let db = MockArchiveDb::default();
        *db.dtako_dates.lock().unwrap() = vec![
            ("t1".into(), "2026-03-01".into(), 10),
            ("t1".into(), "2026-04-01".into(), 20),
        ];
        *db.columns.lock().unwrap() = vec![("col".into(), "TEXT".into(), "NO".into(), None)];
        *db.rows.lock().unwrap() =
            vec![serde_json::json!({"vehicle_cd": 1, "data_date_time": "2026-04-01 10:00:00"})];
        let storage = TestStorage::new();

        // start_date filters out March, archives April
        dtako_archive_range(&db, &storage, false, Some("2026-04-01"), None)
            .await
            .unwrap();
        // April archived (schema + data + manifest)
        let data_keys: Vec<_> = storage
            .keys()
            .into_iter()
            .filter(|k| k.ends_with(".jsonl.gz"))
            .collect();
        assert_eq!(data_keys.len(), 1);
    }

    #[tokio::test]
    async fn test_dtako_archive_range_end_only() {
        let db = MockArchiveDb::default();
        *db.dtako_dates.lock().unwrap() = vec![
            ("t1".into(), "2026-03-01".into(), 10),
            ("t1".into(), "2026-04-01".into(), 20),
        ];
        *db.columns.lock().unwrap() = vec![("col".into(), "TEXT".into(), "NO".into(), None)];
        *db.rows.lock().unwrap() =
            vec![serde_json::json!({"vehicle_cd": 1, "data_date_time": "2026-03-01 10:00:00"})];
        let storage = TestStorage::new();

        // end_date filters out April, archives March
        dtako_archive_range(&db, &storage, false, None, Some("2026-03-01"))
            .await
            .unwrap();
        let data_keys: Vec<_> = storage
            .keys()
            .into_iter()
            .filter(|k| k.ends_with(".jsonl.gz"))
            .collect();
        assert_eq!(data_keys.len(), 1);
        assert!(data_keys[0].contains("2026/03/01"));
    }

    #[tokio::test]
    async fn test_dtako_archive_range_phase_b_filters() {
        // Phase B: delete old dates should also respect date range
        let db = MockArchiveDb::default();
        *db.dtako_dates.lock().unwrap() = vec![];
        *db.old_dates.lock().unwrap() = vec![
            ("t1".into(), "2026-02-01".into(), 50),
            ("t1".into(), "2026-03-01".into(), 50),
            ("t1".into(), "2026-04-01".into(), 50),
        ];
        *db.deleted_count.lock().unwrap() = 50;
        *db.columns.lock().unwrap() = vec![("col".into(), "TEXT".into(), "NO".into(), None)];

        let mut manifest = Manifest::default();
        manifest
            .archived_dates
            .entry("t1".into())
            .or_default()
            .insert(
                "2026-02-01".into(),
                ArchivedDateInfo {
                    row_count: 50,
                    archived_at: "prev".into(),
                    r2_key: "k".into(),
                },
            );
        manifest
            .archived_dates
            .entry("t1".into())
            .or_default()
            .insert(
                "2026-03-01".into(),
                ArchivedDateInfo {
                    row_count: 50,
                    archived_at: "prev".into(),
                    r2_key: "k".into(),
                },
            );
        let storage = TestStorage::new();
        storage.put(MANIFEST_KEY, serde_json::to_vec(&manifest).unwrap());

        // Only delete March, skip February
        dtako_archive_range(&db, &storage, false, Some("2026-03-01"), Some("2026-03-01"))
            .await
            .unwrap();
    }
}
