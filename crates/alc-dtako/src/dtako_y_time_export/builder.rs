//! 1 ドライバー分の segment 列 (start, end, rest_minutes) を Y時間 シート行に
//! bucketing する pure logic。DB / R2 アクセスはここに含まない。

use chrono::{NaiveDate, NaiveDateTime, Timelike};

use super::models::YTimeRow;

#[derive(Debug, Clone, PartialEq)]
pub struct SegmentInput {
    pub start: NaiveDateTime,
    pub end: NaiveDateTime,
    /// 後方互換用 (sum)。実態は rest_intervals の duration 合計と一致。
    pub rest_minutes: i32,
    /// 各休憩 (event_cd=301) の (start, end)。bucket 確定後に時間帯別に振り分けるために必要。
    pub rest_intervals: Vec<(NaiveDateTime, NaiveDateTime)>,
    pub note: Option<String>,
}

/// (A) cutoff: 所定労働時間 (h)。これ以上の勤務時間 (= end - start - rest) は深夜跨ぎ時に
/// 終業日側 row に F=1 で記録、未満なら始業日 row に 24h+ 表記で記録。
const WORK_CUTOFF_HOURS: f64 = 7.0;

/// `segments` を bucketing して `YTimeRow` の列を返す。
///
/// 戻り値: `(rows, warnings)`
///
/// ルール:
/// - 同日 (start.date() == end.date()) → bucket = start.date()、F=0
/// - 深夜跨ぎ + 勤務時間 < 7h → bucket = start.date()、F=0、H 列 24h+ 表記
/// - 深夜跨ぎ + 勤務時間 ≥ 7h → bucket = end.date()、F=1、H 列 = end の minutes_of_day
///   (テンプレ数式が「F=1 のとき G を 0 として扱う」ので、当日 0:00 から H までを当日労働として計算)
/// - 期間外 (`bucket_date < from || > to`) は drop
/// - 同 bucket_date に複数 segment: 結合 (G=最早, H=最遅, rest=各 segment 合計, F=any)
pub fn build_y_time_rows(
    mut segments: Vec<SegmentInput>,
    from: NaiveDate,
    to: NaiveDate,
) -> (Vec<YTimeRow>, Vec<String>) {
    segments.sort_by_key(|s| s.start);

    let mut warnings: Vec<String> = Vec::new();
    // bucket_date → 集約中の row データ
    let mut buckets: std::collections::HashMap<NaiveDate, BucketAccum> =
        std::collections::HashMap::new();
    let mut bucket_order: Vec<NaiveDate> = Vec::new();

    for seg in segments {
        let work_minutes = (seg.end - seg.start).num_minutes() as i32 - seg.rest_minutes;
        let work_hours = work_minutes as f64 / 60.0;

        let (bucket, previous_day_start, end_min_from_bucket) =
            if seg.start.date() == seg.end.date() {
                (seg.start.date(), false, minutes_of_day(seg.end))
            } else if work_hours >= WORK_CUTOFF_HOURS {
                // 深夜跨ぎ + 長時間勤務 → 終業日 row F=1
                (seg.end.date(), true, minutes_of_day(seg.end))
            } else {
                // 深夜跨ぎ + 短時間 → 始業日 row 24h+ 表記
                let bucket = seg.start.date();
                let bucket_midnight = bucket.and_hms_opt(0, 0, 0).expect("valid midnight");
                let end_min = (seg.end - bucket_midnight).num_minutes() as i32;
                (bucket, false, end_min)
            };

        // 期間外
        if bucket < from || bucket > to {
            continue;
        }

        // 休憩 7 セル split を計算 (bucket date に対する 前日/当日/翌日 で時間帯振り分け)
        let rest_split = split_rest_intervals(&seg.rest_intervals, bucket);

        let entry = buckets.entry(bucket).or_insert_with(|| {
            bucket_order.push(bucket);
            BucketAccum::new(bucket)
        });

        let already_had_seg = entry.has_segment;
        entry.merge(SegInBucket {
            start_min: minutes_of_day(seg.start),
            end_min: end_min_from_bucket,
            previous_day_start,
            rest: rest_split,
            note: seg.note,
        });

        if already_had_seg {
            warnings.push(format!(
                "{bucket}: 複数 segment 結合 (1 行に集約: 最早始業 / 最遅終業 / 休憩合計)"
            ));
        }
    }

    let mut rows: Vec<YTimeRow> = bucket_order
        .into_iter()
        .map(|d| buckets.remove(&d).expect("bucket exists").into_row())
        .collect();
    rows.sort_by_key(|r| r.date);
    (rows, warnings)
}

/// 休憩 (event_cd=301) intervals を、bucket date を基準にして 7 セル (前日/当日/翌日 × 時間帯) に振り分け。
fn split_rest_intervals(
    intervals: &[(NaiveDateTime, NaiveDateTime)],
    bucket: NaiveDate,
) -> RestSplit {
    let mut s = RestSplit::default();
    let prev = bucket.pred_opt().expect("valid prev date");
    let next = bucket.succ_opt().expect("valid next date");
    for (start, end) in intervals {
        // 前日
        s.prev_5_22 += overlap_minutes(*start, *end, prev, 5 * 60, 22 * 60);
        s.prev_22_0 += overlap_minutes(*start, *end, prev, 22 * 60, 24 * 60);
        // 当日
        s.today_0_5 += overlap_minutes(*start, *end, bucket, 0, 5 * 60);
        s.today_5_22 += overlap_minutes(*start, *end, bucket, 5 * 60, 22 * 60);
        s.today_22_0 += overlap_minutes(*start, *end, bucket, 22 * 60, 24 * 60);
        // 翌日
        s.next_0_5 += overlap_minutes(*start, *end, next, 0, 5 * 60);
        s.next_5_22 += overlap_minutes(*start, *end, next, 5 * 60, 22 * 60);
    }
    s
}

/// interval (start, end) と (date 0:00 + start_min, date 0:00 + end_min) の重複分数。
fn overlap_minutes(
    a_start: NaiveDateTime,
    a_end: NaiveDateTime,
    date: NaiveDate,
    start_min: i32,
    end_min: i32,
) -> i32 {
    let midnight = date.and_hms_opt(0, 0, 0).expect("valid midnight");
    let b_start = midnight + chrono::Duration::minutes(start_min as i64);
    let b_end = midnight + chrono::Duration::minutes(end_min as i64);
    let lo = a_start.max(b_start);
    let hi = a_end.min(b_end);
    if hi > lo {
        (hi - lo).num_minutes() as i32
    } else {
        0
    }
}

#[derive(Debug, Default, Clone, PartialEq)]
struct RestSplit {
    prev_5_22: i32,
    prev_22_0: i32,
    today_0_5: i32,
    today_5_22: i32,
    today_22_0: i32,
    next_0_5: i32,
    next_5_22: i32,
}

impl RestSplit {
    fn add(&mut self, other: &RestSplit) {
        self.prev_5_22 += other.prev_5_22;
        self.prev_22_0 += other.prev_22_0;
        self.today_0_5 += other.today_0_5;
        self.today_5_22 += other.today_5_22;
        self.today_22_0 += other.today_22_0;
        self.next_0_5 += other.next_0_5;
        self.next_5_22 += other.next_5_22;
    }
}

struct SegInBucket {
    start_min: i32,
    end_min: i32,
    previous_day_start: bool,
    rest: RestSplit,
    note: Option<String>,
}

struct BucketAccum {
    date: NaiveDate,
    has_segment: bool,
    /// 最早始業 (= 一番小さい start_min)
    earliest_start: i32,
    /// 最遅終業 (= 一番大きい end_min。bucket midnight 起点 24h+ 含む)
    latest_end: i32,
    rest: RestSplit,
    previous_day_start: bool,
    notes: Vec<String>,
}

impl BucketAccum {
    fn new(date: NaiveDate) -> Self {
        Self {
            date,
            has_segment: false,
            earliest_start: 0,
            latest_end: 0,
            rest: RestSplit::default(),
            previous_day_start: false,
            notes: Vec::new(),
        }
    }

    fn merge(&mut self, seg: SegInBucket) {
        if !self.has_segment {
            self.earliest_start = seg.start_min;
            self.latest_end = seg.end_min;
            self.has_segment = true;
        } else {
            // F=1 (前日始業) は値が大きいことが多い (前日 22:00 = 1320) ので min/max を素朴に取ると
            // バグる。F が混在するなら「F=1 を優先」する: F=1 の start_min はそのまま記録、
            // F=0 の start_min は無視 (テンプレ数式上 F=1 の G は 0 として扱うため、表示用)
            if seg.previous_day_start && !self.previous_day_start {
                // F=1 の segment を採用
                self.earliest_start = seg.start_min;
            } else if seg.previous_day_start == self.previous_day_start {
                // 同じ F なら最早を取る
                self.earliest_start = self.earliest_start.min(seg.start_min);
            }
            // 終業は常に最遅
            self.latest_end = self.latest_end.max(seg.end_min);
        }
        self.rest.add(&seg.rest);
        self.previous_day_start |= seg.previous_day_start;
        if let Some(n) = seg.note {
            self.notes.push(n);
        }
    }

    fn into_row(self) -> YTimeRow {
        YTimeRow {
            date: self.date,
            previous_day_start: self.previous_day_start,
            start_minutes_of_day: self.earliest_start,
            end_minutes_from_bucket_date: self.latest_end,
            rest_prev_5_22: self.rest.prev_5_22,
            rest_prev_22_0: self.rest.prev_22_0,
            rest_today_0_5: self.rest.today_0_5,
            rest_today_5_22: self.rest.today_5_22,
            rest_today_22_0: self.rest.today_22_0,
            rest_next_0_5: self.rest.next_0_5,
            rest_next_5_22: self.rest.next_5_22,
            note: if self.notes.is_empty() {
                None
            } else {
                Some(self.notes.join(" / "))
            },
        }
    }
}

fn minutes_of_day(dt: NaiveDateTime) -> i32 {
    dt.time().hour() as i32 * 60 + dt.time().minute() as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dt(date: (i32, u32, u32), time: (u32, u32)) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(date.0, date.1, date.2)
            .unwrap()
            .and_hms_opt(time.0, time.1, 0)
            .unwrap()
    }

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    fn seg_simple(
        start: NaiveDateTime,
        end: NaiveDateTime,
        rest: i32,
        note: Option<&str>,
    ) -> SegmentInput {
        SegmentInput {
            start,
            end,
            rest_minutes: rest,
            rest_intervals: Vec::new(),
            note: note.map(String::from),
        }
    }

    #[test]
    fn single_same_day_segment() {
        let s = seg_simple(
            dt((2024, 4, 15), (8, 30)),
            dt((2024, 4, 15), (17, 0)),
            60,
            None,
        );
        let (rows, warns) = build_y_time_rows(vec![s], d(2024, 4, 1), d(2024, 4, 30));
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].date, d(2024, 4, 15));
        assert!(!rows[0].previous_day_start);
        assert_eq!(rows[0].start_minutes_of_day, 8 * 60 + 30);
        assert_eq!(rows[0].end_minutes_from_bucket_date, 17 * 60);
        assert!(warns.is_empty());
    }

    #[test]
    fn cross_midnight_short_work_uses_start_date_24h_plus() {
        // 4/15 22:30 → 4/16 04:30 (work 6h, rest 30 → work 5.5h < 7h cutoff)
        let s = SegmentInput {
            start: dt((2024, 4, 15), (22, 30)),
            end: dt((2024, 4, 16), (4, 30)),
            rest_minutes: 30,
            rest_intervals: vec![(dt((2024, 4, 16), (1, 0)), dt((2024, 4, 16), (1, 30)))],
            note: None,
        };
        let (rows, warns) = build_y_time_rows(vec![s], d(2024, 4, 1), d(2024, 4, 30));
        assert_eq!(rows.len(), 1);
        // start_date 側 (4/15) に 24h+ で記録
        assert_eq!(rows[0].date, d(2024, 4, 15));
        assert!(!rows[0].previous_day_start);
        assert_eq!(rows[0].start_minutes_of_day, 22 * 60 + 30);
        // 28:30 = 4:30 翌日 = 28*60+30 = 1710
        assert_eq!(rows[0].end_minutes_from_bucket_date, 28 * 60 + 30);
        // 休憩 4/16 1:00-1:30 → 4/15 bucket では「翌日 0-5時」
        assert_eq!(rows[0].rest_next_0_5, 30);
        assert!(warns.is_empty());
    }

    #[test]
    fn cross_midnight_long_work_uses_end_date_with_f1() {
        // 4/15 22:00 → 4/16 06:00 (work 8h ≥ 7h cutoff)
        let s = seg_simple(
            dt((2024, 4, 15), (22, 0)),
            dt((2024, 4, 16), (6, 0)),
            0,
            None,
        );
        let (rows, _warns) = build_y_time_rows(vec![s], d(2024, 4, 1), d(2024, 4, 30));
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].date, d(2024, 4, 16));
        assert!(rows[0].previous_day_start);
        assert_eq!(rows[0].start_minutes_of_day, 22 * 60);
        assert_eq!(rows[0].end_minutes_from_bucket_date, 6 * 60);
    }

    #[test]
    fn template_example_two_starts_on_same_calendar_day() {
        // テンプレ作者の例: 5/14 1:00-12:00 + 5/14 22:30 → 5/15 9:30
        let s1 = seg_simple(
            dt((2024, 5, 14), (1, 0)),
            dt((2024, 5, 14), (12, 0)),
            0,
            None,
        );
        let s2 = seg_simple(
            dt((2024, 5, 14), (22, 30)),
            dt((2024, 5, 15), (9, 30)),
            0,
            None,
        );
        let (rows, warns) = build_y_time_rows(vec![s1, s2], d(2024, 5, 1), d(2024, 5, 31));
        assert_eq!(rows.len(), 2);
        // 5/14 row: same-day
        assert_eq!(rows[0].date, d(2024, 5, 14));
        assert!(!rows[0].previous_day_start);
        assert_eq!(rows[0].start_minutes_of_day, 60);
        assert_eq!(rows[0].end_minutes_from_bucket_date, 12 * 60);
        // 5/15 row: cross-midnight, 11h ≥ 7h → end_date with F=1
        assert_eq!(rows[1].date, d(2024, 5, 15));
        assert!(rows[1].previous_day_start);
        assert_eq!(rows[1].start_minutes_of_day, 22 * 60 + 30);
        assert_eq!(rows[1].end_minutes_from_bucket_date, 9 * 60 + 30);
        assert!(warns.is_empty());
    }

    #[test]
    fn same_bucket_combine_two_segments() {
        // 4/2 22:00 → 4/3 09:00 (11h cross-midnight, ≥ 7h → 4/3 F=1)
        // 4/3 11:00 → 4/3 17:00 (6h same-day → 4/3)
        // → 4/3 row に結合: F=1, 始業 22:00 (前日), 終業 17:00
        let s1 = seg_simple(dt((2024, 4, 2), (22, 0)), dt((2024, 4, 3), (9, 0)), 0, None);
        let s2 = seg_simple(
            dt((2024, 4, 3), (11, 0)),
            dt((2024, 4, 3), (17, 0)),
            0,
            None,
        );
        let (rows, warns) = build_y_time_rows(vec![s1, s2], d(2024, 4, 1), d(2024, 4, 30));
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].date, d(2024, 4, 3));
        assert!(rows[0].previous_day_start);
        // F=1 を優先で start = 22:00 (前日)
        assert_eq!(rows[0].start_minutes_of_day, 22 * 60);
        // 終業最遅 17:00
        assert_eq!(rows[0].end_minutes_from_bucket_date, 17 * 60);
        assert_eq!(warns.len(), 1);
        assert!(warns[0].contains("複数 segment 結合"));
    }

    #[test]
    fn out_of_range_filtered() {
        let s = seg_simple(
            dt((2024, 4, 15), (8, 0)),
            dt((2024, 4, 15), (17, 0)),
            0,
            None,
        );
        let (rows, _) = build_y_time_rows(vec![s], d(2024, 4, 16), d(2024, 4, 30));
        assert!(rows.is_empty());
    }

    #[test]
    fn multiple_distinct_days_sorted_ascending() {
        let segments = vec![
            seg_simple(
                dt((2024, 4, 18), (8, 0)),
                dt((2024, 4, 18), (17, 0)),
                0,
                None,
            ),
            seg_simple(
                dt((2024, 4, 15), (8, 0)),
                dt((2024, 4, 15), (17, 0)),
                0,
                None,
            ),
            seg_simple(
                dt((2024, 4, 22), (8, 0)),
                dt((2024, 4, 22), (17, 0)),
                0,
                None,
            ),
        ];
        let (rows, warns) = build_y_time_rows(segments, d(2024, 4, 1), d(2024, 4, 30));
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].date, d(2024, 4, 15));
        assert_eq!(rows[1].date, d(2024, 4, 18));
        assert_eq!(rows[2].date, d(2024, 4, 22));
        assert!(warns.is_empty());
    }

    #[test]
    fn rest_split_categorizes_by_time_of_day() {
        // 4/15 8:00 - 4/15 22:00 (same-day) with rests:
        //   4/15 4:00-5:00 (前日?? いや、同日の 0-5時) ← 当日 0-5
        //   4/15 12:00-13:00 (5-22 帯) ← 当日 5-22
        //   4/15 23:00-23:30 (22-0 帯) ← 当日 22-0
        let s = SegmentInput {
            start: dt((2024, 4, 15), (8, 0)),
            end: dt((2024, 4, 15), (22, 0)),
            rest_minutes: 150,
            rest_intervals: vec![
                (dt((2024, 4, 15), (4, 0)), dt((2024, 4, 15), (5, 0))),
                (dt((2024, 4, 15), (12, 0)), dt((2024, 4, 15), (13, 0))),
                (dt((2024, 4, 15), (23, 0)), dt((2024, 4, 15), (23, 30))),
            ],
            note: None,
        };
        let (rows, _) = build_y_time_rows(vec![s], d(2024, 4, 1), d(2024, 4, 30));
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].rest_today_0_5, 60);
        assert_eq!(rows[0].rest_today_5_22, 60);
        assert_eq!(rows[0].rest_today_22_0, 30);
        assert_eq!(rows[0].rest_prev_5_22, 0);
        assert_eq!(rows[0].rest_next_0_5, 0);
    }

    #[test]
    fn note_passes_through() {
        let s = seg_simple(
            dt((2024, 4, 15), (8, 0)),
            dt((2024, 4, 15), (17, 0)),
            0,
            Some("テスト備考"),
        );
        let (rows, _) = build_y_time_rows(vec![s], d(2024, 4, 1), d(2024, 4, 30));
        assert_eq!(rows[0].note.as_deref(), Some("テスト備考"));
    }

    #[test]
    fn empty_input_yields_empty_output() {
        let (rows, warns) = build_y_time_rows(vec![], d(2024, 4, 1), d(2024, 4, 30));
        assert!(rows.is_empty());
        assert!(warns.is_empty());
    }
}
