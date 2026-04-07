//! R2 アーカイブから dtakologs を読み取るモジュール。
//! by-date-range エンドポイントで DB にないデータを R2 からフォールバック取得する。

use alc_core::models::DtakologRow;
use alc_core::storage::StorageBackend;
use flate2::read::GzDecoder;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Read;

const MANIFEST_KEY: &str = "archive/alc_api/dtakologs/_manifest.json";

#[derive(Debug, Serialize, Deserialize, Default)]
struct Manifest {
    #[serde(default)]
    archived_dates: HashMap<String, HashMap<String, ArchivedDateInfo>>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ArchivedDateInfo {
    #[serde(default)]
    row_count: usize,
    #[serde(default)]
    r2_key: String,
    #[serde(default)]
    archived_at: String,
}

/// R2 から指定 tenant_id + 日付範囲の dtakologs を読み込み返す。
/// DB の重複期間のデータも含まれうるので、呼び出し側で dedup する。
pub async fn fetch_from_r2(
    storage: &dyn StorageBackend,
    tenant_id: &str,
    start_date: &str,
    end_date: &str,
    vehicle_cd: Option<i32>,
) -> anyhow::Result<Vec<DtakologRow>> {
    // Load manifest
    let manifest: Manifest = match storage.download(MANIFEST_KEY).await {
        Ok(data) => serde_json::from_slice(&data).unwrap_or_default(),
        Err(_) => return Ok(vec![]),
    };

    let tenant_dates = match manifest.archived_dates.get(tenant_id) {
        Some(dates) => dates,
        None => return Ok(vec![]),
    };

    let mut all_rows = Vec::new();

    for (date_str, info) in tenant_dates {
        if date_str.as_str() < start_date || date_str.as_str() > end_date {
            continue;
        }

        // Download and decompress
        let compressed = match storage.download(&info.r2_key).await {
            Ok(d) => d,
            Err(e) => {
                tracing::warn!("Failed to download archive {}: {}", info.r2_key, e);
                continue;
            }
        };

        let mut decoder = GzDecoder::new(compressed.as_slice());
        let mut content = String::new();
        if let Err(e) = decoder.read_to_string(&mut content) {
            tracing::warn!("Failed to decompress {}: {}", info.r2_key, e);
            continue;
        }

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

            // Filter by vehicle_cd if specified
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

fn json_to_dtakolog_row(v: &serde_json::Value) -> DtakologRow {
    DtakologRow {
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
