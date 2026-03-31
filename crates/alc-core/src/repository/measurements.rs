use async_trait::async_trait;
use uuid::Uuid;

use crate::models::{
    CreateMeasurement, Measurement, MeasurementFilter, StartMeasurement, UpdateMeasurement,
};

/// Paginated list result (internal, before wrapping in MeasurementsResponse)
pub struct ListResult {
    pub measurements: Vec<Measurement>,
    pub total: i64,
}

#[async_trait]
pub trait MeasurementsRepository: Send + Sync {
    async fn start(
        &self,
        tenant_id: Uuid,
        input: &StartMeasurement,
    ) -> Result<Measurement, sqlx::Error>;

    async fn create(
        &self,
        tenant_id: Uuid,
        input: &CreateMeasurement,
    ) -> Result<Measurement, sqlx::Error>;

    async fn update(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        input: &UpdateMeasurement,
    ) -> Result<Option<Measurement>, sqlx::Error>;

    async fn get(&self, tenant_id: Uuid, id: Uuid) -> Result<Option<Measurement>, sqlx::Error>;

    async fn list(
        &self,
        tenant_id: Uuid,
        filter: &MeasurementFilter,
        page: i64,
        per_page: i64,
    ) -> Result<ListResult, sqlx::Error>;
}
