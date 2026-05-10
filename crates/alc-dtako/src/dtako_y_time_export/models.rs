use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

/// Y時間 シート 1 行分の出力データ。
///
/// `*_minutes_*` は **整数の分数** で返す。Worker 側で `/1440` して
/// fractional-day numeric として cell に書き込むことで、テンプレ既存の
/// `[h]:mm` 形式 (`25:30` のような 24h+ 表示も含む) を維持する。
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct YTimeRow {
    /// A 列とマッチングする bucket date (yyyy-mm-dd)
    pub date: NaiveDate,
    /// F 列: true → `1`、false → 空。1 暦日 2 始業ケース or 深夜跨ぎ + 勤務 ≥ 7h の終業日側で true。
    pub previous_day_start: bool,
    /// G 列の元値: 始業時刻の 0:00 からの分 (0..=1439)。
    /// F=1 のとき: 前日始業時刻 (前日 0:00 起点)。
    pub start_minutes_of_day: i32,
    /// H 列の元値: 終業時刻の bucket_date 0:00 からの分。
    /// 24h 越え時は 1440 以上 (例: 翌日 09:30 → 33h30m → 2010)。
    pub end_minutes_from_bucket_date: i32,
    /// I 列 (前日 5-22時): 休憩時間 (分)
    pub rest_prev_5_22: i32,
    /// J 列 (前日 22-0時): 休憩時間 (分)
    pub rest_prev_22_0: i32,
    /// K 列 (当日 0-5時): 休憩時間 (分)
    pub rest_today_0_5: i32,
    /// L 列 (当日 5-22時): 休憩時間 (分)
    pub rest_today_5_22: i32,
    /// M 列 (当日 22-0時): 休憩時間 (分)
    pub rest_today_22_0: i32,
    /// N 列 (翌日 0-5時): 休憩時間 (分)
    pub rest_next_0_5: i32,
    /// O 列 (翌日 5-22時): 休憩時間 (分)
    pub rest_next_5_22: i32,
    /// C 列: 自由文 (オプション)。同 bucket 結合や 24h cut 時の備考も入れる
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct YTimeDriver {
    pub cd: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct YTimePeriod {
    pub from: NaiveDate,
    pub to: NaiveDate,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct YTimeExportResponse {
    pub driver: YTimeDriver,
    pub period: YTimePeriod,
    pub rows: Vec<YTimeRow>,
    /// 例: 同 bucket_date に複数 segment が出現した場合の警告
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct YTimeExportQuery {
    pub driver_cd: String,
    pub from: NaiveDate,
    pub to: NaiveDate,
}
