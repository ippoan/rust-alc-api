use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use alc_core::models::{
    DtakoOperation, DtakoOperationFilter, DtakoOperationListItem, DtakoOperationsResponse,
};

use alc_core::tenant::TenantConn;

pub use alc_core::repository::dtako_operations::*;

pub struct PgDtakoOperationsRepository {
    pool: PgPool,
}

impl PgDtakoOperationsRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl DtakoOperationsRepository for PgDtakoOperationsRepository {
    async fn calendar_dates(
        &self,
        tenant_id: Uuid,
        date_from: NaiveDate,
        date_to: NaiveDate,
    ) -> Result<Vec<(NaiveDate, i64)>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, (NaiveDate, i64)>(
            r#"SELECT reading_date, COUNT(*)::BIGINT
               FROM alc_api.dtako_operations
               WHERE reading_date >= $1 AND reading_date <= $2
               GROUP BY reading_date
               ORDER BY reading_date"#,
        )
        .bind(date_from)
        .bind(date_to)
        .fetch_all(&mut *tc.conn)
        .await
    }

    async fn list(
        &self,
        tenant_id: Uuid,
        filter: &DtakoOperationFilter,
    ) -> Result<DtakoOperationsResponse, sqlx::Error> {
        let page = filter.page.unwrap_or(1).max(1);
        let per_page = filter.per_page.unwrap_or(50).min(200);
        let offset = (page - 1) * per_page;

        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;

        let total: (i64,) = sqlx::query_as(
            r#"SELECT COUNT(*)::BIGINT FROM alc_api.dtako_operations o
               LEFT JOIN alc_api.employees d ON o.driver_id = d.id
               LEFT JOIN alc_api.dtako_vehicles v ON o.vehicle_id = v.id
               WHERE ($1::DATE IS NULL OR o.reading_date >= $1)
                 AND ($2::DATE IS NULL OR o.reading_date <= $2)
                 AND ($3::TEXT IS NULL OR d.driver_cd = $3)
                 AND ($4::TEXT IS NULL OR v.vehicle_cd = $4)"#,
        )
        .bind(filter.date_from)
        .bind(filter.date_to)
        .bind(&filter.driver_cd)
        .bind(&filter.vehicle_cd)
        .fetch_one(&mut *tc.conn)
        .await?;

        let operations = sqlx::query_as::<_, DtakoOperationListItem>(
            r#"SELECT o.id, o.unko_no, o.crew_role, o.reading_date, o.operation_date,
                      d.name AS driver_name, v.vehicle_name, v.vehicle_cd,
                      o.total_distance, o.safety_score, o.economy_score, o.total_score,
                      o.has_kudgivt
               FROM alc_api.dtako_operations o
               LEFT JOIN alc_api.employees d ON o.driver_id = d.id
               LEFT JOIN alc_api.dtako_vehicles v ON o.vehicle_id = v.id
               WHERE ($1::DATE IS NULL OR o.reading_date >= $1)
                 AND ($2::DATE IS NULL OR o.reading_date <= $2)
                 AND ($3::TEXT IS NULL OR d.driver_cd = $3)
                 AND ($4::TEXT IS NULL OR v.vehicle_cd = $4)
               ORDER BY o.reading_date DESC, o.unko_no
               LIMIT $5 OFFSET $6"#,
        )
        .bind(filter.date_from)
        .bind(filter.date_to)
        .bind(&filter.driver_cd)
        .bind(&filter.vehicle_cd)
        .bind(per_page)
        .bind(offset)
        .fetch_all(&mut *tc.conn)
        .await?;

        Ok(DtakoOperationsResponse {
            operations,
            total: total.0,
            page,
            per_page,
        })
    }

    async fn get_by_unko_no(
        &self,
        tenant_id: Uuid,
        unko_no: &str,
    ) -> Result<Vec<DtakoOperation>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, DtakoOperation>(
            "SELECT * FROM alc_api.dtako_operations WHERE unko_no = $1 ORDER BY crew_role",
        )
        .bind(unko_no)
        .fetch_all(&mut *tc.conn)
        .await
    }

    async fn delete_by_unko_no(&self, tenant_id: Uuid, unko_no: &str) -> Result<u64, sqlx::Error> {
        use crate::repo::dtako_operation_changes::{fetch_snapshots, insert_change, NewChange};

        let mut tx = self.pool.begin().await?;
        alc_core::tenant::set_current_tenant(&mut tx, &tenant_id.to_string()).await?;

        // crew_role をまとめて消すので、旧行は crew_role ごとに読んで 1 行ずつ残す
        let before = fetch_snapshots(&mut tx, tenant_id, unko_no, None).await?;
        let result = sqlx::query(
            "DELETE FROM alc_api.dtako_operations WHERE tenant_id = $1 AND unko_no = $2",
        )
        .bind(tenant_id)
        .bind(unko_no)
        .execute(&mut *tx)
        .await?;
        for (crew_role, snapshot) in &before {
            insert_change(
                &mut tx,
                &NewChange {
                    tenant_id,
                    unko_no,
                    crew_role: *crew_role,
                    upload_id: None,
                    reason: "manual_delete",
                    before: Some(snapshot),
                    after: None,
                },
            )
            .await?;
        }
        tx.commit().await?;
        Ok(result.rows_affected())
    }

    async fn list_operation_changes(
        &self,
        tenant_id: Uuid,
        driver_cd: &str,
    ) -> Result<Vec<OperationChangeRow>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, OperationChangeRow>(
            r#"SELECT unko_no, crew_role, recorded_at, reason, before, after
                 FROM alc_api.dtako_operation_changes
                WHERE tenant_id = $1
                  AND (driver_cd = $2 OR before->>'driver_cd' = $2 OR after->>'driver_cd' = $2)
                ORDER BY unko_no, crew_role, recorded_at"#,
        )
        .bind(tenant_id)
        .bind(driver_cd)
        .fetch_all(&mut *tc.conn)
        .await
    }

    async fn operation_changes_recording_since(
        &self,
        tenant_id: Uuid,
    ) -> Result<Option<DateTime<Utc>>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let (since,): (Option<DateTime<Utc>>,) = sqlx::query_as(
            "SELECT MIN(recorded_at) FROM alc_api.dtako_operation_changes WHERE tenant_id = $1",
        )
        .bind(tenant_id)
        .fetch_one(&mut *tc.conn)
        .await?;
        Ok(since)
    }
}
