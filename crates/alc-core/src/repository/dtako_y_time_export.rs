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
}
