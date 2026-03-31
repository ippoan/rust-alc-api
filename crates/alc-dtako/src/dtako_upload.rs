use axum::{
    body::Body,
    extract::{Multipart, Path, Query, State},
    http::StatusCode,
    response::Response,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use alc_core::auth_middleware::TenantId;
use alc_core::repository::dtako_upload::{
    InsertDailyWorkHoursParams, InsertOperationParams, InsertSegmentParams,
};
use alc_core::AppState;
use alc_csv_parser;
use alc_csv_parser::kudgivt::{parse_kudgivt, KudgivtRow};
use alc_csv_parser::kudguri::{parse_kudguri, KudguriRow};
use alc_csv_parser::work_segments::EventClass;
use tokio_stream::StreamExt;

pub fn tenant_router() -> Router<AppState> {
    Router::new()
        .route("/upload", post(upload_zip))
        .route("/recalculate", post(internal_recalculate_all))
        .route("/recalculate-driver", post(recalculate_driver))
        .route("/recalculate-drivers", post(recalculate_drivers_batch))
        .route("/split-csv/{upload_id}", post(split_csv_handler))
        .route("/split-csv-all", post(split_csv_all_handler))
        .route("/uploads", get(list_uploads))
        .route("/internal/pending", get(list_pending_uploads))
        .route("/internal/rerun/{upload_id}", post(internal_rerun))
        .route("/internal/download/{upload_id}", get(internal_download))
}

#[derive(Debug, Serialize)]
pub struct UploadResponse {
    pub upload_id: Uuid,
    pub operations_count: i32,
    pub status: String,
}

async fn upload_zip(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    mut multipart: Multipart,
) -> Result<Json<UploadResponse>, (StatusCode, String)> {
    let tenant_id = tenant.0 .0;

    // Extract ZIP file from multipart
    let (filename, zip_bytes) = extract_file(&mut multipart).await?;

    // Create upload history record
    let upload_id = Uuid::new_v4();
    state
        .dtako_upload
        .create_upload_history(tenant_id, upload_id, &filename)
        .await
        .map_err(internal_err)?;

    // Process ZIP
    match process_zip(&state, tenant_id, upload_id, &filename, &zip_bytes).await {
        Ok(count) => {
            // Mark success
            state
                .dtako_upload
                .update_upload_completed(tenant_id, upload_id, count)
                .await
                .map_err(internal_err)?;

            // CSV split (non-blocking)
            try_split_csv(&state, upload_id).await;

            Ok(Json(UploadResponse {
                upload_id,
                operations_count: count,
                status: "completed".to_string(),
            }))
        }
        Err(e) => {
            let _ = state
                .dtako_upload
                .mark_upload_failed(upload_id, &e.to_string())
                .await;
            Err((StatusCode::BAD_REQUEST, e.to_string()))
        }
    }
}

async fn extract_file(
    multipart: &mut Multipart,
) -> Result<(String, Vec<u8>), (StatusCode, String)> {
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("multipart error: {e}")))?
    {
        let name = field.name().unwrap_or("").to_string();
        if name == "file" {
            let filename = field.file_name().unwrap_or("upload.zip").to_string();
            let bytes = field
                .bytes()
                .await
                .map_err(|e| (StatusCode::BAD_REQUEST, format!("read error: {e}")))?;
            return Ok((filename, bytes.to_vec()));
        }
    }
    Err((StatusCode::BAD_REQUEST, "no 'file' field found".to_string()))
}

async fn process_zip(
    state: &AppState,
    tenant_id: Uuid,
    upload_id: Uuid,
    filename: &str,
    zip_bytes: &[u8],
) -> Result<i32, anyhow::Error> {
    // 1. Save original ZIP to R2 (dtako bucket)
    let zip_key = format!("{}/uploads/{}/{}", tenant_id, upload_id, filename);
    let dtako_st = state
        .dtako_storage
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("DTAKO_R2_BUCKET not configured"))?;
    dtako_st
        .upload(&zip_key, zip_bytes, "application/zip")
        .await
        .map_err(|e| anyhow::anyhow!("R2 upload failed: {e}"))?;

    // Update upload_history with R2 key
    state
        .dtako_upload
        .update_upload_r2_key(tenant_id, upload_id, &zip_key)
        .await?;

    // 2. Extract ZIP
    let files = alc_csv_parser::extract_zip(zip_bytes)?;

    // 3. Find and parse KUDGURI.csv
    let kudguri_file = files
        .iter()
        .find(|(name, _)| name.to_uppercase().contains("KUDGURI"))
        .ok_or_else(|| anyhow::anyhow!("KUDGURI.csv not found in ZIP"))?;

    let csv_text = alc_csv_parser::decode_shift_jis(&kudguri_file.1);
    let rows = parse_kudguri(&csv_text)?;
    tracing::info!("KUDGURI parsed: {} rows (tenant={})", rows.len(), tenant_id);

    if rows.is_empty() {
        return Ok(0);
    }

    // 3b. Find and parse KUDGIVT.csv
    let kudgivt_file = files
        .iter()
        .find(|(name, _)| name.to_uppercase().contains("KUDGIVT"))
        .ok_or_else(|| anyhow::anyhow!("KUDGIVT.csv not found in ZIP"))?;

    let kudgivt_text = alc_csv_parser::decode_shift_jis(&kudgivt_file.1);
    let kudgivt_rows = parse_kudgivt(&kudgivt_text)?;
    let msg = format!(
        "KUDGIVT parsed: {} rows (tenant={})",
        kudgivt_rows.len(),
        tenant_id
    );
    tracing::info!("{msg}");

    // 4. Upsert masters and insert operations
    let mut operations_count = 0i32;
    for row in &rows {
        // Upsert office
        let office_id = state
            .dtako_upload
            .upsert_office(tenant_id, &row.office_cd, &row.office_name)
            .await?;
        // Upsert vehicle
        let vehicle_id = state
            .dtako_upload
            .upsert_vehicle(tenant_id, &row.vehicle_cd, &row.vehicle_name)
            .await?;
        // Upsert driver
        let driver_id = state
            .dtako_upload
            .upsert_driver(tenant_id, &row.driver_cd, &row.driver_name)
            .await?;

        let r2_key_prefix = format!("{}/unko/{}", tenant_id, row.unko_no);

        // Delete existing operation with same (tenant_id, unko_no, crew_role) for re-upload
        state
            .dtako_upload
            .delete_operation(tenant_id, &row.unko_no, row.crew_role)
            .await?;

        // Insert operation
        state
            .dtako_upload
            .insert_operation(
                tenant_id,
                &InsertOperationParams {
                    tenant_id,
                    unko_no: row.unko_no.clone(),
                    crew_role: row.crew_role,
                    reading_date: row.reading_date,
                    operation_date: row.operation_date,
                    office_id,
                    vehicle_id,
                    driver_id,
                    departure_at: row.departure_at,
                    return_at: row.return_at,
                    garage_out_at: row.garage_out_at,
                    garage_in_at: row.garage_in_at,
                    meter_start: row.meter_start,
                    meter_end: row.meter_end,
                    total_distance: row.total_distance,
                    drive_time_general: row.drive_time_general,
                    drive_time_highway: row.drive_time_highway,
                    drive_time_bypass: row.drive_time_bypass,
                    safety_score: row.safety_score,
                    economy_score: row.economy_score,
                    total_score: row.total_score,
                    raw_data: row.raw_data.clone(),
                    r2_key_prefix,
                },
            )
            .await?;

        operations_count += 1;
    }

    let msg = format!(
        "DB upsert done: {} operations (tenant={})",
        operations_count, tenant_id
    );
    tracing::info!("{msg}");

    // 5. Calculate daily_work_hours using KUDGIVT events
    // フェリー時間はCSV分割時にR2のKUDGFRYから取得済み（アップロード時はまだ未保存）
    let ferry_minutes: std::collections::HashMap<String, FerryData> =
        std::collections::HashMap::new();
    calculate_daily_hours(state, tenant_id, &rows, &kudgivt_rows, &ferry_minutes, None).await?;
    tracing::info!("calculate_daily_hours done (tenant={})", tenant_id);

    Ok(operations_count)
}

// group_operations_into_work_days は alc_compare::group_operations_into_work_days を使用

/// フェリーデータ（合計分 + 各エントリの開始時刻）
struct FerryData {
    total_minutes: i32,
    start_times: Vec<chrono::NaiveDateTime>,
    /// フェリー乗船期間(start, end)リスト
    periods: Vec<(chrono::NaiveDateTime, chrono::NaiveDateTime)>,
}

/// R2のKUDGFRYからフェリー乗船時間(分)を取得
/// Returns: unko_no → FerryData のマッピング
async fn load_ferry_minutes(
    state: &AppState,
    tenant_id: Uuid,
    rows: &[KudguriRow],
) -> std::collections::HashMap<String, FerryData> {
    use std::collections::HashMap;

    let mut ferry_map: HashMap<String, FerryData> = HashMap::new();

    let futures: Vec<_> = rows
        .iter()
        .map(|row| {
            let r2_key = format!("{}/unko/{}/KUDGFRY.csv", tenant_id, row.unko_no);
            let storage = state.dtako_storage.as_ref().unwrap().clone();
            let unko_no = row.unko_no.clone();
            async move { (unko_no, storage.download(&r2_key).await) }
        })
        .collect();

    let results = futures::future::join_all(futures).await;

    for (unko_no, result) in results {
        if let Ok(bytes) = result {
            let text = alc_csv_parser::decode_shift_jis(&bytes);
            let mut total_ferry = 0i32;
            let mut start_times = Vec::new();
            let mut periods = Vec::new();
            for line in text.lines().skip(1) {
                let cols: Vec<&str> = line.split(',').collect();
                if cols.len() <= 11 {
                    continue;
                }
                let start =
                    chrono::NaiveDateTime::parse_from_str(cols[10].trim(), "%Y/%m/%d %H:%M:%S")
                        .ok();
                let end =
                    chrono::NaiveDateTime::parse_from_str(cols[11].trim(), "%Y/%m/%d %H:%M:%S")
                        .ok();
                if let (Some(start), Some(end)) = (start, end) {
                    let secs = (end - start).num_seconds();
                    let mins = ((secs + 30) / 60) as i32;
                    if mins > 0 {
                        total_ferry += mins;
                        start_times.push(start);
                        periods.push((start, end));
                        tracing::debug!("Ferry {}: {}min ({} → {})", unko_no, mins, start, end);
                    }
                }
            }
            if total_ferry > 0 {
                ferry_map.insert(
                    unko_no,
                    FerryData {
                        total_minutes: total_ferry,
                        start_times,
                        periods,
                    },
                );
            }
        }
    }

    let msg = format!(
        "Ferry minutes loaded: {} operations with ferry",
        ferry_map.len()
    );
    tracing::info!("{msg}");
    ferry_map
}

async fn calculate_daily_hours(
    state: &AppState,
    tenant_id: Uuid,
    rows: &[KudguriRow],
    kudgivt_rows: &[KudgivtRow],
    ferry_minutes: &std::collections::HashMap<String, FerryData>,
    progress_tx: Option<tokio::sync::mpsc::Sender<String>>,
) -> Result<(), anyhow::Error> {
    use std::collections::HashMap;

    // 0. 始業ベースのワークデイグルーピング（unko_no → work_date）
    let unko_work_date = alc_compare::group_operations_into_work_days(rows);

    // 1. Load or initialize event classifications
    let classifications = load_or_init_classifications(state, tenant_id, kudgivt_rows).await?;

    // 2. Group KUDGIVT rows by unko_no
    let mut kudgivt_by_unko: HashMap<String, Vec<&KudgivtRow>> = HashMap::new();
    for row in kudgivt_rows {
        kudgivt_by_unko
            .entry(row.unko_no.clone())
            .or_default()
            .push(row);
    }

    // 2.5. 302休息イベントを始業ベースのワークデイで集計
    let mut rest_event_map: HashMap<(String, chrono::NaiveDate), i32> = HashMap::new();
    for row in kudgivt_rows {
        if classifications.get(&row.event_cd) == Some(&EventClass::RestSplit) {
            let dur = row.duration_minutes.unwrap_or(0);
            if dur <= 0 {
                continue;
            }
            let work_date = unko_work_date
                .get(&row.unko_no)
                .copied()
                .unwrap_or(row.start_at.date());
            *rest_event_map
                .entry((row.driver_cd.clone(), work_date))
                .or_insert(0) += dur;
        }
    }

    // 3. 共通 build_day_map で日別集計を構築
    use alc_compare::{build_day_map, FerryInfo};

    let build_result = build_day_map(rows, &kudgivt_by_unko, &classifications);
    let mut compare_day_map = build_result.day_map;
    let mut workday_boundaries = build_result.workday_boundaries;
    let mut day_work_events = build_result.day_work_events;

    // 3.5. FerryInfoをuploadのFerryDataから構築
    let compare_ferry_info = {
        let mut fi_minutes: HashMap<String, i32> = HashMap::new();
        let mut fi_break_dur: HashMap<String, i32> = HashMap::new();
        let mut fi_period_map: HashMap<
            String,
            Vec<(chrono::NaiveDateTime, chrono::NaiveDateTime)>,
        > = HashMap::new();
        for (unko_no, fd) in ferry_minutes.iter() {
            fi_minutes.insert(unko_no.clone(), fd.total_minutes);
            fi_period_map.insert(unko_no.clone(), fd.periods.clone());
            let Some(events) = kudgivt_by_unko.get(unko_no.as_str()) else {
                continue;
            };
            let mut break_total = 0i32;
            for ferry_start in &fd.start_times {
                let matched_301 = events
                    .iter()
                    .filter(|e| classifications.get(&e.event_cd) == Some(&EventClass::Break))
                    .filter(|e| e.duration_minutes.unwrap_or(0) > 0)
                    .min_by_key(|e| (e.start_at - *ferry_start).num_seconds().unsigned_abs());
                if let Some(evt) = matched_301 {
                    break_total += evt.duration_minutes.unwrap_or(0);
                }
            }
            if break_total > 0 {
                fi_break_dur.insert(unko_no.clone(), break_total);
            }
        }
        FerryInfo {
            ferry_minutes: fi_minutes,
            ferry_break_dur: fi_break_dur,
            ferry_period_map: fi_period_map,
        }
    };

    // 3.6. 共通 post_process_day_map で構内結合・overlap計算・フェリー控除を実行
    alc_compare::post_process_day_map(
        &mut compare_day_map,
        &mut workday_boundaries,
        &build_result.multi_wd_boundaries,
        &mut day_work_events,
        &kudgivt_by_unko,
        &classifications,
        rows,
        &compare_ferry_info,
    );

    // 3.7. compare::DayAgg → upload用の enriched 構造体に変換
    #[derive(Clone)]
    struct SegmentRecord {
        unko_no: String,
        segment_index: i32,
        start_at: chrono::NaiveDateTime,
        end_at: chrono::NaiveDateTime,
        work_minutes: i32,
        labor_minutes: i32,
        late_night_minutes: i32,
        drive_minutes: i32,
        cargo_minutes: i32,
    }

    struct UploadDayAgg {
        driver_id: Option<Uuid>,
        total_work_minutes: i32,
        total_labor_minutes: i32,
        late_night_minutes: i32,
        drive_minutes: i32,
        cargo_minutes: i32,
        total_distance: f64,
        operation_count: i32,
        unko_nos: Vec<String>,
        segments: Vec<SegmentRecord>,
        rest_event_minutes: i32,
        overlap_drive_minutes: i32,
        overlap_cargo_minutes: i32,
        overlap_break_minutes: i32,
        overlap_restraint_minutes: i32,
        ot_late_night_minutes: i32,
    }

    // driver_cd → driver_id キャッシュ
    let mut driver_id_cache: HashMap<String, Option<Uuid>> = HashMap::new();

    // unko_no → (total_distance, driver_cd) マッピング
    let mut unko_meta: HashMap<String, (f64, String)> = HashMap::new();
    for row in rows {
        unko_meta.insert(
            row.unko_no.clone(),
            (row.total_distance.unwrap_or(0.0), row.driver_cd.clone()),
        );
    }

    let mut day_map: HashMap<(String, chrono::NaiveDate, chrono::NaiveTime), UploadDayAgg> =
        HashMap::new();

    for (key, c_agg) in &compare_day_map {
        let (driver_cd, _work_date, _start_time) = key;

        // driver_id を取得（キャッシュ）
        let driver_id = if !driver_cd.is_empty() {
            if let Some(cached) = driver_id_cache.get(driver_cd) {
                *cached
            } else {
                let id = state
                    .dtako_upload
                    .get_employee_id_by_driver_cd(tenant_id, driver_cd)
                    .await?;
                driver_id_cache.insert(driver_cd.clone(), id);
                id
            }
        } else {
            None
        };

        // total_distance: 各unko_noの距離をwork_minutes比率で按分
        let total_distance: f64 = c_agg
            .unko_nos
            .iter()
            .map(|u| unko_meta.get(u).map(|(d, _)| *d).unwrap_or(0.0))
            .sum();

        // rest_event_minutes: rest_event_mapから取得
        let rest_minutes = rest_event_map
            .get(&(driver_cd.clone(), *_work_date))
            .copied()
            .unwrap_or(0);

        // SegmentRecord の構築: compare SegRec の start_at/end_at から詳細を再計算
        // unko_no の特定: セグメント時刻と operations の dep/ret を照合
        let mut segments: Vec<SegmentRecord> = Vec::new();
        // unko_no ごとのセグメントカウンター
        let mut seg_counters: HashMap<String, i32> = HashMap::new();

        for seg_rec in &c_agg.segments {
            let seg_duration = (seg_rec.end_at - seg_rec.start_at).num_minutes() as i32;
            let seg_late_night = alc_csv_parser::work_segments::calc_late_night_mins(
                seg_rec.start_at,
                seg_rec.end_at,
            );

            // unko_no を特定: どのoperationの dep..ret に含まれるか
            let unko_no = c_agg
                .unko_nos
                .iter()
                .find(|u| {
                    rows.iter().any(|r| {
                        &r.unko_no == *u
                            && r.departure_at
                                .map(|d| seg_rec.start_at >= d)
                                .unwrap_or(false)
                            && r.return_at
                                .map(|ret| seg_rec.end_at <= ret)
                                .unwrap_or(false)
                    })
                })
                .or_else(|| c_agg.unko_nos.first())
                .cloned()
                .unwrap_or_default();

            let seg_idx = seg_counters.entry(unko_no.clone()).or_insert(0);

            // drive/cargo はセグメント時間に対する日合計の比率で按分
            let day_total_seg_mins: i32 = c_agg
                .segments
                .iter()
                .map(|s| (s.end_at - s.start_at).num_minutes() as i32)
                .sum();
            let ratio = seg_duration as f64 / day_total_seg_mins.max(1) as f64;

            segments.push(SegmentRecord {
                unko_no,
                segment_index: *seg_idx,
                start_at: seg_rec.start_at,
                end_at: seg_rec.end_at,
                work_minutes: seg_duration,
                labor_minutes: ((c_agg.drive_minutes + c_agg.cargo_minutes) as f64 * ratio).round()
                    as i32,
                late_night_minutes: seg_late_night,
                drive_minutes: (c_agg.drive_minutes as f64 * ratio).round() as i32,
                cargo_minutes: (c_agg.cargo_minutes as f64 * ratio).round() as i32,
            });

            *seg_idx += 1;
        }

        day_map.insert(
            key.clone(),
            UploadDayAgg {
                driver_id,
                total_work_minutes: c_agg.total_work_minutes,
                total_labor_minutes: c_agg.drive_minutes + c_agg.cargo_minutes,
                late_night_minutes: c_agg.late_night_minutes,
                drive_minutes: c_agg.drive_minutes,
                cargo_minutes: c_agg.cargo_minutes,
                total_distance,
                operation_count: c_agg.unko_nos.len() as i32,
                unko_nos: c_agg.unko_nos.clone(),
                segments,
                rest_event_minutes: rest_minutes,
                overlap_drive_minutes: c_agg.overlap_drive_minutes,
                overlap_cargo_minutes: c_agg.overlap_cargo_minutes,
                overlap_break_minutes: c_agg.overlap_break_minutes,
                overlap_restraint_minutes: c_agg.overlap_restraint_minutes,
                ot_late_night_minutes: c_agg.ot_late_night_minutes,
            },
        );
    }

    // 4. Persist to DB
    // 日跨ぎ修正で帰属日が変わると古い日のデータが残るため、
    // 対象ドライバー×unko_noの既存データを一括削除してから再挿入する
    {
        // 全対象 unko_no を収集
        let mut all_unko_nos: Vec<String> = Vec::new();
        let mut driver_ids_seen: std::collections::HashSet<Uuid> = std::collections::HashSet::new();
        for ((_dc, _wd, _st), agg) in &day_map {
            if let Some(did) = agg.driver_id {
                driver_ids_seen.insert(did);
            }
            for u in &agg.unko_nos {
                if !all_unko_nos.contains(u) {
                    all_unko_nos.push(u.clone());
                }
            }
        }
        // unko_noベースで古いセグメント・daily_work_hours を削除
        for did in &driver_ids_seen {
            for unko in &all_unko_nos {
                state
                    .dtako_upload
                    .delete_segments_by_unko(tenant_id, *did, unko)
                    .await?;
            }
            // unko_nosカラム（配列）に含まれるエントリも削除
            state
                .dtako_upload
                .delete_daily_hours_by_unko_nos(tenant_id, *did, &all_unko_nos)
                .await?;
        }
    }

    let day_entries: Vec<_> = day_map.iter().collect();
    let save_total = day_entries.len();
    for (i, ((_driver_cd, work_date, _start_time), agg)) in day_entries.into_iter().enumerate() {
        let Some(driver_id) = agg.driver_id else {
            continue;
        };

        let rest_minutes = agg.rest_event_minutes;

        // Delete existing for re-upload (start_time含めて正確に削除)
        state
            .dtako_upload
            .delete_daily_hours_exact(tenant_id, driver_id, *work_date, *_start_time)
            .await?;

        state
            .dtako_upload
            .insert_daily_work_hours(
                tenant_id,
                &InsertDailyWorkHoursParams {
                    tenant_id,
                    driver_id,
                    work_date: *work_date,
                    start_time: *_start_time,
                    total_work_minutes: agg.total_work_minutes,
                    total_drive_minutes: agg.total_labor_minutes,
                    total_rest_minutes: rest_minutes,
                    late_night_minutes: (agg.late_night_minutes - agg.ot_late_night_minutes).max(0),
                    drive_minutes: agg.drive_minutes,
                    cargo_minutes: agg.cargo_minutes,
                    total_distance: agg.total_distance,
                    operation_count: agg.operation_count,
                    unko_nos: agg.unko_nos.clone(),
                    overlap_drive_minutes: agg.overlap_drive_minutes,
                    overlap_cargo_minutes: agg.overlap_cargo_minutes,
                    overlap_break_minutes: agg.overlap_break_minutes,
                    overlap_restraint_minutes: agg.overlap_restraint_minutes,
                    ot_late_night_minutes: agg.ot_late_night_minutes,
                },
            )
            .await?;

        // Delete and re-insert segments
        state
            .dtako_upload
            .delete_segments_by_date(tenant_id, driver_id, *work_date)
            .await?;

        for seg in &agg.segments {
            state
                .dtako_upload
                .insert_segment(
                    tenant_id,
                    &InsertSegmentParams {
                        tenant_id,
                        driver_id,
                        work_date: *work_date,
                        unko_no: seg.unko_no.clone(),
                        segment_index: seg.segment_index,
                        start_at: seg.start_at,
                        end_at: seg.end_at,
                        work_minutes: seg.work_minutes,
                        labor_minutes: seg.labor_minutes,
                        late_night_minutes: seg.late_night_minutes,
                        drive_minutes: seg.drive_minutes,
                        cargo_minutes: seg.cargo_minutes,
                    },
                )
                .await?;
        }

        if let Some(tx) = progress_tx
            .as_ref()
            .filter(|_| (i + 1) % 20 == 0 || i + 1 == save_total)
        {
            let msg = format!(
                "data: {}\n\n",
                serde_json::json!({"event":"progress","current":i+1,"total":save_total,"step":"save"})
            );
            let _ = tx.send(msg).await;
        }
    }
    Ok(())
}

/// R2のZIPからKUDGIVTを取得（テナント・月の全ZIPを走査）
async fn load_kudgivt_from_zips(
    state: &AppState,
    tenant_id: Uuid,
    month_start: chrono::NaiveDate,
    _month_end: chrono::NaiveDate,
) -> Result<Vec<KudgivtRow>, anyhow::Error> {
    // 該当月のupload_historyからZIPキーを取得
    let zip_keys = state
        .dtako_upload
        .fetch_zip_keys(tenant_id, month_start)
        .await?;

    let mut all_kudgivt = Vec::new();

    for zip_key in &zip_keys {
        match state
            .dtako_storage
            .as_ref()
            .unwrap()
            .download(zip_key)
            .await
        {
            Ok(zip_bytes) => match alc_csv_parser::extract_zip(&zip_bytes) {
                Ok(files) => {
                    if let Some((_, bytes)) = files
                        .iter()
                        .find(|(name, _)| name.to_uppercase().contains("KUDGIVT"))
                    {
                        let text = alc_csv_parser::decode_shift_jis(bytes);
                        match parse_kudgivt(&text) {
                            Ok(rows) => {
                                tracing::info!("KUDGIVT from ZIP {}: {} rows", zip_key, rows.len());
                                all_kudgivt.extend(rows);
                            }
                            Err(e) => tracing::warn!("KUDGIVT parse error in {}: {e}", zip_key),
                        }
                    }
                }
                Err(e) => tracing::warn!("ZIP extract error {}: {e}", zip_key),
            },
            Err(e) => tracing::warn!("ZIP download error {}: {e}", zip_key),
        }
    }

    // 重複排除: 同じ(unko_no, event_cd, start_at)のイベントは1つだけ保持
    // 複数ZIPに同じKUDGIVTデータが含まれる場合の対策
    let before = all_kudgivt.len();
    let mut seen = std::collections::HashSet::new();
    all_kudgivt
        .retain(|row| seen.insert((row.unko_no.clone(), row.event_cd.clone(), row.start_at)));
    let msg = format!(
        "Total KUDGIVT from ZIPs: {} rows (deduped from {})",
        all_kudgivt.len(),
        before
    );
    tracing::info!("{msg}");
    Ok(all_kudgivt)
}

/// イベント分類をDBから取得、なければデフォルトで初期化
async fn load_or_init_classifications(
    state: &AppState,
    tenant_id: Uuid,
    kudgivt_rows: &[KudgivtRow],
) -> Result<std::collections::HashMap<String, EventClass>, anyhow::Error> {
    use std::collections::HashMap;

    // DBから既存の分類を取得
    let existing = state
        .dtako_upload
        .load_event_classifications(tenant_id)
        .await?;

    let mut map: HashMap<String, EventClass> = HashMap::new();
    for (cd, cls) in &existing {
        let ec = match cls.as_str() {
            "drive" => EventClass::Drive,
            "cargo" => EventClass::Cargo,
            "work" => EventClass::Drive, // legacy fallback
            "rest_split" => EventClass::RestSplit,
            "break" => EventClass::Break,
            _ => EventClass::Ignore,
        };
        map.insert(cd.clone(), ec);
    }

    // 未登録のイベントをKUDGIVTから検出してデフォルト分類で登録
    let mut seen: std::collections::HashSet<String> = map.keys().cloned().collect();
    for row in kudgivt_rows {
        if seen.contains(&row.event_cd) {
            continue;
        }
        seen.insert(row.event_cd.clone());

        let (cls_str, ec) = default_classification(&row.event_cd);
        map.insert(row.event_cd.clone(), ec);

        let _ = state
            .dtako_upload
            .insert_event_classification(tenant_id, &row.event_cd, &row.event_name, cls_str)
            .await;
    }

    Ok(map)
}

pub fn default_classification(event_cd: &str) -> (&'static str, EventClass) {
    match event_cd {
        "201" => ("drive", EventClass::Drive),          // 走行(運転)
        "202" => ("cargo", EventClass::Cargo),          // 積み
        "203" => ("cargo", EventClass::Cargo),          // 降し
        "204" => ("cargo", EventClass::Cargo),          // その他 → 荷役
        "302" => ("rest_split", EventClass::RestSplit), // 休息
        "301" => ("break", EventClass::Break),          // 休憩
        _ => ("ignore", EventClass::Ignore),            // その他は無視
    }
}

pub fn internal_err(e: impl std::fmt::Display) -> (StatusCode, String) {
    tracing::error!("internal error: {e}");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        "internal server error".to_string(),
    )
}

/// 年月から月初・月末を計算 (month==12 の年跨ぎ対応)
pub fn compute_month_range(
    year: i32,
    month: u32,
) -> Option<(chrono::NaiveDate, chrono::NaiveDate)> {
    let start = chrono::NaiveDate::from_ymd_opt(year, month, 1)?;
    let end = if month == 12 {
        chrono::NaiveDate::from_ymd_opt(year + 1, 1, 1)?
    } else {
        chrono::NaiveDate::from_ymd_opt(year, month + 1, 1)?
    } - chrono::Duration::days(1);
    Some((start, end))
}

/// CSV split を試行 (失敗してもブロックしない)
pub(crate) async fn try_split_csv(state: &AppState, upload_id: Uuid) {
    if let Err(e) = split_csv_from_r2(state, upload_id).await {
        tracing::warn!("CSV split failed (will not block): {e}");
    }
}

/// R2 から ZIP をダウンロードして CSV を unko_no 別に分割アップロード
pub(crate) async fn split_csv_from_r2(
    state: &AppState,
    upload_id: Uuid,
) -> Result<(), anyhow::Error> {
    let record = state
        .dtako_upload
        .get_upload_tenant_and_key(upload_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("upload {} not found", upload_id))?;

    let tenant_id = record.tenant_id;
    let r2_zip_key = record.r2_zip_key;

    let dtako_st = state
        .dtako_storage
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("DTAKO_R2_BUCKET not configured"))?;
    let zip_bytes = dtako_st
        .download(&r2_zip_key)
        .await
        .map_err(|e| anyhow::anyhow!("R2 download failed: {e}"))?;

    let files = alc_csv_parser::extract_zip(&zip_bytes)?;

    let mut kudgivt_unko_nos: Vec<String> = Vec::new();

    // アップロード対象を事前に全て準備
    let mut upload_items: Vec<(String, Vec<u8>, bool)> = Vec::new(); // (key, content, is_kudgivt)
    for (name, bytes) in &files {
        if !name.to_lowercase().ends_with(".csv") {
            continue;
        }
        let utf8_text = alc_csv_parser::decode_shift_jis(bytes);
        let header = alc_csv_parser::csv_header(&utf8_text);
        let grouped = alc_csv_parser::group_csv_by_unko_no(&utf8_text);
        let is_kudgivt = name.to_uppercase().contains("KUDGIVT");

        for (unko_no, lines) in &grouped {
            let csv_name = name
                .rsplit('/')
                .next()
                .unwrap_or(name)
                .to_uppercase()
                .replace(".CSV", ".csv");
            let key = format!("{}/unko/{}/{}", tenant_id, unko_no, csv_name);
            let mut content = String::new();
            if let Some(h) = header {
                content.push_str(h);
                content.push('\n');
            }
            for line in lines {
                content.push_str(line);
                content.push('\n');
            }
            upload_items.push((key, content.into_bytes(), is_kudgivt));

            if is_kudgivt {
                kudgivt_unko_nos.push(unko_no.clone());
            }
        }
    }

    // バッチ並列アップロード（20並列）
    let batch_size = 20;
    let mut csv_count = 0usize;
    for chunk in upload_items.chunks(batch_size) {
        let futures: Vec<_> = chunk
            .iter()
            .map(|(key, content, _)| {
                let storage = state.dtako_storage.as_ref().unwrap().clone();
                let k = key.clone();
                let c = content.clone();
                async move { storage.upload(&k, &c, "text/csv").await }
            })
            .collect();
        let results = futures::future::join_all(futures).await;
        csv_count += results.len();
    }

    // has_kudgivt フラグを更新
    if !kudgivt_unko_nos.is_empty() {
        if let Err(e) = state
            .dtako_upload
            .update_has_kudgivt(tenant_id, &kudgivt_unko_nos)
            .await
        {
            tracing::error!("Failed to update has_kudgivt: {e}");
        } else {
            tracing::info!("has_kudgivt updated: {} operations", kudgivt_unko_nos.len());
        }
    }

    let msg = format!(
        "CSV split done: {} files uploaded (upload_id={}, tenant={})",
        csv_count, upload_id, tenant_id
    );
    tracing::info!("{msg}");
    Ok(())
}

/// R2 に保存済みの ZIP をダウンロード
async fn internal_download(
    State(state): State<AppState>,
    Path(upload_id): Path<Uuid>,
) -> Result<Response, (StatusCode, String)> {
    let record = state
        .dtako_upload
        .get_upload_history(upload_id)
        .await
        .map_err(internal_err)?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                format!("upload {} not found", upload_id),
            )
        })?;

    let r2_zip_key = record.r2_zip_key;
    let filename = record.filename;

    let zip_bytes = state
        .dtako_storage
        .as_ref()
        .unwrap()
        .download(&r2_zip_key)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("R2 download failed: {e}"),
            )
        })?;

    // ASCII-safe filename fallback
    let safe_name = filename
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '.' || *c == '-' || *c == '_')
        .collect::<String>();
    let safe_name = if safe_name.is_empty() {
        "download.zip".to_string()
    } else {
        safe_name
    };

    Ok(Response::builder()
        .header("Content-Type", "application/zip")
        .header(
            "Content-Disposition",
            format!("attachment; filename=\"{}\"", safe_name),
        )
        .body(Body::from(zip_bytes))
        .unwrap())
}

/// R2 に保存済みの ZIP を再処理
async fn internal_rerun(
    State(state): State<AppState>,
    Path(upload_id): Path<Uuid>,
) -> Result<Json<UploadResponse>, (StatusCode, String)> {
    // upload_history から r2_zip_key を取得
    let record = state
        .dtako_upload
        .get_upload_history(upload_id)
        .await
        .map_err(internal_err)?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                format!("upload {} not found", upload_id),
            )
        })?;

    let tenant_id = record.tenant_id;
    let r2_zip_key = record.r2_zip_key;
    let filename = record.filename;

    // R2 から ZIP をダウンロード
    let zip_bytes = state
        .dtako_storage
        .as_ref()
        .unwrap()
        .download(&r2_zip_key)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("R2 download failed: {e}"),
            )
        })?;

    let msg = format!(
        "Rerun: upload_id={}, tenant={}, file={}",
        upload_id, tenant_id, filename
    );
    tracing::info!("{msg}");

    match process_zip(&state, tenant_id, upload_id, &filename, &zip_bytes).await {
        Ok(count) => {
            state
                .dtako_upload
                .update_upload_completed(tenant_id, upload_id, count)
                .await
                .map_err(internal_err)?;

            // CSV split (non-blocking)
            try_split_csv(&state, upload_id).await;

            Ok(Json(UploadResponse {
                upload_id,
                operations_count: count,
                status: "completed".to_string(),
            }))
        }
        Err(e) => {
            let _ = state
                .dtako_upload
                .mark_upload_failed(upload_id, &e.to_string())
                .await;
            Err((StatusCode::BAD_REQUEST, e.to_string()))
        }
    }
}

#[derive(Debug, Deserialize)]
struct RecalcFilter {
    year: i32,
    month: u32,
}

/// 月指定再計算のコアロジック (テスト可能)
pub async fn recalculate_all_core(
    state: &AppState,
    tenant_id: Uuid,
    year: i32,
    month: u32,
    progress_tx: Option<tokio::sync::mpsc::Sender<String>>,
) -> Result<usize, anyhow::Error> {
    #[rustfmt::skip]
    let send = |msg: String, ptx: &Option<tokio::sync::mpsc::Sender<String>>| {
        let ptx = ptx.clone();
        async move { if let Some(tx) = ptx { let _ = tx.send(msg).await; } }
    };
    let (month_start, month_end) =
        compute_month_range(year, month).ok_or_else(|| anyhow::anyhow!("invalid year/month"))?;

    let fetch_end = month_end + chrono::Duration::days(1);
    let op_rows = state
        .dtako_upload
        .fetch_operations_for_recalc(tenant_id, month_start, fetch_end)
        .await?;

    let ops: Vec<KudguriRow> = op_rows
        .iter()
        .map(|r| KudguriRow {
            unko_no: r.unko_no.clone(),
            reading_date: r.reading_date,
            operation_date: r.operation_date,
            office_cd: String::new(),
            office_name: String::new(),
            vehicle_cd: String::new(),
            vehicle_name: String::new(),
            driver_cd: r.driver_cd.clone().unwrap_or_default(),
            driver_name: String::new(),
            crew_role: 0,
            departure_at: r.departure_at.map(|dt| dt.naive_utc()),
            return_at: r.return_at.map(|dt| dt.naive_utc()),
            garage_out_at: None,
            garage_in_at: None,
            meter_start: None,
            meter_end: None,
            total_distance: r.total_distance,
            drive_time_general: r.drive_time_general,
            drive_time_highway: r.drive_time_highway,
            drive_time_bypass: r.drive_time_bypass,
            safety_score: None,
            economy_score: None,
            total_score: None,
            raw_data: serde_json::Value::Null,
        })
        .collect();

    let total = ops.len();
    send(
        format!(
            "data: {}\n\n",
            serde_json::json!({"event":"progress","current":0,"total":total,"step":"start"})
        ),
        &progress_tx,
    )
    .await;

    // R2から各運行のKUDGIVT.csvを取得
    let mut all_kudgivt: Vec<KudgivtRow> = Vec::new();
    let batch_size = 20;
    for batch_start in (0..total).step_by(batch_size) {
        let batch_end = (batch_start + batch_size).min(total);
        let futures: Vec<_> = ops[batch_start..batch_end]
            .iter()
            .map(|op| {
                let r2_key = format!("{}/unko/{}/KUDGIVT.csv", tenant_id, op.unko_no);
                let storage = state.dtako_storage.as_ref().unwrap().clone();
                async move { (op.unko_no.clone(), storage.download(&r2_key).await) }
            })
            .collect();
        let results = futures::future::join_all(futures).await;
        for (unko_no, result) in results {
            match result {
                Ok(bytes) => {
                    let csv_text = String::from_utf8_lossy(&bytes);
                    match parse_kudgivt(&csv_text) {
                        Ok(rows) => all_kudgivt.extend(rows),
                        Err(e) => tracing::warn!("KUDGIVT parse error {}: {e}", unko_no),
                    }
                }
                Err(e) => tracing::warn!("KUDGIVT not found for {}: {e}", unko_no),
            }
        }
    }

    if all_kudgivt.is_empty() && total > 0 {
        return Err(anyhow::anyhow!(
            "KUDGIVTが見つかりません。先にCSV分割を実行してください。"
        ));
    }

    let ferry_minutes = load_ferry_minutes(state, tenant_id, &ops).await;

    calculate_daily_hours(
        state,
        tenant_id,
        &ops,
        &all_kudgivt,
        &ferry_minutes,
        progress_tx,
    )
    .await?;

    Ok(total)
}

/// 月指定で再計算（R2の個別CSVから。SSEで進捗通知）
#[allow(clippy::redundant_closure)]
async fn internal_recalculate_all(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Query(params): Query<RecalcFilter>,
) -> Response<Body> {
    let tenant_id = tenant.0 .0;
    let year = params.year;
    let month = params.month;

    let (tx, rx) = tokio::sync::mpsc::channel::<String>(32);

    tokio::spawn(async move {
        let send = |json: serde_json::Value| {
            let tx = tx.clone();
            async move {
                let s = serde_json::to_string(&json).unwrap_or_default();
                let _ = tx.send(format!("data: {s}\n\n")).await;
            }
        };

        match recalculate_all_core(&state, tenant_id, year, month, Some(tx.clone())).await {
            Ok(total) => {
                send(serde_json::json!({"event":"done","total":total,"success":total,"failed":0}))
                    .await;
            }
            Err(e) => {
                send(serde_json::json!({"event":"error","message": e.to_string()})).await;
            }
        }
    });

    let stream = tokio_stream::wrappers::ReceiverStream::new(rx)
        .map(|msg| Ok::<_, std::convert::Infallible>(msg));

    Response::builder()
        .status(200)
        .header("Content-Type", "text/event-stream")
        .header("Cache-Control", "no-cache")
        .header("X-Accel-Buffering", "no")
        .body(Body::from_stream(stream))
        .unwrap()
}

/// pending_retry / failed のアップロード一覧
async fn list_pending_uploads(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
) -> Result<Json<Vec<serde_json::Value>>, (StatusCode, String)> {
    let tenant_id = tenant.0 .0;
    let items = state
        .dtako_upload
        .list_pending_uploads(tenant_id)
        .await
        .map_err(internal_err)?;
    Ok(Json(items))
}

#[derive(Debug, Deserialize)]
struct RecalcDriverFilter {
    year: i32,
    month: u32,
    driver_id: Uuid,
}

/// recalculate 共通: ドライバーの operations を KudguriRow として取得
async fn load_driver_ops_as_kudguri(
    state: &AppState,
    tenant_id: Uuid,
    driver_id: Uuid,
    driver_cd: &str,
    month_start: chrono::NaiveDate,
    fetch_end: chrono::NaiveDate,
) -> Result<Vec<KudguriRow>, anyhow::Error> {
    let op_rows = state
        .dtako_upload
        .load_driver_operations(tenant_id, driver_id, month_start, fetch_end)
        .await?;

    Ok(op_rows
        .iter()
        .map(|r| KudguriRow {
            unko_no: r.unko_no.clone(),
            reading_date: r.reading_date,
            operation_date: r.operation_date,
            office_cd: String::new(),
            office_name: String::new(),
            vehicle_cd: String::new(),
            vehicle_name: String::new(),
            driver_cd: driver_cd.to_string(),
            driver_name: String::new(),
            crew_role: 0,
            departure_at: r.departure_at.map(|dt| dt.naive_utc()),
            return_at: r.return_at.map(|dt| dt.naive_utc()),
            garage_out_at: None,
            garage_in_at: None,
            meter_start: None,
            meter_end: None,
            total_distance: r.total_distance,
            drive_time_general: r.drive_time_general,
            drive_time_highway: r.drive_time_highway,
            drive_time_bypass: r.drive_time_bypass,
            safety_score: None,
            economy_score: None,
            total_score: None,
            raw_data: serde_json::Value::Null,
        })
        .collect())
}

/// recalculate のコア処理 (SSE なし、テストから直接呼び出し可能)
pub async fn recalculate_driver_core(
    state: &AppState,
    tenant_id: Uuid,
    driver_id: Uuid,
    year: i32,
    month: u32,
    progress_tx: Option<tokio::sync::mpsc::Sender<String>>,
) -> Result<usize, anyhow::Error> {
    let (month_start, month_end) =
        compute_month_range(year, month).ok_or_else(|| anyhow::anyhow!("invalid year/month"))?;

    // driver_cd を取得
    let driver_cd = state
        .dtako_upload
        .get_driver_cd(tenant_id, driver_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("ドライバーが見つかりません"))?;

    let fetch_end = month_end + chrono::Duration::days(1);
    let ops = load_driver_ops_as_kudguri(
        state,
        tenant_id,
        driver_id,
        &driver_cd,
        month_start,
        fetch_end,
    )
    .await?;
    let total = ops.len();

    // R2のZIPからKUDGIVT取得
    let all_kudgivt = load_kudgivt_from_zips(state, tenant_id, month_start, month_end).await?;

    // KUDGFRYからフェリー時間を取得
    let ferry_minutes = load_ferry_minutes(state, tenant_id, &ops).await;

    // 再計算
    calculate_daily_hours(
        state,
        tenant_id,
        &ops,
        &all_kudgivt,
        &ferry_minutes,
        progress_tx,
    )
    .await?;

    Ok(total)
}

/// 1ドライバー分の月次再計算（R2からKUDGIVT取得→再計算）— SSE ラッパー
#[allow(clippy::redundant_closure)]
async fn recalculate_driver(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Query(params): Query<RecalcDriverFilter>,
) -> Response<Body> {
    let tenant_id = tenant.0 .0;
    let year = params.year;
    let month = params.month;
    let driver_id = params.driver_id;

    let (tx, rx) = tokio::sync::mpsc::channel::<String>(32);

    tokio::spawn(async move {
        let send = |json: serde_json::Value| {
            let tx = tx.clone();
            async move {
                let s = serde_json::to_string(&json).unwrap_or_default();
                let _ = tx.send(format!("data: {s}\n\n")).await;
            }
        };

        send(serde_json::json!({"event":"progress","current":0,"total":0,"step":"start"})).await;

        match recalculate_driver_core(&state, tenant_id, driver_id, year, month, Some(tx.clone()))
            .await
        {
            Ok(total) => {
                send(serde_json::json!({
                    "event": "done",
                    "total": total
                }))
                .await;
            }
            Err(e) => {
                send(serde_json::json!({"event":"error","message": e.to_string()})).await;
            }
        }
    });

    let stream = tokio_stream::wrappers::ReceiverStream::new(rx)
        .map(|msg| Ok::<_, std::convert::Infallible>(msg));

    Response::builder()
        .status(200)
        .header("Content-Type", "text/event-stream")
        .header("Cache-Control", "no-cache")
        .header("X-Accel-Buffering", "no")
        .body(Body::from_stream(stream))
        .unwrap()
}

/// バッチ再計算: 1ドライバー分の処理 (事前ロード済み KUDGIVT を使用)
async fn process_single_driver_batch(
    state: &AppState,
    tenant_id: Uuid,
    driver_id: Uuid,
    month_start: chrono::NaiveDate,
    fetch_end: chrono::NaiveDate,
    all_kudgivt: &[KudgivtRow],
) -> Result<(), anyhow::Error> {
    let driver_cd = state
        .dtako_upload
        .get_driver_cd(tenant_id, driver_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("driver not found"))?;

    let ops = load_driver_ops_as_kudguri(
        state,
        tenant_id,
        driver_id,
        &driver_cd,
        month_start,
        fetch_end,
    )
    .await?;
    let ferry_minutes = load_ferry_minutes(state, tenant_id, &ops).await;

    calculate_daily_hours(state, tenant_id, &ops, all_kudgivt, &ferry_minutes, None).await?;
    Ok(())
}

/// 複数ドライバー一括再計算（SSEストリーム）
#[derive(Deserialize)]
struct BatchRecalcBody {
    year: i32,
    month: u32,
    driver_ids: Vec<Uuid>,
}

/// バッチ再計算コアロジック (テスト可能)
pub async fn recalculate_drivers_batch_core(
    state: &AppState,
    tenant_id: Uuid,
    year: i32,
    month: u32,
    driver_ids: &[Uuid],
) -> Result<(usize, usize), anyhow::Error> {
    let (month_start, month_end) =
        compute_month_range(year, month).ok_or_else(|| anyhow::anyhow!("invalid year/month"))?;

    let all_kudgivt = load_kudgivt_from_zips(state, tenant_id, month_start, month_end).await?;
    let fetch_end = month_end + chrono::Duration::days(1);
    let all_kudgivt = std::sync::Arc::new(all_kudgivt);

    let done_count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let error_count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));

    let chunks: Vec<Vec<Uuid>> = driver_ids.chunks(10).map(|c| c.to_vec()).collect();
    for chunk in chunks {
        let futs: Vec<_> = chunk
            .iter()
            .map(|driver_id| {
                let state = state.clone();
                let all_kudgivt = all_kudgivt.clone();
                let done_count = done_count.clone();
                let error_count = error_count.clone();
                let driver_id = *driver_id;
                async move {
                    match process_single_driver_batch(
                        &state,
                        tenant_id,
                        driver_id,
                        month_start,
                        fetch_end,
                        &all_kudgivt,
                    )
                    .await
                    {
                        Ok(()) => {
                            done_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        }
                        Err(_) => {
                            error_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        }
                    }
                }
            })
            .collect();
        futures::future::join_all(futs).await;
    }

    let done = done_count.load(std::sync::atomic::Ordering::Relaxed);
    let errors = error_count.load(std::sync::atomic::Ordering::Relaxed);
    Ok((done, errors))
}

#[allow(clippy::redundant_closure)]
async fn recalculate_drivers_batch(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Json(body): Json<BatchRecalcBody>,
) -> Response<Body> {
    let tenant_id = tenant.0 .0;
    let year = body.year;
    let month = body.month;
    let driver_ids = body.driver_ids;

    let (tx, rx) = tokio::sync::mpsc::channel::<String>(32);

    tokio::spawn(async move {
        let send = |json: serde_json::Value| {
            let tx = tx.clone();
            async move {
                let s = serde_json::to_string(&json).unwrap_or_default();
                let _ = tx.send(format!("data: {s}\n\n")).await;
            }
        };

        let total_drivers = driver_ids.len();
        send(serde_json::json!({"event":"batch_start","total_drivers":total_drivers})).await;

        match recalculate_drivers_batch_core(&state, tenant_id, year, month, &driver_ids).await {
            Ok((done, errors)) => {
                send(serde_json::json!({"event":"batch_done","total":total_drivers,"done":done,"errors":errors})).await;
            }
            Err(e) => {
                send(serde_json::json!({"event":"error","message": e.to_string()})).await;
            }
        }
    });

    let stream = tokio_stream::wrappers::ReceiverStream::new(rx)
        .map(|msg| Ok::<_, std::convert::Infallible>(msg));

    Response::builder()
        .status(200)
        .header("Content-Type", "text/event-stream")
        .header("Cache-Control", "no-cache")
        .header("X-Accel-Buffering", "no")
        .body(Body::from_stream(stream))
        .unwrap()
}

/// アップロード一覧
async fn list_uploads(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
) -> Result<Json<Vec<serde_json::Value>>, (StatusCode, String)> {
    let tenant_id = tenant.0 .0;
    let items = state
        .dtako_upload
        .list_uploads(tenant_id)
        .await
        .map_err(internal_err)?;
    Ok(Json(items))
}

/// 認証付きCSV分割エンドポイント
async fn split_csv_handler(
    State(state): State<AppState>,
    _tenant: axum::Extension<TenantId>,
    Path(upload_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    tracing::info!("split-csv (auth) called: upload_id={}", upload_id);

    split_csv_from_r2(&state, upload_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(
        serde_json::json!({ "status": "ok", "upload_id": upload_id }),
    ))
}

/// CSV 一括分割コアロジック (テスト可能)
pub async fn split_csv_all_core(
    state: &AppState,
    tenant_id: Uuid,
) -> Result<(usize, usize), anyhow::Error> {
    let uploads = state
        .dtako_upload
        .list_uploads_needing_split(tenant_id)
        .await?;

    let mut seen_filenames = std::collections::HashSet::new();
    let uploads: Vec<_> = uploads
        .into_iter()
        .filter(|(_, f)| seen_filenames.insert(f.clone()))
        .collect();

    let total = uploads.len();
    if total == 0 {
        return Ok((0, 0));
    }

    let success = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let failed = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));

    for chunk in uploads.chunks(5) {
        let futures: Vec<_> = chunk
            .iter()
            .map(|(upload_id, _)| {
                let st = state.clone();
                let uid = *upload_id;
                let s = success.clone();
                let f = failed.clone();
                async move {
                    match split_csv_from_r2(&st, uid).await {
                        Ok(()) => {
                            s.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        }
                        Err(e) => {
                            tracing::warn!("split failed for {}: {e}", uid);
                            f.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        }
                    }
                }
            })
            .collect();
        futures::future::join_all(futures).await;
    }

    Ok((
        success.load(std::sync::atomic::Ordering::Relaxed),
        failed.load(std::sync::atomic::Ordering::Relaxed),
    ))
}

/// 全completedアップロードのCSV分割（SSE進捗）
#[allow(clippy::redundant_closure)]
async fn split_csv_all_handler(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
) -> Response<Body> {
    let tenant_id = tenant.0 .0;

    let (tx, rx) = tokio::sync::mpsc::channel::<String>(32);

    tokio::spawn(async move {
        let send = |json: serde_json::Value| {
            let tx = tx.clone();
            async move {
                let s = serde_json::to_string(&json).unwrap_or_default();
                let _ = tx.send(format!("data: {s}\n\n")).await;
            }
        };

        match split_csv_all_core(&state, tenant_id).await {
            Ok((success, failed)) => {
                send(serde_json::json!({"event":"done","total":success+failed,"success":success,"failed":failed})).await;
            }
            Err(e) => {
                send(serde_json::json!({"event":"error","message": e.to_string()})).await;
            }
        }
    });

    let stream = tokio_stream::wrappers::ReceiverStream::new(rx)
        .map(|msg| Ok::<_, std::convert::Infallible>(msg));

    Response::builder()
        .status(200)
        .header("Content-Type", "text/event-stream")
        .header("Cache-Control", "no-cache")
        .header("X-Accel-Buffering", "no")
        .body(Body::from_stream(stream))
        .unwrap()
}
