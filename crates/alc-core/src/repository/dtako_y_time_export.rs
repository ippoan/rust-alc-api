use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use uuid::Uuid;

/// `dtako_operations` 1 行から Y時間 export に必要な最小情報のみ取り出した型。
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct YTimeExportOperation {
    pub unko_no: String,
    pub crew_role: i32,
    pub departure_at: Option<DateTime<Utc>>,
    pub return_at: Option<DateTime<Utc>>,
    pub r2_key_prefix: Option<String>,
}

/// 全乗務員版で使う乗務員参照 (Refs ohishi-exp/rust-ichibanboshi#205 実装計画 01)。
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct DtakoDriverRef {
    pub driver_id: Uuid,
    pub driver_cd: String,
    pub driver_name: String,
}

/// `YTimeExportOperation` に「どの乗務員の運行か」を添えた型。
/// 全乗務員分を 1 クエリで引くために使う。
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct DtakoDriverOperation {
    pub driver_id: Uuid,
    pub unko_no: String,
    pub crew_role: i32,
    pub departure_at: Option<DateTime<Utc>>,
    pub return_at: Option<DateTime<Utc>>,
    pub r2_key_prefix: Option<String>,
}

/// `has_kudgivt = FALSE` (未 split) の 1 運行 (Refs ohishi-exp/rust-ichibanboshi#205 の 36)。
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct UnsplitOperation {
    pub unko_no: String,
    pub driver_cd: String,
    pub reading_date: NaiveDate,
}

/// `dtako_operations` を期間で列挙する読み取り口。
///
/// **列挙系 4 メソッドの期間条件は共通で「読取日 (`reading_date`) と
/// 運行日 (`operation_date`) の OR」** — どちらかが `[from-1, to+1]` に入れば拾う
/// (Refs ohishi-exp/rust-ichibanboshi#205 の 38)。読取日だけだと月末の運行
/// (読まれるのが翌月上旬) が構造的に落ち、運行日だけだと `operation_date` が NULL の行が
/// 落ちる。実測の根拠は `alc-dtako` 側の `repo::dtako_y_time_export` の module docs 参照。
#[async_trait]
pub trait DtakoYTimeExportRepository: Send + Sync {
    /// `(tenant_id, driver_cd)` から `(driver_id, driver_name)` を返す。
    /// 未存在は `Ok(None)`。
    async fn lookup_driver(
        &self,
        tenant_id: Uuid,
        driver_cd: &str,
    ) -> Result<Option<(Uuid, String)>, sqlx::Error>;

    /// 期間内 (`reading_date` **または** `operation_date` が `[from-1, to+1]` に入る)
    /// の運行を列挙。`has_kudgivt = true` でフィルタ済み。
    async fn list_operations(
        &self,
        tenant_id: Uuid,
        driver_id: Uuid,
        from: NaiveDate,
        to: NaiveDate,
    ) -> Result<Vec<YTimeExportOperation>, sqlx::Error>;

    /// 期間内に `has_kudgivt = TRUE` の運行を持つ乗務員を `driver_cd` 昇順で列挙する。
    /// `after_driver_cd` は排他的下限 (keyset paging)。`limit` 件で打ち切る。
    ///
    /// 全乗務員版 `GET /api/dtako/events` (driver_cd 省略時) 専用。既存の
    /// Y時間 export はこのメソッドを使わない。
    async fn list_drivers_with_operations(
        &self,
        tenant_id: Uuid,
        from: NaiveDate,
        to: NaiveDate,
        after_driver_cd: Option<&str>,
        limit: i64,
    ) -> Result<Vec<DtakoDriverRef>, sqlx::Error>;

    /// 複数乗務員分の運行を 1 クエリで列挙。フィルタ条件は `list_operations` と同じ
    /// (読取日/運行日 の OR を ±1 日、`has_kudgivt = TRUE`、`(driver_id, unko_no)` で dedup)。
    async fn list_operations_for_drivers(
        &self,
        tenant_id: Uuid,
        driver_ids: &[Uuid],
        from: NaiveDate,
        to: NaiveDate,
    ) -> Result<Vec<DtakoDriverOperation>, sqlx::Error>;

    /// 期間内 (読取日/運行日 の OR を ±1 日、他 3 クエリと同じ広げ方) に `has_kudgivt = FALSE` の
    /// 運行を `unko_no` 昇順で列挙する。上限は掛けない — 表示件数の絞り込みと総数の算出は
    /// 呼び出し側 (`unsplit_total` = `len()`) に委ねる。
    async fn list_unsplit_operations(
        &self,
        tenant_id: Uuid,
        from: NaiveDate,
        to: NaiveDate,
    ) -> Result<Vec<UnsplitOperation>, sqlx::Error>;
}
