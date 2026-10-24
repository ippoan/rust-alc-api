use async_trait::async_trait;
use chrono::DateTime;
use sqlx::PgPool;
use uuid::Uuid;

use alc_core::models::{
    HubMeasurement, HubMeasurementCreate, HubMeasurementFilter, HubMeasurementsIngestResponse,
};
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
            // 重複の判定は dev端末の軸 (is_dev) ごと (alc-migrations 155 の 4 列 unique を
            // 推論させる)。is_dev は列の既定値で入る。126 の 3 列 unique は alc-migrations 156 で
            // 落ちたので、同じ端末・同じ連番は軸ごとに 1 行ずつ入る (同じ軸の再送だけが弾かれる)
            let res = sqlx::query(
                r#"
                INSERT INTO hub_measurements (
                    tenant_id, device_id, kind, payload, seq, recorded_at, session_id, via_device_id
                )
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
                ON CONFLICT (tenant_id, device_id, seq, is_dev) DO NOTHING
                "#,
            )
            .bind(tenant_id)
            .bind(&item.device_id)
            .bind(&item.kind)
            .bind(&item.payload)
            .bind(item.seq)
            .bind(recorded_at)
            .bind(&item.session_id)
            .bind(&item.via_device_id)
            .execute(&mut *tc.conn)
            .await?;
            // ON CONFLICT DO NOTHING なので rows_affected() は 0 か 1。
            // 1 = 新規に入った行 (端末の再送は 0 になる)
            inserted += res.rows_affected() as i64;
        }
        Ok(HubMeasurementsIngestResponse {
            inserted,
            duplicates: items.len() as i64 - inserted,
        })
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
        let mut conditions = vec!["h.tenant_id = $1".to_string()];
        let mut idx = 2u32;
        if filter.device_id.is_some() {
            // 機体の鍵の ID で絞ったときは、その鍵に結び付いた hub の ID の行も含める
            // (結び付き = 鍵の ID で入った行の via_device_id。is_dev の軸は跨がない)
            conditions.push(format!(
                "(h.device_id = ${idx} OR h.device_id IN (
                    SELECT l.via_device_id FROM hub_measurements l
                     WHERE l.tenant_id = h.tenant_id AND l.is_dev = h.is_dev
                       AND l.device_id = ${idx} AND l.via_device_id IS NOT NULL))"
            ));
            idx += 1;
        }
        if filter.kind.is_some() {
            conditions.push(format!("h.kind = ${idx}"));
            idx += 1;
        }
        if filter.session_id.is_some() {
            conditions.push(format!("h.session_id = ${idx}"));
            idx += 1;
        }
        if filter.from.is_some() {
            conditions.push(format!("h.created_at >= ${idx}"));
            idx += 1;
        }
        if filter.to.is_some() {
            conditions.push(format!("h.created_at <= ${idx}"));
            idx += 1;
        }
        let where_clause = conditions.join(" AND ");

        // has_more 判定のため 1 件多く引く (COUNT(*) は張らない、models の
        // HubMeasurementsListResponse の doc コメント参照)。
        // machine_id: hub の ID の行は、結び付いた機体の鍵の ID (最後に見えたもの) に畳む
        let sql = format!(
            "SELECT h.id, h.tenant_id, h.device_id, h.kind, h.payload, h.seq, h.session_id,
                    h.via_device_id, COALESCE(link.key_id, h.device_id) AS machine_id,
                    h.recorded_at, h.created_at
               FROM hub_measurements h
               LEFT JOIN LATERAL (
                    SELECT l.device_id AS key_id FROM hub_measurements l
                     WHERE l.tenant_id = h.tenant_id AND l.is_dev = h.is_dev
                       AND l.via_device_id = h.device_id
                     ORDER BY l.created_at DESC LIMIT 1
               ) link ON true
              WHERE {where_clause}
              ORDER BY h.created_at DESC, h.id DESC
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
