use async_trait::async_trait;
use rand::RngCore;
use sqlx::PgPool;
use uuid::Uuid;

use alc_core::models::{
    DtakoTicket, DtakoTicketCreate, DtakoTicketFilter, DtakoTicketScrapedPatch,
    DtakoTicketsResponse,
};
use alc_core::tenant::TenantConn;

pub use alc_core::repository::dtako_tickets::*;

pub struct PgDtakoTicketsRepository {
    pool: PgPool,
}

impl PgDtakoTicketsRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// 推測不可な URL-safe close token (32 byte → 64 hex chars) を生成。
    pub(crate) fn generate_close_token() -> String {
        let mut buf = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut buf);
        buf.iter().map(|b| format!("{b:02x}")).collect()
    }
}

#[async_trait]
impl DtakoTicketsRepository for PgDtakoTicketsRepository {
    async fn create(
        &self,
        tenant_id: Uuid,
        input: &DtakoTicketCreate,
    ) -> Result<DtakoTicket, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let close_token = Self::generate_close_token();
        sqlx::query_as::<_, DtakoTicket>(
            r#"
            INSERT INTO dtako_tickets (
                tenant_id, source,
                source_email_subject, source_email_from, source_email_message_id,
                source_email_received_at,
                vehicle_name, vehicle_code, error_kind,
                raw_email_text, close_token
            )
            VALUES (
                $1, $2,
                $3, $4, $5,
                $6,
                $7, $8, $9,
                $10, $11
            )
            RETURNING
                id, tenant_id, source,
                source_email_subject, source_email_from, source_email_message_id,
                source_email_received_at,
                vehicle_name, vehicle_code, error_kind, status,
                comp_id, unko_no,
                operation_started_at, operation_ended_at,
                scraped_payload, settings_zip_r2_key,
                close_token, closed_at, closed_by,
                raw_email_text, created_at, updated_at
            "#,
        )
        .bind(tenant_id)
        .bind(&input.source)
        .bind(input.source_email_subject.as_deref())
        .bind(input.source_email_from.as_deref())
        .bind(input.source_email_message_id.as_deref())
        .bind(input.source_email_received_at)
        .bind(&input.vehicle_name)
        .bind(input.vehicle_code.as_deref())
        .bind(&input.error_kind)
        .bind(input.raw_email_text.as_deref())
        .bind(&close_token)
        .fetch_one(&mut *tc.conn)
        .await
    }

    async fn patch_scraped(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        input: &DtakoTicketScrapedPatch,
    ) -> Result<Option<DtakoTicket>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, DtakoTicket>(
            r#"
            UPDATE dtako_tickets
               SET comp_id              = COALESCE($3, comp_id),
                   unko_no              = COALESCE($4, unko_no),
                   operation_started_at = COALESCE($5, operation_started_at),
                   operation_ended_at   = COALESCE($6, operation_ended_at),
                   settings_zip_r2_key  = COALESCE($7, settings_zip_r2_key),
                   scraped_payload      = COALESCE($8, scraped_payload),
                   status               = 'scraped',
                   updated_at           = now()
             WHERE tenant_id = $1
               AND id        = $2
            RETURNING
                id, tenant_id, source,
                source_email_subject, source_email_from, source_email_message_id,
                source_email_received_at,
                vehicle_name, vehicle_code, error_kind, status,
                comp_id, unko_no,
                operation_started_at, operation_ended_at,
                scraped_payload, settings_zip_r2_key,
                close_token, closed_at, closed_by,
                raw_email_text, created_at, updated_at
            "#,
        )
        .bind(tenant_id)
        .bind(id)
        .bind(input.comp_id.as_deref())
        .bind(input.unko_no.as_deref())
        .bind(input.operation_started_at)
        .bind(input.operation_ended_at)
        .bind(input.settings_zip_r2_key.as_deref())
        .bind(input.scraped_payload.as_ref())
        .fetch_optional(&mut *tc.conn)
        .await
    }

    async fn list(
        &self,
        tenant_id: Uuid,
        filter: &DtakoTicketFilter,
    ) -> Result<DtakoTicketsResponse, sqlx::Error> {
        let per_page = filter.per_page.unwrap_or(50).clamp(1, 1000);
        let page = filter.page.unwrap_or(1).max(1);
        let offset = (page - 1) * per_page;

        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;

        let mut where_clauses = vec!["tenant_id = $1".to_string()];
        let mut idx = 2u32;
        if filter.status.is_some() {
            where_clauses.push(format!("status = ${idx}"));
            idx += 1;
        }
        if filter.vehicle_name.is_some() {
            where_clauses.push(format!("vehicle_name ILIKE '%' || ${idx} || '%'"));
            idx += 1;
        }
        let where_sql = where_clauses.join(" AND ");

        let count_sql = format!("SELECT COUNT(*) FROM dtako_tickets WHERE {where_sql}");
        let list_sql = format!(
            r#"SELECT
                id, tenant_id, source,
                source_email_subject, source_email_from, source_email_message_id,
                source_email_received_at,
                vehicle_name, vehicle_code, error_kind, status,
                comp_id, unko_no,
                operation_started_at, operation_ended_at,
                scraped_payload, settings_zip_r2_key,
                close_token, closed_at, closed_by,
                raw_email_text, created_at, updated_at
            FROM dtako_tickets
            WHERE {where_sql}
            ORDER BY source_email_received_at DESC
            LIMIT ${idx} OFFSET ${}"#,
            idx + 1
        );

        let mut count_q = sqlx::query_scalar::<_, i64>(&count_sql).bind(tenant_id);
        let mut list_q = sqlx::query_as::<_, DtakoTicket>(&list_sql).bind(tenant_id);
        if let Some(ref v) = filter.status {
            count_q = count_q.bind(v);
            list_q = list_q.bind(v);
        }
        if let Some(ref v) = filter.vehicle_name {
            count_q = count_q.bind(v);
            list_q = list_q.bind(v);
        }
        list_q = list_q.bind(per_page).bind(offset);

        let total = count_q.fetch_one(&mut *tc.conn).await?;
        let tickets = list_q.fetch_all(&mut *tc.conn).await?;
        Ok(DtakoTicketsResponse {
            tickets,
            total,
            page,
            per_page,
        })
    }

    async fn get(&self, tenant_id: Uuid, id: Uuid) -> Result<Option<DtakoTicket>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, DtakoTicket>(
            r#"SELECT
                id, tenant_id, source,
                source_email_subject, source_email_from, source_email_message_id,
                source_email_received_at,
                vehicle_name, vehicle_code, error_kind, status,
                comp_id, unko_no,
                operation_started_at, operation_ended_at,
                scraped_payload, settings_zip_r2_key,
                close_token, closed_at, closed_by,
                raw_email_text, created_at, updated_at
            FROM dtako_tickets
            WHERE tenant_id = $1 AND id = $2"#,
        )
        .bind(tenant_id)
        .bind(id)
        .fetch_optional(&mut *tc.conn)
        .await
    }

    async fn close_by_token(
        &self,
        close_token: &str,
        closed_by: Option<&str>,
    ) -> Result<Option<Uuid>, sqlx::Error> {
        // SECURITY DEFINER 関数経由なので RLS の tenant set は不要。
        // pool から直接 acquire する。
        sqlx::query_scalar::<_, Option<Uuid>>("SELECT close_dtako_ticket_by_token($1, $2)")
            .bind(close_token)
            .bind(closed_by)
            .fetch_one(&self.pool)
            .await
    }
}
