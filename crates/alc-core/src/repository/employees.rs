use async_trait::async_trait;
use uuid::Uuid;

use crate::models::{CreateEmployee, Employee, FaceDataEntry, UpdateEmployee, UpdateFace};

#[async_trait]
pub trait EmployeeRepository: Send + Sync {
    async fn create(
        &self,
        tenant_id: Uuid,
        input: &CreateEmployee,
    ) -> Result<Employee, sqlx::Error>;

    async fn list(&self, tenant_id: Uuid) -> Result<Vec<Employee>, sqlx::Error>;

    async fn get(&self, tenant_id: Uuid, id: Uuid) -> Result<Option<Employee>, sqlx::Error>;

    async fn get_by_nfc(
        &self,
        tenant_id: Uuid,
        nfc_id: &str,
    ) -> Result<Option<Employee>, sqlx::Error>;

    async fn get_by_code(
        &self,
        tenant_id: Uuid,
        code: &str,
    ) -> Result<Option<Employee>, sqlx::Error>;

    async fn update(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        input: &UpdateEmployee,
    ) -> Result<Option<Employee>, sqlx::Error>;

    /// Soft-delete. Returns true if a row was affected.
    async fn delete(&self, tenant_id: Uuid, id: Uuid) -> Result<bool, sqlx::Error>;

    async fn update_face(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        input: &UpdateFace,
    ) -> Result<Option<Employee>, sqlx::Error>;

    async fn list_face_data(&self, tenant_id: Uuid) -> Result<Vec<FaceDataEntry>, sqlx::Error>;

    async fn update_license(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        issue_date: Option<chrono::NaiveDate>,
        expiry_date: Option<chrono::NaiveDate>,
    ) -> Result<Option<Employee>, sqlx::Error>;

    async fn update_nfc_id(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        nfc_id: &str,
    ) -> Result<Option<Employee>, sqlx::Error>;

    async fn approve_face(
        &self,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<Option<Employee>, sqlx::Error>;

    async fn reject_face(&self, tenant_id: Uuid, id: Uuid)
        -> Result<Option<Employee>, sqlx::Error>;
}
