use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use alc_core::models::{CreateTroubleOffice, TroubleOffice};
use alc_core::tenant::TenantConn;

pub use alc_core::repository::trouble_offices::*;

pub struct PgTroubleOfficesRepository {
    pool: PgPool,
}

impl PgTroubleOfficesRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl TroubleOfficesRepository for PgTroubleOfficesRepository {
    async fn list(&self, tenant_id: Uuid) -> Result<Vec<TroubleOffice>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, TroubleOffice>(
            "SELECT * FROM trouble_offices WHERE tenant_id = $1 ORDER BY sort_order, name",
        )
        .bind(tenant_id)
        .fetch_all(&mut *tc.conn)
        .await
    }

    async fn create(
        &self,
        tenant_id: Uuid,
        input: &CreateTroubleOffice,
    ) -> Result<TroubleOffice, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, TroubleOffice>(
            r#"INSERT INTO trouble_offices (tenant_id, name, sort_order)
            VALUES ($1, $2, $3)
            RETURNING *"#,
        )
        .bind(tenant_id)
        .bind(&input.name)
        .bind(input.sort_order.unwrap_or(0))
        .fetch_one(&mut *tc.conn)
        .await
    }

    async fn delete(&self, tenant_id: Uuid, id: Uuid) -> Result<bool, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let result = sqlx::query("DELETE FROM trouble_offices WHERE id = $1 AND tenant_id = $2")
            .bind(id)
            .bind(tenant_id)
            .execute(&mut *tc.conn)
            .await?;
        Ok(result.rows_affected() > 0)
    }
}
