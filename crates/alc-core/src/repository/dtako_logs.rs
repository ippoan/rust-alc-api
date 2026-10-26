use async_trait::async_trait;
use uuid::Uuid;

use crate::models::DtakologRow;

#[async_trait]
pub trait DtakoLogsRepository: Send + Sync {
    async fn current_list_all(&self, tenant_id: Uuid) -> Result<Vec<DtakologRow>, sqlx::Error>;

    async fn get_date(
        &self,
        tenant_id: Uuid,
        date_time: &str,
        vehicle_cd: Option<i32>,
    ) -> Result<Vec<DtakologRow>, sqlx::Error>;

    async fn current_list_select(
        &self,
        tenant_id: Uuid,
        address_disp_p: Option<&str>,
        branch_cd: Option<i32>,
        vehicle_cds: &[i32],
    ) -> Result<Vec<DtakologRow>, sqlx::Error>;

    async fn get_date_range(
        &self,
        tenant_id: Uuid,
        start: &str,
        end: &str,
        vehicle_cd: Option<i32>,
    ) -> Result<Vec<DtakologRow>, sqlx::Error>;
}
