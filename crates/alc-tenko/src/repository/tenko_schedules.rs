use async_trait::async_trait;
use uuid::Uuid;

use crate::models::{CreateTenkoSchedule, TenkoSchedule, TenkoScheduleFilter, UpdateTenkoSchedule};

/// Paginated list result
pub struct ScheduleListResult {
    pub schedules: Vec<TenkoSchedule>,
    pub total: i64,
}

#[async_trait]
pub trait TenkoSchedulesRepository: Send + Sync {
    async fn create(
        &self,
        tenant_id: Uuid,
        input: &CreateTenkoSchedule,
    ) -> Result<TenkoSchedule, sqlx::Error>;

    async fn batch_create(
        &self,
        tenant_id: Uuid,
        inputs: &[CreateTenkoSchedule],
    ) -> Result<Vec<TenkoSchedule>, sqlx::Error>;

    async fn list(
        &self,
        tenant_id: Uuid,
        filter: &TenkoScheduleFilter,
        page: i64,
        per_page: i64,
    ) -> Result<ScheduleListResult, sqlx::Error>;

    async fn get(&self, tenant_id: Uuid, id: Uuid) -> Result<Option<TenkoSchedule>, sqlx::Error>;

    async fn update(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        input: &UpdateTenkoSchedule,
    ) -> Result<Option<TenkoSchedule>, sqlx::Error>;

    /// Hard-delete. Returns true if a row was affected.
    async fn delete(&self, tenant_id: Uuid, id: Uuid) -> Result<bool, sqlx::Error>;

    async fn get_pending(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
    ) -> Result<Vec<TenkoSchedule>, sqlx::Error>;
}
