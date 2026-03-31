use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use crate::db::models::DtakoEventClassification;

use super::TenantConn;

pub use alc_core::repository::dtako_event_classifications::*;

pub struct PgDtakoEventClassificationsRepository {
    pool: PgPool,
}

impl PgDtakoEventClassificationsRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl DtakoEventClassificationsRepository for PgDtakoEventClassificationsRepository {
    async fn list(&self, tenant_id: Uuid) -> Result<Vec<DtakoEventClassification>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, DtakoEventClassification>(
            "SELECT * FROM alc_api.dtako_event_classifications ORDER BY event_cd",
        )
        .fetch_all(&mut *tc.conn)
        .await
    }

    async fn update(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        classification: &str,
    ) -> Result<Option<DtakoEventClassification>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, DtakoEventClassification>(
            "UPDATE alc_api.dtako_event_classifications SET classification = $1 WHERE id = $2 RETURNING *",
        )
        .bind(classification)
        .bind(id)
        .fetch_optional(&mut *tc.conn)
        .await
    }
}
