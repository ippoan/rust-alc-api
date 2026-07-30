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

#[async_trait]
pub trait DtakoYTimeExportRepository: Send + Sync {
    /// `(tenant_id, driver_cd)` から `(driver_id, driver_name)` を返す。
    /// 未存在は `Ok(None)`。
    async fn lookup_driver(
        &self,
        tenant_id: Uuid,
        driver_cd: &str,
    ) -> Result<Option<(Uuid, String)>, sqlx::Error>;

    /// 期間内 (`reading_date` が `[from-1, to+1]` と重なる) の運行を列挙。
    /// `has_kudgivt = true` でフィルタ済み。
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
    /// (`reading_date` ±1 日、`has_kudgivt = TRUE`、`(driver_id, unko_no)` で dedup)。
    async fn list_operations_for_drivers(
        &self,
        tenant_id: Uuid,
        driver_ids: &[Uuid],
        from: NaiveDate,
        to: NaiveDate,
    ) -> Result<Vec<DtakoDriverOperation>, sqlx::Error>;
}
