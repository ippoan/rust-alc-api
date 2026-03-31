use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use crate::db::models::DtakoVehicle;

use super::TenantConn;

pub use alc_core::repository::dtako_vehicles::*;

pub struct PgDtakoVehiclesRepository {
    pool: PgPool,
}

impl PgDtakoVehiclesRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl DtakoVehiclesRepository for PgDtakoVehiclesRepository {
    async fn list(&self, tenant_id: Uuid) -> Result<Vec<DtakoVehicle>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, DtakoVehicle>(
            "SELECT * FROM alc_api.dtako_vehicles ORDER BY vehicle_cd",
        )
        .fetch_all(&mut *tc.conn)
        .await
    }
}
