use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use crate::db::models::{CreateEmployee, Employee, FaceDataEntry, UpdateEmployee, UpdateFace};

use super::TenantConn;

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

pub struct PgEmployeeRepository {
    pool: PgPool,
}

impl PgEmployeeRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl EmployeeRepository for PgEmployeeRepository {
    async fn create(
        &self,
        tenant_id: Uuid,
        input: &CreateEmployee,
    ) -> Result<Employee, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, Employee>(
            r#"
            INSERT INTO employees (tenant_id, code, nfc_id, name, role)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING *
            "#,
        )
        .bind(tenant_id)
        .bind(&input.code)
        .bind(&input.nfc_id)
        .bind(&input.name)
        .bind(&input.role)
        .fetch_one(&mut *tc.conn)
        .await
    }

    async fn list(&self, tenant_id: Uuid) -> Result<Vec<Employee>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, Employee>(
            "SELECT * FROM employees WHERE tenant_id = $1 AND deleted_at IS NULL ORDER BY name",
        )
        .bind(tenant_id)
        .fetch_all(&mut *tc.conn)
        .await
    }

    async fn get(&self, tenant_id: Uuid, id: Uuid) -> Result<Option<Employee>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, Employee>(
            "SELECT * FROM employees WHERE tenant_id = $1 AND id = $2 AND deleted_at IS NULL",
        )
        .bind(tenant_id)
        .bind(id)
        .fetch_optional(&mut *tc.conn)
        .await
    }

    async fn get_by_nfc(
        &self,
        tenant_id: Uuid,
        nfc_id: &str,
    ) -> Result<Option<Employee>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, Employee>(
            "SELECT * FROM employees WHERE tenant_id = $1 AND nfc_id = $2 AND deleted_at IS NULL",
        )
        .bind(tenant_id)
        .bind(nfc_id)
        .fetch_optional(&mut *tc.conn)
        .await
    }

    async fn get_by_code(
        &self,
        tenant_id: Uuid,
        code: &str,
    ) -> Result<Option<Employee>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, Employee>(
            "SELECT * FROM employees WHERE tenant_id = $1 AND code = $2 AND deleted_at IS NULL",
        )
        .bind(tenant_id)
        .bind(code)
        .fetch_optional(&mut *tc.conn)
        .await
    }

    async fn update(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        input: &UpdateEmployee,
    ) -> Result<Option<Employee>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, Employee>(
            r#"
            UPDATE employees SET name = $1, code = $2, role = COALESCE($5, role), updated_at = NOW()
            WHERE id = $3 AND tenant_id = $4 AND deleted_at IS NULL
            RETURNING *
            "#,
        )
        .bind(&input.name)
        .bind(&input.code)
        .bind(id)
        .bind(tenant_id)
        .bind(&input.role)
        .fetch_optional(&mut *tc.conn)
        .await
    }

    async fn delete(&self, tenant_id: Uuid, id: Uuid) -> Result<bool, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let result = sqlx::query(
            r#"
            UPDATE employees SET deleted_at = NOW(), updated_at = NOW()
            WHERE id = $1 AND tenant_id = $2 AND deleted_at IS NULL
            "#,
        )
        .bind(id)
        .bind(tenant_id)
        .execute(&mut *tc.conn)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn update_face(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        input: &UpdateFace,
    ) -> Result<Option<Employee>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, Employee>(
            r#"
            UPDATE employees SET
                face_photo_url = COALESCE($1, face_photo_url),
                face_embedding = COALESCE($2, face_embedding),
                face_embedding_at = CASE WHEN $2 IS NOT NULL THEN NOW() ELSE face_embedding_at END,
                face_model_version = CASE WHEN $2 IS NOT NULL THEN $5 ELSE face_model_version END,
                face_approval_status = CASE WHEN $2 IS NOT NULL THEN 'pending' ELSE face_approval_status END,
                updated_at = NOW()
            WHERE id = $3 AND tenant_id = $4 AND deleted_at IS NULL
            RETURNING *
            "#,
        )
        .bind(&input.face_photo_url)
        .bind(&input.face_embedding)
        .bind(id)
        .bind(tenant_id)
        .bind(&input.face_model_version)
        .fetch_optional(&mut *tc.conn)
        .await
    }

    async fn list_face_data(&self, tenant_id: Uuid) -> Result<Vec<FaceDataEntry>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, FaceDataEntry>(
            r#"
            SELECT id, face_embedding, face_embedding_at, face_model_version, face_approval_status
            FROM employees
            WHERE tenant_id = $1 AND deleted_at IS NULL AND face_embedding IS NOT NULL
              AND face_approval_status = 'approved'
            "#,
        )
        .bind(tenant_id)
        .fetch_all(&mut *tc.conn)
        .await
    }

    async fn update_license(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        issue_date: Option<chrono::NaiveDate>,
        expiry_date: Option<chrono::NaiveDate>,
    ) -> Result<Option<Employee>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, Employee>(
            r#"
            UPDATE employees SET
                license_issue_date = COALESCE($1, license_issue_date),
                license_expiry_date = COALESCE($2, license_expiry_date),
                updated_at = NOW()
            WHERE id = $3 AND tenant_id = $4 AND deleted_at IS NULL
            RETURNING *
            "#,
        )
        .bind(issue_date)
        .bind(expiry_date)
        .bind(id)
        .bind(tenant_id)
        .fetch_optional(&mut *tc.conn)
        .await
    }

    async fn update_nfc_id(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        nfc_id: &str,
    ) -> Result<Option<Employee>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, Employee>(
            r#"
            UPDATE employees SET nfc_id = $1, updated_at = NOW()
            WHERE id = $2 AND tenant_id = $3
            RETURNING *
            "#,
        )
        .bind(nfc_id)
        .bind(id)
        .bind(tenant_id)
        .fetch_optional(&mut *tc.conn)
        .await
    }

    async fn approve_face(
        &self,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<Option<Employee>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, Employee>(
            r#"
            UPDATE employees SET
                face_approval_status = 'approved',
                face_approved_at = NOW(),
                updated_at = NOW()
            WHERE id = $1 AND tenant_id = $2 AND face_approval_status = 'pending' AND deleted_at IS NULL
            RETURNING *
            "#,
        )
        .bind(id)
        .bind(tenant_id)
        .fetch_optional(&mut *tc.conn)
        .await
    }

    async fn reject_face(
        &self,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<Option<Employee>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, Employee>(
            r#"
            UPDATE employees SET
                face_approval_status = 'rejected',
                updated_at = NOW()
            WHERE id = $1 AND tenant_id = $2 AND face_approval_status = 'pending' AND deleted_at IS NULL
            RETURNING *
            "#,
        )
        .bind(id)
        .bind(tenant_id)
        .fetch_optional(&mut *tc.conn)
        .await
    }
}
