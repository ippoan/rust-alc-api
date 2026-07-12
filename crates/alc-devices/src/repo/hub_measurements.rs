use async_trait::async_trait;
use chrono::DateTime;
use sqlx::PgPool;
use uuid::Uuid;

use alc_core::models::{HubMeasurementCreate, HubMeasurementsIngestResponse};
use alc_core::tenant::TenantConn;

pub use alc_core::repository::hub_measurements::*;

pub struct PgHubMeasurementsRepository {
    pool: PgPool,
}

impl PgHubMeasurementsRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl HubMeasurementsRepository for PgHubMeasurementsRepository {
    async fn insert_batch(
        &self,
        tenant_id: Uuid,
        items: &[HubMeasurementCreate],
    ) -> Result<HubMeasurementsIngestResponse, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let mut inserted: i64 = 0;
        for item in items {
            // recorded_at_ms (端末計時 unix ms) → TIMESTAMPTZ。範囲外は NULL に落とす。
            let recorded_at = item
                .recorded_at_ms
                .and_then(DateTime::from_timestamp_millis);
            let res = sqlx::query(
                r#"
                INSERT INTO hub_measurements (
                    tenant_id, device_id, kind, payload, seq, recorded_at
                )
                VALUES ($1, $2, $3, $4, $5, $6)
                ON CONFLICT (tenant_id, device_id, seq) DO NOTHING
                "#,
            )
            .bind(tenant_id)
            .bind(&item.device_id)
            .bind(&item.kind)
            .bind(&item.payload)
            .bind(item.seq)
            .bind(recorded_at)
            .execute(&mut *tc.conn)
            .await?;
            inserted += res.rows_affected() as i64;
        }
        Ok(HubMeasurementsIngestResponse {
            inserted,
            duplicates: items.len() as i64 - inserted,
        })
    }
}
