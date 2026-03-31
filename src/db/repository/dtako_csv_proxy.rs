use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use super::TenantConn;

pub use alc_core::repository::dtako_csv_proxy::*;

pub struct PgDtakoCsvProxyRepository {
    pool: PgPool,
}

impl PgDtakoCsvProxyRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl DtakoCsvProxyRepository for PgDtakoCsvProxyRepository {
    async fn get_r2_key_prefix(
        &self,
        tenant_id: Uuid,
        unko_no: &str,
    ) -> Result<Option<String>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_scalar(
            "SELECT r2_key_prefix FROM alc_api.dtako_operations WHERE tenant_id = $1 AND unko_no = $2 LIMIT 1",
        )
        .bind(tenant_id)
        .bind(unko_no)
        .fetch_optional(&mut *tc.conn)
        .await
    }
}
