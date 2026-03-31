use async_trait::async_trait;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PdfDriver {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub driver_cd: Option<String>,
    pub driver_name: String,
}

#[async_trait]
pub trait DtakoRestraintReportPdfRepository: Send + Sync {
    async fn list_drivers(&self, tenant_id: Uuid) -> Result<Vec<PdfDriver>, sqlx::Error>;

    async fn get_driver(
        &self,
        tenant_id: Uuid,
        driver_id: Uuid,
    ) -> Result<Vec<PdfDriver>, sqlx::Error>;
}
