use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use uuid::Uuid;

use crate::models::{DtakoOperation, DtakoOperationFilter, DtakoOperationsResponse};

#[async_trait]
pub trait DtakoOperationsRepository: Send + Sync {
    async fn calendar_dates(
        &self,
        tenant_id: Uuid,
        date_from: NaiveDate,
        date_to: NaiveDate,
    ) -> Result<Vec<(NaiveDate, i64)>, sqlx::Error>;

    async fn list(
        &self,
        tenant_id: Uuid,
        filter: &DtakoOperationFilter,
    ) -> Result<DtakoOperationsResponse, sqlx::Error>;

    async fn get_by_unko_no(
        &self,
        tenant_id: Uuid,
        unko_no: &str,
    ) -> Result<Vec<DtakoOperation>, sqlx::Error>;

    /// 運行を crew_role ごとまとめて消す。消す前に旧行を読み、crew_role ごとに
    /// `dtako_operation_changes` へ reason='manual_delete'・after=NULL で残す
    /// (消すのと記録は同じトランザクション)。
    async fn delete_by_unko_no(&self, tenant_id: Uuid, unko_no: &str) -> Result<u64, sqlx::Error>;

    /// `driver_cd` が記録の driver_cd 列または before/after の driver_cd に一致する変更記録。
    /// 運行の日付での絞り込みは呼び手が行う (unko_no の先頭 6 桁が日付のため)。
    async fn list_operation_changes(
        &self,
        tenant_id: Uuid,
        driver_cd: &str,
    ) -> Result<Vec<OperationChangeRow>, sqlx::Error>;

    /// 記録の最古 recorded_at (= 記録を始めた時点)。1 行も無ければ `None`。
    async fn operation_changes_recording_since(
        &self,
        tenant_id: Uuid,
    ) -> Result<Option<DateTime<Utc>>, sqlx::Error>;
}

/// `dtako_operation_changes` の 1 行。
#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct OperationChangeRow {
    pub unko_no: String,
    pub crew_role: i32,
    pub recorded_at: DateTime<Utc>,
    pub reason: String,
    pub before: Option<serde_json::Value>,
    pub after: Option<serde_json::Value>,
}
