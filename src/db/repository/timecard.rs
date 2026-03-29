use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::db::models::{TimePunch, TimePunchWithDevice, TimecardCard};

use super::TenantConn;

/// CSV エクスポート用の行データ
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct TimePunchCsvRow {
    pub id: Uuid,
    pub punched_at: DateTime<Utc>,
    pub employee_name: String,
    pub employee_code: Option<String>,
    pub device_name: Option<String>,
}

#[async_trait]
pub trait TimecardRepository: Send + Sync {
    async fn create_card(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
        card_id: &str,
        label: Option<&str>,
    ) -> Result<TimecardCard, sqlx::Error>;

    async fn list_cards(
        &self,
        tenant_id: Uuid,
        employee_id: Option<Uuid>,
    ) -> Result<Vec<TimecardCard>, sqlx::Error>;

    async fn get_card(
        &self,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<Option<TimecardCard>, sqlx::Error>;

    async fn get_card_by_card_id(
        &self,
        tenant_id: Uuid,
        card_id: &str,
    ) -> Result<Option<TimecardCard>, sqlx::Error>;

    /// Delete a card. Returns true if a row was affected.
    async fn delete_card(&self, tenant_id: Uuid, id: Uuid) -> Result<bool, sqlx::Error>;

    /// Find a card by card_id (for punch lookup).
    async fn find_card_by_card_id(
        &self,
        tenant_id: Uuid,
        card_id: &str,
    ) -> Result<Option<TimecardCard>, sqlx::Error>;

    /// Find employee by nfc_id (fallback for punch).
    async fn find_employee_id_by_nfc(
        &self,
        tenant_id: Uuid,
        nfc_id: &str,
    ) -> Result<Option<Uuid>, sqlx::Error>;

    /// Create a time punch record.
    async fn create_punch(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
        device_id: Option<Uuid>,
    ) -> Result<TimePunch, sqlx::Error>;

    /// Get employee name by id.
    async fn get_employee_name(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
    ) -> Result<String, sqlx::Error>;

    /// List today's punches for an employee.
    async fn list_today_punches(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
    ) -> Result<Vec<TimePunch>, sqlx::Error>;

    /// Count punches with filters.
    async fn count_punches(
        &self,
        tenant_id: Uuid,
        employee_id: Option<Uuid>,
        date_from: Option<DateTime<Utc>>,
        date_to: Option<DateTime<Utc>>,
    ) -> Result<i64, sqlx::Error>;

    /// List punches with filters, pagination, and JOINed device/employee names.
    async fn list_punches(
        &self,
        tenant_id: Uuid,
        employee_id: Option<Uuid>,
        date_from: Option<DateTime<Utc>>,
        date_to: Option<DateTime<Utc>>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<TimePunchWithDevice>, sqlx::Error>;

    /// List punches for CSV export (with employee code, no pagination).
    async fn list_punches_for_csv(
        &self,
        tenant_id: Uuid,
        employee_id: Option<Uuid>,
        date_from: Option<DateTime<Utc>>,
        date_to: Option<DateTime<Utc>>,
    ) -> Result<Vec<TimePunchCsvRow>, sqlx::Error>;
}

pub struct PgTimecardRepository {
    pool: PgPool,
}

impl PgTimecardRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

/// Build dynamic WHERE clause and bind parameters.
/// Returns (where_clause, param_count_after).
fn build_punch_where(
    employee_id: Option<Uuid>,
    date_from: Option<DateTime<Utc>>,
    date_to: Option<DateTime<Utc>>,
    table_prefix: &str,
) -> (String, u32) {
    let mut conditions = vec![format!("{table_prefix}.tenant_id = $1")];
    let mut param_idx = 2u32;

    if employee_id.is_some() {
        conditions.push(format!("{table_prefix}.employee_id = ${param_idx}"));
        param_idx += 1;
    }
    if date_from.is_some() {
        conditions.push(format!("{table_prefix}.punched_at >= ${param_idx}"));
        param_idx += 1;
    }
    if date_to.is_some() {
        conditions.push(format!("{table_prefix}.punched_at <= ${param_idx}"));
        param_idx += 1;
    }

    (conditions.join(" AND "), param_idx)
}

#[async_trait]
impl TimecardRepository for PgTimecardRepository {
    async fn create_card(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
        card_id: &str,
        label: Option<&str>,
    ) -> Result<TimecardCard, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, TimecardCard>(
            r#"
            INSERT INTO timecard_cards (tenant_id, employee_id, card_id, label)
            VALUES ($1, $2, $3, $4)
            RETURNING *
            "#,
        )
        .bind(tenant_id)
        .bind(employee_id)
        .bind(card_id)
        .bind(label)
        .fetch_one(&mut *tc.conn)
        .await
    }

    async fn list_cards(
        &self,
        tenant_id: Uuid,
        employee_id: Option<Uuid>,
    ) -> Result<Vec<TimecardCard>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        if let Some(eid) = employee_id {
            sqlx::query_as::<_, TimecardCard>(
                "SELECT * FROM timecard_cards WHERE tenant_id = $1 AND employee_id = $2 ORDER BY created_at",
            )
            .bind(tenant_id)
            .bind(eid)
            .fetch_all(&mut *tc.conn)
            .await
        } else {
            sqlx::query_as::<_, TimecardCard>(
                "SELECT * FROM timecard_cards WHERE tenant_id = $1 ORDER BY created_at",
            )
            .bind(tenant_id)
            .fetch_all(&mut *tc.conn)
            .await
        }
    }

    async fn get_card(
        &self,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<Option<TimecardCard>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, TimecardCard>(
            "SELECT * FROM timecard_cards WHERE id = $1 AND tenant_id = $2",
        )
        .bind(id)
        .bind(tenant_id)
        .fetch_optional(&mut *tc.conn)
        .await
    }

    async fn get_card_by_card_id(
        &self,
        tenant_id: Uuid,
        card_id: &str,
    ) -> Result<Option<TimecardCard>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, TimecardCard>(
            "SELECT * FROM timecard_cards WHERE tenant_id = $1 AND card_id = $2",
        )
        .bind(tenant_id)
        .bind(card_id)
        .fetch_optional(&mut *tc.conn)
        .await
    }

    async fn delete_card(&self, tenant_id: Uuid, id: Uuid) -> Result<bool, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let result = sqlx::query("DELETE FROM timecard_cards WHERE id = $1 AND tenant_id = $2")
            .bind(id)
            .bind(tenant_id)
            .execute(&mut *tc.conn)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn find_card_by_card_id(
        &self,
        tenant_id: Uuid,
        card_id: &str,
    ) -> Result<Option<TimecardCard>, sqlx::Error> {
        // Same as get_card_by_card_id — kept as alias for clarity in punch flow
        self.get_card_by_card_id(tenant_id, card_id).await
    }

    async fn find_employee_id_by_nfc(
        &self,
        tenant_id: Uuid,
        nfc_id: &str,
    ) -> Result<Option<Uuid>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_scalar::<_, Uuid>(
            "SELECT id FROM employees WHERE tenant_id = $1 AND nfc_id = $2",
        )
        .bind(tenant_id)
        .bind(nfc_id)
        .fetch_optional(&mut *tc.conn)
        .await
    }

    async fn create_punch(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
        device_id: Option<Uuid>,
    ) -> Result<TimePunch, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, TimePunch>(
            r#"
            INSERT INTO time_punches (tenant_id, employee_id, device_id)
            VALUES ($1, $2, $3)
            RETURNING *
            "#,
        )
        .bind(tenant_id)
        .bind(employee_id)
        .bind(device_id)
        .fetch_one(&mut *tc.conn)
        .await
    }

    async fn get_employee_name(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
    ) -> Result<String, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_scalar("SELECT name FROM employees WHERE id = $1 AND tenant_id = $2")
            .bind(employee_id)
            .bind(tenant_id)
            .fetch_one(&mut *tc.conn)
            .await
    }

    async fn list_today_punches(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
    ) -> Result<Vec<TimePunch>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, TimePunch>(
            r#"
            SELECT * FROM time_punches
            WHERE tenant_id = $1 AND employee_id = $2
              AND punched_at >= CURRENT_DATE
            ORDER BY punched_at
            "#,
        )
        .bind(tenant_id)
        .bind(employee_id)
        .fetch_all(&mut *tc.conn)
        .await
    }

    async fn count_punches(
        &self,
        tenant_id: Uuid,
        employee_id: Option<Uuid>,
        date_from: Option<DateTime<Utc>>,
        date_to: Option<DateTime<Utc>>,
    ) -> Result<i64, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let (where_clause, _) = build_punch_where(employee_id, date_from, date_to, "tp");
        let count_sql = format!("SELECT COUNT(*) FROM time_punches tp WHERE {where_clause}");

        let mut query = sqlx::query_scalar::<_, i64>(&count_sql).bind(tenant_id);
        if let Some(eid) = employee_id {
            query = query.bind(eid);
        }
        if let Some(df) = date_from {
            query = query.bind(df);
        }
        if let Some(dt) = date_to {
            query = query.bind(dt);
        }
        query.fetch_one(&mut *tc.conn).await
    }

    async fn list_punches(
        &self,
        tenant_id: Uuid,
        employee_id: Option<Uuid>,
        date_from: Option<DateTime<Utc>>,
        date_to: Option<DateTime<Utc>>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<TimePunchWithDevice>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let (where_clause, param_idx) = build_punch_where(employee_id, date_from, date_to, "tp");

        let sql = format!(
            r#"SELECT tp.id, tp.tenant_id, tp.employee_id, tp.device_id, d.device_name,
                      e.name as employee_name, tp.punched_at, tp.created_at
               FROM time_punches tp
               LEFT JOIN devices d ON d.id = tp.device_id
               LEFT JOIN employees e ON e.id = tp.employee_id
               WHERE {where_clause}
               ORDER BY tp.punched_at DESC LIMIT ${param_idx} OFFSET ${}"#,
            param_idx + 1
        );

        let mut query = sqlx::query_as::<_, TimePunchWithDevice>(&sql).bind(tenant_id);
        if let Some(eid) = employee_id {
            query = query.bind(eid);
        }
        if let Some(df) = date_from {
            query = query.bind(df);
        }
        if let Some(dt) = date_to {
            query = query.bind(dt);
        }
        query = query.bind(limit).bind(offset);

        query.fetch_all(&mut *tc.conn).await
    }

    async fn list_punches_for_csv(
        &self,
        tenant_id: Uuid,
        employee_id: Option<Uuid>,
        date_from: Option<DateTime<Utc>>,
        date_to: Option<DateTime<Utc>>,
    ) -> Result<Vec<TimePunchCsvRow>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let (where_clause, _) = build_punch_where(employee_id, date_from, date_to, "tp");

        let sql = format!(
            r#"
            SELECT tp.id, tp.punched_at, e.name as employee_name, e.code as employee_code,
                   d.device_name
            FROM time_punches tp
            JOIN employees e ON e.id = tp.employee_id
            LEFT JOIN devices d ON d.id = tp.device_id
            WHERE {where_clause}
            ORDER BY tp.punched_at DESC
            "#
        );

        let mut query = sqlx::query_as::<_, TimePunchCsvRow>(&sql).bind(tenant_id);
        if let Some(eid) = employee_id {
            query = query.bind(eid);
        }
        if let Some(df) = date_from {
            query = query.bind(df);
        }
        if let Some(dt) = date_to {
            query = query.bind(dt);
        }

        query.fetch_all(&mut *tc.conn).await
    }
}
