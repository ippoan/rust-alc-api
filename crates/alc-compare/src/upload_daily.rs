//! アップロードの取り込みが保存する、日別の労働時間とセグメントの計算 (Refs ippoan/rust-alc-api#725)。
//!
//! backend の `alc-dtako` (`calculate_daily_hours`) と分割 worker (ippoan/alc-dtako-worker) が、同じこの関数を呼ぶ。
//! **DB を呼ばない** (分類の読み込み・乗務員 id の引き当て・削除・保存は呼び手が持つ)。
//! 式は `calculate_daily_hours` に在ったものをそのまま移した — 数値を変えない。

use std::collections::HashMap;

use chrono::{NaiveDate, NaiveDateTime};

use alc_csv_parser::kudgivt::KudgivtRow;
use alc_csv_parser::kudguri::KudguriRow;
use alc_csv_parser::work_segments::EventClass;

use crate::{
    build_day_map, group_operations_into_work_days, post_process_day_map, DayKey, FerryInfo,
};

/// フェリーデータ（合計分 + 各エントリの開始時刻）
#[derive(Debug, Clone)]
pub struct FerryData {
    pub total_minutes: i32,
    pub start_times: Vec<NaiveDateTime>,
    /// フェリー乗船期間(start, end)リスト
    pub periods: Vec<(NaiveDateTime, NaiveDateTime)>,
}

/// 保存するセグメント 1 件 (`dtako_daily_work_segments` の 1 行ぶん)。
#[derive(Debug, Clone, PartialEq)]
pub struct DailySegment {
    pub unko_no: String,
    pub segment_index: i32,
    pub start_at: NaiveDateTime,
    pub end_at: NaiveDateTime,
    pub work_minutes: i32,
    pub labor_minutes: i32,
    pub late_night_minutes: i32,
    pub drive_minutes: i32,
    pub cargo_minutes: i32,
}

/// 日エントリ 1 件 (`dtako_daily_work_hours` の 1 行ぶん + そのセグメント)。key は [`DayKey`]。
#[derive(Debug, Clone, PartialEq)]
pub struct DailyHours {
    pub total_work_minutes: i32,
    pub total_labor_minutes: i32,
    pub late_night_minutes: i32,
    pub drive_minutes: i32,
    pub cargo_minutes: i32,
    pub total_distance: f64,
    pub operation_count: i32,
    pub unko_nos: Vec<String>,
    pub segments: Vec<DailySegment>,
    pub rest_event_minutes: i32,
    pub overlap_drive_minutes: i32,
    pub overlap_cargo_minutes: i32,
    pub overlap_break_minutes: i32,
    pub overlap_restraint_minutes: i32,
    pub ot_late_night_minutes: i32,
}

impl DailyHours {
    /// 保存する `total_drive_minutes` の値 (= 労働の合計)。
    pub fn saved_total_drive_minutes(&self) -> i32 {
        self.total_labor_minutes
    }

    /// 保存する `late_night_minutes` の値 (時間外の深夜を引いたもの。負にはしない)。
    pub fn saved_late_night_minutes(&self) -> i32 {
        (self.late_night_minutes - self.ot_late_night_minutes).max(0)
    }
}

/// KUDGURI・KUDGIVT の行と分類・フェリーから、日エントリ ([`DayKey`] → [`DailyHours`]) を計算する。
pub fn compute_daily_hours(
    rows: &[KudguriRow],
    kudgivt_rows: &[KudgivtRow],
    classifications: &HashMap<String, EventClass>,
    ferry: &HashMap<String, FerryData>,
) -> HashMap<DayKey, DailyHours> {
    // 0. 始業ベースのワークデイグルーピング（unko_no → work_date）
    let unko_work_date = group_operations_into_work_days(rows);

    // 2. Group KUDGIVT rows by unko_no
    let mut kudgivt_by_unko: HashMap<String, Vec<&KudgivtRow>> = HashMap::new();
    for row in kudgivt_rows {
        kudgivt_by_unko
            .entry(row.unko_no.clone())
            .or_default()
            .push(row);
    }

    // 2.5. 302休息イベントを始業ベースのワークデイで集計
    let mut rest_event_map: HashMap<(String, NaiveDate), i32> = HashMap::new();
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
    let build_result = build_day_map(rows, &kudgivt_by_unko, classifications);
    let mut compare_day_map = build_result.day_map;
    let mut workday_boundaries = build_result.workday_boundaries;
    let mut day_work_events = build_result.day_work_events;

    // 3.5. FerryInfoをuploadのFerryDataから構築
    let compare_ferry_info = {
        let mut fi_minutes: HashMap<String, i32> = HashMap::new();
        let mut fi_break_dur: HashMap<String, i32> = HashMap::new();
        let mut fi_period_map: HashMap<String, Vec<(NaiveDateTime, NaiveDateTime)>> =
            HashMap::new();
        for (unko_no, fd) in ferry.iter() {
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
    post_process_day_map(
        &mut compare_day_map,
        &mut workday_boundaries,
        &build_result.multi_wd_boundaries,
        &mut day_work_events,
        &kudgivt_by_unko,
        classifications,
        rows,
        &compare_ferry_info,
    );

    // 3.7. compare::DayAgg → upload用の enriched 構造体に変換
    // unko_no → (total_distance, driver_cd) マッピング
    let mut unko_meta: HashMap<String, (f64, String)> = HashMap::new();
    for row in rows {
        unko_meta.insert(
            row.unko_no.clone(),
            (row.total_distance.unwrap_or(0.0), row.driver_cd.clone()),
        );
    }

    let mut day_map: HashMap<DayKey, DailyHours> = HashMap::new();

    for (key, c_agg) in &compare_day_map {
        let (driver_cd, _work_date, _start_time) = key;

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
        let mut segments: Vec<DailySegment> = Vec::new();
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

            segments.push(DailySegment {
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
            DailyHours {
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

    day_map
}
