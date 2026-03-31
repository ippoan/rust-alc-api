use async_trait::async_trait;
use chrono::NaiveDate;
use sqlx::PgPool;
use uuid::Uuid;

use super::TenantConn;

pub use alc_core::repository::dtako_scraper::*;

pub struct PgDtakoScraperRepository {
    pool: PgPool,
}

impl PgDtakoScraperRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl DtakoScraperRepository for PgDtakoScraperRepository {
    async fn insert_scrape_history(
        &self,
        tenant_id: Uuid,
        target_date: NaiveDate,
        comp_id: &str,
        status: &str,
        message: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query(
            r#"INSERT INTO alc_api.dtako_scrape_history (tenant_id, target_date, comp_id, status, message)
               VALUES ($1, $2, $3, $4, $5)"#,
        )
        .bind(tenant_id)
        .bind(target_date)
        .bind(comp_id)
        .bind(status)
        .bind(message)
        .execute(&mut *tc.conn)
        .await?;
        Ok(())
    }

    async fn list_scrape_history(
        &self,
        tenant_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<ScrapeHistoryItem>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, ScrapeHistoryItem>(
            r#"SELECT id, target_date, comp_id, status, message, created_at
               FROM alc_api.dtako_scrape_history
               WHERE tenant_id = $1
               ORDER BY created_at DESC
               LIMIT $2 OFFSET $3"#,
        )
        .bind(tenant_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&mut *tc.conn)
        .await
    }
}
