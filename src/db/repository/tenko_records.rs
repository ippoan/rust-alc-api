use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use crate::db::models::{TenkoRecord, TenkoRecordFilter};

use super::TenantConn;

pub use alc_core::repository::tenko_records::*;

/// Helper: build WHERE clause + bind params dynamically
struct FilterQuery {
    conditions: Vec<String>,
    param_idx: u32,
}

impl FilterQuery {
    fn new(filter: &TenkoRecordFilter) -> Self {
        let mut conditions = vec!["r.tenant_id = $1".to_string()];
        let mut param_idx = 2u32;

        if filter.employee_id.is_some() {
            conditions.push(format!("r.employee_id = ${param_idx}"));
            param_idx += 1;
        }
        if filter.tenko_type.is_some() {
            conditions.push(format!("r.tenko_type = ${param_idx}"));
            param_idx += 1;
        }
        if filter.status.is_some() {
            conditions.push(format!("r.status = ${param_idx}"));
            param_idx += 1;
        }
        if filter.date_from.is_some() {
            conditions.push(format!("r.recorded_at >= ${param_idx}"));
            param_idx += 1;
        }
        if filter.date_to.is_some() {
            conditions.push(format!("r.recorded_at <= ${param_idx}"));
            param_idx += 1;
        }

        Self {
            conditions,
            param_idx,
        }
    }

    fn where_clause(&self) -> String {
        self.conditions.join(" AND ")
    }

    fn bind_filter<'q, O>(
        query: sqlx::query::QueryAs<'q, sqlx::Postgres, O, sqlx::postgres::PgArguments>,
        tenant_id: Uuid,
        filter: &'q TenkoRecordFilter,
    ) -> sqlx::query::QueryAs<'q, sqlx::Postgres, O, sqlx::postgres::PgArguments> {
        let mut q = query.bind(tenant_id);
        if let Some(employee_id) = filter.employee_id {
            q = q.bind(employee_id);
        }
        if let Some(ref tenko_type) = filter.tenko_type {
            q = q.bind(tenko_type);
        }
        if let Some(ref status) = filter.status {
            q = q.bind(status);
        }
        if let Some(date_from) = filter.date_from {
            q = q.bind(date_from);
        }
        if let Some(date_to) = filter.date_to {
            q = q.bind(date_to);
        }
        q
    }

    fn bind_filter_scalar<'q>(
        query: sqlx::query::QueryScalar<'q, sqlx::Postgres, i64, sqlx::postgres::PgArguments>,
        tenant_id: Uuid,
        filter: &'q TenkoRecordFilter,
    ) -> sqlx::query::QueryScalar<'q, sqlx::Postgres, i64, sqlx::postgres::PgArguments> {
        let mut q = query.bind(tenant_id);
        if let Some(employee_id) = filter.employee_id {
            q = q.bind(employee_id);
        }
        if let Some(ref tenko_type) = filter.tenko_type {
            q = q.bind(tenko_type);
        }
        if let Some(ref status) = filter.status {
            q = q.bind(status);
        }
        if let Some(date_from) = filter.date_from {
            q = q.bind(date_from);
        }
        if let Some(date_to) = filter.date_to {
            q = q.bind(date_to);
        }
        q
    }
}

pub struct PgTenkoRecordsRepository {
    pool: PgPool,
}

impl PgTenkoRecordsRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl TenkoRecordsRepository for PgTenkoRecordsRepository {
    async fn count(&self, tenant_id: Uuid, filter: &TenkoRecordFilter) -> Result<i64, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let fq = FilterQuery::new(filter);
        let sql = format!(
            "SELECT COUNT(*) FROM tenko_records r WHERE {}",
            fq.where_clause()
        );
        let query = sqlx::query_scalar::<_, i64>(&sql);
        let query = FilterQuery::bind_filter_scalar(query, tenant_id, filter);
        query.fetch_one(&mut *tc.conn).await
    }

    async fn list(
        &self,
        tenant_id: Uuid,
        filter: &TenkoRecordFilter,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<TenkoRecord>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let fq = FilterQuery::new(filter);
        let sql = format!(
            "SELECT r.* FROM tenko_records r WHERE {} ORDER BY r.recorded_at DESC LIMIT ${} OFFSET ${}",
            fq.where_clause(),
            fq.param_idx,
            fq.param_idx + 1
        );
        let query = sqlx::query_as::<_, TenkoRecord>(&sql);
        let query = FilterQuery::bind_filter(query, tenant_id, filter);
        let query = query.bind(limit).bind(offset);
        query.fetch_all(&mut *tc.conn).await
    }

    async fn get(&self, tenant_id: Uuid, id: Uuid) -> Result<Option<TenkoRecord>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, TenkoRecord>(
            "SELECT * FROM tenko_records WHERE id = $1 AND tenant_id = $2",
        )
        .bind(id)
        .bind(tenant_id)
        .fetch_optional(&mut *tc.conn)
        .await
    }

    async fn list_all(
        &self,
        tenant_id: Uuid,
        filter: &TenkoRecordFilter,
    ) -> Result<Vec<TenkoRecord>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let fq = FilterQuery::new(filter);
        let sql = format!(
            "SELECT r.* FROM tenko_records r WHERE {} ORDER BY r.recorded_at DESC",
            fq.where_clause()
        );
        let query = sqlx::query_as::<_, TenkoRecord>(&sql);
        let query = FilterQuery::bind_filter(query, tenant_id, filter);
        query.fetch_all(&mut *tc.conn).await
    }
}
