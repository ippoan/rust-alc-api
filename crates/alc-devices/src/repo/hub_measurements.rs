use async_trait::async_trait;
use chrono::DateTime;
use sqlx::PgPool;
use uuid::Uuid;

use alc_core::models::{HubMeasurement, HubMeasurementCreate, HubMeasurementFilter};
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
    ) -> Result<Vec<bool>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let mut inserted = Vec::with_capacity(items.len());
        for item in items {
            // recorded_at_ms (端末計時 unix ms) → TIMESTAMPTZ。範囲外は NULL に落とす。
            let recorded_at = item
                .recorded_at_ms
                .and_then(DateTime::from_timestamp_millis);
            let res = sqlx::query(
                r#"
                INSERT INTO hub_measurements (
                    tenant_id, device_id, kind, payload, seq, recorded_at, session_id
                )
                VALUES ($1, $2, $3, $4, $5, $6, $7)
                ON CONFLICT (tenant_id, device_id, seq) DO NOTHING
                "#,
            )
            .bind(tenant_id)
            .bind(&item.device_id)
            .bind(&item.kind)
            .bind(&item.payload)
            .bind(item.seq)
            .bind(recorded_at)
            .bind(&item.session_id)
            .execute(&mut *tc.conn)
            .await?;
            // ON CONFLICT DO NOTHING なので rows_affected() は 0 か 1。
            // 1 = 新規に入った行 (端末の再送は 0 になる)
            inserted.push(res.rows_affected() > 0);
        }
        Ok(inserted)
    }

    async fn list(
        &self,
        tenant_id: Uuid,
        filter: &HubMeasurementFilter,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<HubMeasurement>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;

        // RLS (migration 126 の hub_measurements_tenant) が同じ条件を課すが、
        // `tenant_id = $1` を明示して index hub_measurements_tenant_device
        // (tenant_id, device_id, created_at DESC) の先頭列に乗せる。
        // 二重防御でもある — RLS だけに依存しない (テストで固定、Refs #592)。
        let mut conditions = vec!["tenant_id = $1".to_string()];
        let mut idx = 2u32;
        if filter.device_id.is_some() {
            conditions.push(format!("device_id = ${idx}"));
            idx += 1;
        }
        if filter.kind.is_some() {
            conditions.push(format!("kind = ${idx}"));
            idx += 1;
        }
        if filter.session_id.is_some() {
            conditions.push(format!("session_id = ${idx}"));
            idx += 1;
        }
        if filter.from.is_some() {
            conditions.push(format!("created_at >= ${idx}"));
            idx += 1;
        }
        if filter.to.is_some() {
            conditions.push(format!("created_at <= ${idx}"));
            idx += 1;
        }
        let where_clause = conditions.join(" AND ");

        // has_more 判定のため 1 件多く引く (COUNT(*) は張らない、models の
        // HubMeasurementsListResponse の doc コメント参照)。
        let sql = format!(
            "SELECT id, tenant_id, device_id, kind, payload, seq, session_id, recorded_at, created_at
               FROM hub_measurements
              WHERE {where_clause}
              ORDER BY created_at DESC, id DESC
              LIMIT ${idx} OFFSET ${}",
            idx + 1
        );

        let mut query = sqlx::query_as::<_, HubMeasurement>(&sql).bind(tenant_id);
        if let Some(ref device_id) = filter.device_id {
            query = query.bind(device_id);
        }
        if let Some(ref kind) = filter.kind {
            query = query.bind(kind);
        }
        if let Some(ref session_id) = filter.session_id {
            query = query.bind(session_id);
        }
        if let Some(from) = filter.from {
            query = query.bind(from);
        }
        if let Some(to) = filter.to {
            query = query.bind(to);
        }
        query = query.bind(limit + 1).bind(offset);

        query.fetch_all(&mut *tc.conn).await
    }
}
