use async_trait::async_trait;
use uuid::Uuid;

use crate::models::{CreateHealthBaseline, EmployeeHealthBaseline, UpdateHealthBaseline};

#[async_trait]
pub trait HealthBaselinesRepository: Send + Sync {
    async fn upsert(
        &self,
        tenant_id: Uuid,
        body: &CreateHealthBaseline,
    ) -> Result<EmployeeHealthBaseline, sqlx::Error>;

    async fn list(&self, tenant_id: Uuid) -> Result<Vec<EmployeeHealthBaseline>, sqlx::Error>;

    async fn get(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
    ) -> Result<Option<EmployeeHealthBaseline>, sqlx::Error>;

    async fn update(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
        body: &UpdateHealthBaseline,
    ) -> Result<Option<EmployeeHealthBaseline>, sqlx::Error>;

    async fn delete(&self, tenant_id: Uuid, employee_id: Uuid) -> Result<bool, sqlx::Error>;
}
