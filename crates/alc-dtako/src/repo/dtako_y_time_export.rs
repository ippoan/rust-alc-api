use async_trait::async_trait;
use chrono::NaiveDate;
use sqlx::PgPool;
use uuid::Uuid;

use alc_core::tenant::TenantConn;

pub use alc_core::repository::dtako_y_time_export::*;

pub struct PgDtakoYTimeExportRepository {
    pool: PgPool,
}

impl PgDtakoYTimeExportRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl DtakoYTimeExportRepository for PgDtakoYTimeExportRepository {
    async fn lookup_driver(
        &self,
        tenant_id: Uuid,
        driver_cd: &str,
    ) -> Result<Option<(Uuid, String)>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let row: Option<(Uuid, String)> = sqlx::query_as(
            "SELECT id, name FROM alc_api.employees \
             WHERE tenant_id = $1 AND driver_cd = $2 AND deleted_at IS NULL \
             LIMIT 1",
        )
        .bind(tenant_id)
        .bind(driver_cd)
        .fetch_optional(&mut *tc.conn)
        .await?;
        Ok(row)
    }

    async fn list_operations(
        &self,
        tenant_id: Uuid,
        driver_id: Uuid,
        from: NaiveDate,
        to: NaiveDate,
    ) -> Result<Vec<YTimeExportOperation>, sqlx::Error> {
        // reading_date を 1日広げて取り、暦日をまたぐ運行を取りこぼさない
        let from_widened = from - chrono::Duration::days(1);
        let to_widened = to + chrono::Duration::days(1);
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, YTimeExportOperation>(
            "SELECT unko_no, crew_role, departure_at, return_at, r2_key_prefix \
             FROM alc_api.dtako_operations \
             WHERE tenant_id = $1 \
               AND driver_id = $2 \
               AND reading_date BETWEEN $3 AND $4 \
               AND has_kudgivt = TRUE \
             ORDER BY departure_at NULLS LAST",
        )
        .bind(tenant_id)
        .bind(driver_id)
        .bind(from_widened)
        .bind(to_widened)
        .fetch_all(&mut *tc.conn)
        .await
    }
}
