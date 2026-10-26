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

/// `dtako_operations` を期間で列挙する読み取り口。
///
/// **列挙 (`list_operations`) の期間条件は「読取日 (`reading_date`) と
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
}
