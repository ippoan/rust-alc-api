use crate::archive::compress::{download_decompressed, upload_compressed, upload_json};
use crate::archive::schema_meta::{fetch_schema_metadata, SchemaMetadata};
use alc_core::storage::StorageBackend;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::collections::HashMap;

const BATCH_SIZE: i64 = 10_000;

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Manifest {
    pub updated_at: String,
    /// archived_dates[tenant_id][date_str] = ArchivedDateInfo
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

async fn load_manifest(storage: &dyn StorageBackend) -> Manifest {
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

/// Phase A: Copy unarchived dates to R2
/// Phase B: DELETE rows older than 7 days (only if already in manifest)
pub async fn dtako_archive(
    pool: &PgPool,
    storage: &dyn StorageBackend,
    dry_run: bool,
) -> anyhow::Result<()> {
    let mut manifest = load_manifest(storage).await;

    // Upload schema metadata
    if !dry_run {
        let meta = fetch_schema_metadata(
            pool,
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

    // Phase A: find dates not yet in manifest
    // Use superuser connection (no RLS) for cross-tenant access
    let dates_to_archive = sqlx::query_as::<_, (String, String, i64)>(
        "SELECT tenant_id::TEXT, data_date_time::DATE::TEXT AS date_str, COUNT(*)
         FROM alc_api.dtakologs
         GROUP BY tenant_id, data_date_time::DATE
         ORDER BY date_str",
    )
    .fetch_all(pool)
    .await?;

    println!("=== Phase A: Archive to R2 ===");

    let mut archived_count = 0usize;
    for (tenant_id, date_str, count) in &dates_to_archive {
        let already_archived = manifest
            .archived_dates
            .get(tenant_id)
            .and_then(|dates| dates.get(date_str))
            .is_some();

        if already_archived {
            continue;
        }

        let r2_key = format!(
            "archive/alc_api/dtakologs/{}/{}/{}/{}.jsonl.gz",
            tenant_id,
            &date_str[..4],   // year
            &date_str[5..7],  // month
            &date_str[8..10]  // day
        );

        if dry_run {
            println!(
                "  [dry-run] Would archive {} rows for tenant={} date={} → {}",
                count, tenant_id, date_str, r2_key
            );
            archived_count += *count as usize;
            continue;
        }

        // Fetch all rows for this tenant+date
        let mut buffer = Vec::new();
        let header = serde_json::json!({
            "_archive_header": true,
            "schema_version": 1,
            "table": "alc_api.dtakologs",
            "tenant_id": tenant_id,
            "date": date_str,
            "archived_at": chrono::Utc::now().to_rfc3339(),
        });
        buffer.extend_from_slice(serde_json::to_string(&header)?.as_bytes());
        buffer.push(b'\n');

        let mut offset: i64 = 0;
        let mut row_count = 0usize;
        loop {
            let rows: Vec<(serde_json::Value,)> = sqlx::query_as(
                "SELECT row_to_json(d) FROM alc_api.dtakologs d
                 WHERE tenant_id = $1::UUID AND data_date_time::DATE = $2::DATE
                 ORDER BY data_date_time
                 LIMIT $3 OFFSET $4",
            )
            .bind(tenant_id)
            .bind(date_str)
            .bind(BATCH_SIZE)
            .bind(offset)
            .fetch_all(pool)
            .await?;

            if rows.is_empty() {
                break;
            }

            for (row,) in &rows {
                buffer.extend_from_slice(serde_json::to_string(row)?.as_bytes());
                buffer.push(b'\n');
                row_count += 1;
            }

            offset += rows.len() as i64;
        }

        // Upload to R2
        upload_compressed(storage, &r2_key, &buffer).await?;

        // Update manifest
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
    }

    if !dry_run && archived_count > 0 {
        manifest.updated_at = chrono::Utc::now().to_rfc3339();
        save_manifest(storage, &manifest).await?;
    }
    println!("Phase A: {} rows archived", archived_count);

    // Phase B: DELETE rows older than 7 days (only if in manifest)
    println!("\n=== Phase B: DELETE old rows from DB ===");

    let cutoff = (chrono::Utc::now() - chrono::Duration::days(7))
        .format("%Y-%m-%d")
        .to_string();

    // Find dates older than cutoff that are in the manifest
    let old_dates = sqlx::query_as::<_, (String, String, i64)>(
        "SELECT tenant_id::TEXT, data_date_time::DATE::TEXT AS date_str, COUNT(*)
         FROM alc_api.dtakologs
         WHERE data_date_time::DATE < $1::DATE
         GROUP BY tenant_id, data_date_time::DATE
         ORDER BY date_str",
    )
    .bind(&cutoff)
    .fetch_all(pool)
    .await?;

    let mut deleted_count = 0i64;
    for (tenant_id, date_str, count) in &old_dates {
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

        let result = sqlx::query(
            "DELETE FROM alc_api.dtakologs
             WHERE tenant_id = $1::UUID AND data_date_time::DATE = $2::DATE",
        )
        .bind(tenant_id)
        .bind(date_str)
        .execute(pool)
        .await?;

        println!(
            "  DELETE {} rows for tenant={} date={}",
            result.rows_affected(),
            tenant_id,
            date_str
        );
        deleted_count += result.rows_affected() as i64;
    }

    println!(
        "Phase B: {} rows deleted (cutoff={})",
        deleted_count, cutoff
    );

    Ok(())
}

/// Restore archived data from R2 back to DB
pub async fn dtako_restore(
    pool: &PgPool,
    storage: &dyn StorageBackend,
    tenant_id: &str,
    date: &str,
) -> anyhow::Result<()> {
    // Load and compare schema
    let current_meta = fetch_schema_metadata(pool, "alc_api", "dtakologs", 1, vec![]).await?;

    match storage.download(SCHEMA_KEY).await {
        Ok(data) => {
            let archived_meta: SchemaMetadata = serde_json::from_slice(&data)?;
            compare_schemas(&archived_meta, &current_meta);
        }
        Err(_) => {
            println!("WARNING: No archived schema metadata found, proceeding anyway");
        }
    }

    // Parse date to build R2 key
    let r2_key = format!(
        "archive/alc_api/dtakologs/{}/{}/{}/{}.jsonl.gz",
        tenant_id,
        &date[..4],
        &date[5..7],
        &date[8..10],
    );

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

        // Skip header lines
        if value.get("_archive_header").is_some() {
            continue;
        }

        batch_values.push(line.to_string());
        restored += 1;

        if batch_values.len() >= 500 {
            upsert_batch(pool, &batch_values).await?;
            batch_values.clear();
        }
    }

    if !batch_values.is_empty() {
        upsert_batch(pool, &batch_values).await?;
    }

    println!("Restored {} rows from {}", restored, r2_key);
    Ok(())
}

async fn upsert_batch(pool: &PgPool, rows: &[String]) -> anyhow::Result<()> {
    for row_json in rows {
        let v: serde_json::Value = serde_json::from_str(row_json)?;

        // Extract fields for INSERT ... ON CONFLICT DO UPDATE
        sqlx::query(
            "INSERT INTO alc_api.dtakologs (
                tenant_id, data_date_time, vehicle_cd,
                type, all_state_font_color_index, all_state_ryout_color,
                branch_cd, branch_name, current_work_cd, data_filter_type,
                disp_flag, driver_cd, gps_direction, gps_enable,
                gps_latitude, gps_longitude, gps_satellite_num,
                operation_state, recive_event_type, recive_packet_type,
                recive_work_cd, revo, setting_temp, setting_temp1,
                setting_temp3, setting_temp4, speed, sub_driver_cd,
                temp_state, vehicle_name,
                address_disp_c, address_disp_p, all_state, all_state_ex,
                all_state_font_color, comu_date_time, current_work_name,
                driver_name, event_val, gps_lati_and_long, odometer,
                recive_type_color_name, recive_type_name,
                start_work_date_time, state, state1, state2, state3,
                state_flag, temp1, temp2, temp3, temp4,
                vehicle_icon_color, vehicle_icon_label_for_datetime,
                vehicle_icon_label_for_driver, vehicle_icon_label_for_vehicle
            )
            SELECT
                (j->>'tenant_id')::UUID,
                j->>'data_date_time',
                (j->>'vehicle_cd')::INTEGER,
                COALESCE(j->>'type', ''),
                COALESCE((j->>'all_state_font_color_index')::INTEGER, 0),
                COALESCE(j->>'all_state_ryout_color', 'Transparent'),
                COALESCE((j->>'branch_cd')::INTEGER, 0),
                COALESCE(j->>'branch_name', ''),
                COALESCE((j->>'current_work_cd')::INTEGER, 0),
                COALESCE((j->>'data_filter_type')::INTEGER, 0),
                COALESCE((j->>'disp_flag')::INTEGER, 0),
                COALESCE((j->>'driver_cd')::INTEGER, 0),
                COALESCE((j->>'gps_direction')::DOUBLE PRECISION, 0),
                COALESCE((j->>'gps_enable')::INTEGER, 0),
                COALESCE((j->>'gps_latitude')::DOUBLE PRECISION, 0),
                COALESCE((j->>'gps_longitude')::DOUBLE PRECISION, 0),
                COALESCE((j->>'gps_satellite_num')::INTEGER, 0),
                COALESCE((j->>'operation_state')::INTEGER, 0),
                COALESCE((j->>'recive_event_type')::INTEGER, 0),
                COALESCE((j->>'recive_packet_type')::INTEGER, 0),
                COALESCE((j->>'recive_work_cd')::INTEGER, 0),
                COALESCE((j->>'revo')::INTEGER, 0),
                COALESCE(j->>'setting_temp', ''),
                COALESCE(j->>'setting_temp1', ''),
                COALESCE(j->>'setting_temp3', ''),
                COALESCE(j->>'setting_temp4', ''),
                COALESCE((j->>'speed')::REAL, 0),
                COALESCE((j->>'sub_driver_cd')::INTEGER, 0),
                COALESCE((j->>'temp_state')::INTEGER, 0),
                COALESCE(j->>'vehicle_name', ''),
                j->>'address_disp_c', j->>'address_disp_p',
                j->>'all_state', j->>'all_state_ex',
                j->>'all_state_font_color', j->>'comu_date_time',
                j->>'current_work_name', j->>'driver_name',
                j->>'event_val', j->>'gps_lati_and_long', j->>'odometer',
                j->>'recive_type_color_name', j->>'recive_type_name',
                j->>'start_work_date_time', j->>'state', j->>'state1',
                j->>'state2', j->>'state3', j->>'state_flag',
                j->>'temp1', j->>'temp2', j->>'temp3', j->>'temp4',
                j->>'vehicle_icon_color',
                j->>'vehicle_icon_label_for_datetime',
                j->>'vehicle_icon_label_for_driver',
                j->>'vehicle_icon_label_for_vehicle'
            FROM jsonb_array_elements($1::JSONB) AS j
            ON CONFLICT (tenant_id, data_date_time, vehicle_cd) DO UPDATE SET
                type = EXCLUDED.type,
                speed = EXCLUDED.speed,
                gps_latitude = EXCLUDED.gps_latitude,
                gps_longitude = EXCLUDED.gps_longitude,
                gps_direction = EXCLUDED.gps_direction",
        )
        .bind(format!("[{}]", v))
        .execute(pool)
        .await?;
    }

    Ok(())
}

/// R2 から指定 tenant_id + 日付範囲の dtakologs を読み込み、DtakologRow に変換して返す。
/// フロントエンドの by-date-range エンドポイントから呼ばれる。
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
        // Filter by date range
        if date_str.as_str() < start_date || date_str.as_str() > end_date {
            continue;
        }

        // Download and decompress
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
            // Skip header lines
            if value.get("_archive_header").is_some() {
                continue;
            }

            // Filter by vehicle_cd if specified
            if let Some(vc) = vehicle_cd {
                if value.get("vehicle_cd").and_then(|v| v.as_i64()) != Some(vc as i64) {
                    continue;
                }
            }

            let row = json_to_dtakolog_row(&value);
            all_rows.push(row);
        }
    }

    // Sort by data_date_time
    all_rows.sort_by(|a, b| a.data_date_time.cmp(&b.data_date_time));

    Ok(all_rows)
}

fn json_to_dtakolog_row(v: &serde_json::Value) -> alc_core::models::DtakologRow {
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

fn compare_schemas(archived: &SchemaMetadata, current: &SchemaMetadata) {
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
