use async_trait::async_trait;
use chrono::NaiveDate;
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

    async fn delete_by_unko_no(&self, tenant_id: Uuid, unko_no: &str) -> Result<u64, sqlx::Error>;
}
