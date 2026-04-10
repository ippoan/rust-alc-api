use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use alc_core::models::{
    CreateTroubleTicket, TroubleTicket, TroubleTicketFilter, TroubleTicketsResponse,
    UpdateTroubleTicket,
};
use alc_core::tenant::TenantConn;

pub use alc_core::repository::trouble_tickets::*;

pub struct PgTroubleTicketsRepository {
    pool: PgPool,
}

impl PgTroubleTicketsRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl TroubleTicketsRepository for PgTroubleTicketsRepository {
    async fn create(
        &self,
        tenant_id: Uuid,
        input: &CreateTroubleTicket,
        created_by: Option<Uuid>,
        initial_status_id: Option<Uuid>,
    ) -> Result<TroubleTicket, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, TroubleTicket>(
            r#"
            INSERT INTO trouble_tickets (
                tenant_id, category, title,
                occurred_at, occurred_date,
                company_name, office_name, department,
                person_name, person_id, vehicle_number,
                location, description,
                status_id, assigned_to,
                damage_amount, compensation_amount, road_service_cost,
                counterparty, counterparty_insurance,
                custom_fields, due_date, created_by
            )
            VALUES (
                $1, $2, COALESCE($3, ''),
                $4, $5,
                COALESCE($6, ''), COALESCE($7, ''), COALESCE($8, ''),
                COALESCE($9, ''), $10, COALESCE($11, ''),
                COALESCE($12, ''), COALESCE($13, ''),
                $14, $15,
                $16, $17, $18,
                COALESCE($19, ''), COALESCE($20, ''),
                COALESCE($21, '{}'::jsonb), $22, $23
            )
            RETURNING id, tenant_id, ticket_no, category, title,
                occurred_at, occurred_date,
                company_name, office_name, department,
                person_name, person_id, vehicle_number,
                location, description,
                status_id, assigned_to,
                progress_notes, allowance,
                damage_amount::text, compensation_amount::text,
                confirmation_notice, disciplinary_content,
                road_service_cost::text,
                counterparty, counterparty_insurance,
                custom_fields, due_date, overdue_notified_at,
                created_by, created_at, updated_at, deleted_at
            "#,
        )
        .bind(tenant_id)
        .bind(&input.category)
        .bind(&input.title)
        .bind(input.occurred_at)
        .bind(input.occurred_date)
        .bind(&input.company_name)
        .bind(&input.office_name)
        .bind(&input.department)
        .bind(&input.person_name)
        .bind(input.person_id)
        .bind(&input.vehicle_number)
        .bind(&input.location)
        .bind(&input.description)
        .bind(initial_status_id)
        .bind(input.assigned_to)
        .bind(input.damage_amount)
        .bind(input.compensation_amount)
        .bind(input.road_service_cost)
        .bind(&input.counterparty)
        .bind(&input.counterparty_insurance)
        .bind(&input.custom_fields)
        .bind(input.due_date)
        .bind(created_by)
        .fetch_one(&mut *tc.conn)
        .await
    }

    async fn list(
        &self,
        tenant_id: Uuid,
        filter: &TroubleTicketFilter,
    ) -> Result<TroubleTicketsResponse, sqlx::Error> {
        let per_page = filter.per_page.unwrap_or(50).min(10000);
        let page = filter.page.unwrap_or(1).max(1);
        let offset = (page - 1) * per_page;

        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;

        let mut where_clauses = vec![
            "tenant_id = $1".to_string(),
            "deleted_at IS NULL".to_string(),
        ];
        let mut idx = 2u32;

        if filter.category.is_some() {
            where_clauses.push(format!("category = ${idx}"));
            idx += 1;
        }
        if filter.status_id.is_some() {
            where_clauses.push(format!("status_id = ${idx}"));
            idx += 1;
        }
        if filter.person_name.is_some() {
            where_clauses.push(format!("person_name ILIKE '%' || ${idx} || '%'"));
            idx += 1;
        }
        if filter.company_name.is_some() {
            where_clauses.push(format!("company_name ILIKE '%' || ${idx} || '%'"));
            idx += 1;
        }
        if filter.office_name.is_some() {
            where_clauses.push(format!("office_name ILIKE '%' || ${idx} || '%'"));
            idx += 1;
        }
        if filter.date_from.is_some() {
            where_clauses.push(format!("occurred_date >= ${idx}"));
            idx += 1;
        }
        if filter.date_to.is_some() {
            where_clauses.push(format!("occurred_date <= ${idx}"));
            idx += 1;
        }
        if filter.q.is_some() {
            where_clauses.push(format!(
                "(description ILIKE '%' || ${idx} || '%' OR location ILIKE '%' || ${idx} || '%' OR person_name ILIKE '%' || ${idx} || '%')"
            ));
            idx += 1;
        }

        let where_sql = where_clauses.join(" AND ");

        let count_sql = format!("SELECT COUNT(*) as count FROM trouble_tickets WHERE {where_sql}");
        let list_sql = format!(
            r#"SELECT id, tenant_id, ticket_no, category, title,
                occurred_at, occurred_date,
                company_name, office_name, department,
                person_name, person_id, vehicle_number,
                location, description,
                status_id, assigned_to,
                progress_notes, allowance,
                damage_amount::text, compensation_amount::text,
                confirmation_notice, disciplinary_content,
                road_service_cost::text,
                counterparty, counterparty_insurance,
                custom_fields, due_date, overdue_notified_at,
                created_by, created_at, updated_at, deleted_at
            FROM trouble_tickets
            WHERE {where_sql}
            ORDER BY ticket_no DESC
            LIMIT ${idx} OFFSET ${}"#,
            idx + 1
        );

        // Build count query
        let mut count_q = sqlx::query_scalar::<_, i64>(&count_sql).bind(tenant_id);
        let mut list_q = sqlx::query_as::<_, TroubleTicket>(&list_sql).bind(tenant_id);

        macro_rules! bind_filters {
            ($q:expr) => {
                if let Some(ref v) = filter.category {
                    $q = $q.bind(v);
                }
                if let Some(ref v) = filter.status_id {
                    $q = $q.bind(v);
                }
                if let Some(ref v) = filter.person_name {
                    $q = $q.bind(v);
                }
                if let Some(ref v) = filter.company_name {
                    $q = $q.bind(v);
                }
                if let Some(ref v) = filter.office_name {
                    $q = $q.bind(v);
                }
                if let Some(ref v) = filter.date_from {
                    $q = $q.bind(v);
                }
                if let Some(ref v) = filter.date_to {
                    $q = $q.bind(v);
                }
                if let Some(ref v) = filter.q {
                    $q = $q.bind(v);
                }
            };
        }

        bind_filters!(count_q);
        bind_filters!(list_q);
        list_q = list_q.bind(per_page).bind(offset);

        let total = count_q.fetch_one(&mut *tc.conn).await?;
        let tickets = list_q.fetch_all(&mut *tc.conn).await?;

        Ok(TroubleTicketsResponse {
            tickets,
            total,
            page,
            per_page,
        })
    }

    async fn get(&self, tenant_id: Uuid, id: Uuid) -> Result<Option<TroubleTicket>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, TroubleTicket>(
            r#"SELECT id, tenant_id, ticket_no, category, title,
                occurred_at, occurred_date,
                company_name, office_name, department,
                person_name, person_id, vehicle_number,
                location, description,
                status_id, assigned_to,
                progress_notes, allowance,
                damage_amount::text, compensation_amount::text,
                confirmation_notice, disciplinary_content,
                road_service_cost::text,
                counterparty, counterparty_insurance,
                custom_fields, due_date, overdue_notified_at,
                created_by, created_at, updated_at, deleted_at
            FROM trouble_tickets
            WHERE id = $1 AND tenant_id = $2 AND deleted_at IS NULL"#,
        )
        .bind(id)
        .bind(tenant_id)
        .fetch_optional(&mut *tc.conn)
        .await
    }

    async fn update(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        input: &UpdateTroubleTicket,
    ) -> Result<Option<TroubleTicket>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, TroubleTicket>(
            r#"UPDATE trouble_tickets SET
                category = COALESCE($3, category),
                title = COALESCE($4, title),
                occurred_at = COALESCE($5, occurred_at),
                occurred_date = COALESCE($6, occurred_date),
                company_name = COALESCE($7, company_name),
                office_name = COALESCE($8, office_name),
                department = COALESCE($9, department),
                person_name = COALESCE($10, person_name),
                person_id = COALESCE($11, person_id),
                vehicle_number = COALESCE($12, vehicle_number),
                location = COALESCE($13, location),
                description = COALESCE($14, description),
                assigned_to = COALESCE($15, assigned_to),
                progress_notes = COALESCE($16, progress_notes),
                allowance = COALESCE($17, allowance),
                damage_amount = COALESCE($18, damage_amount),
                compensation_amount = COALESCE($19, compensation_amount),
                confirmation_notice = COALESCE($20, confirmation_notice),
                disciplinary_content = COALESCE($21, disciplinary_content),
                road_service_cost = COALESCE($22, road_service_cost),
                counterparty = COALESCE($23, counterparty),
                counterparty_insurance = COALESCE($24, counterparty_insurance),
                custom_fields = COALESCE($25, custom_fields),
                due_date = COALESCE($26, due_date),
                updated_at = NOW()
            WHERE id = $1 AND tenant_id = $2 AND deleted_at IS NULL
            RETURNING id, tenant_id, ticket_no, category, title,
                occurred_at, occurred_date,
                company_name, office_name, department,
                person_name, person_id, vehicle_number,
                location, description,
                status_id, assigned_to,
                progress_notes, allowance,
                damage_amount::text, compensation_amount::text,
                confirmation_notice, disciplinary_content,
                road_service_cost::text,
                counterparty, counterparty_insurance,
                custom_fields, due_date, overdue_notified_at,
                created_by, created_at, updated_at, deleted_at"#,
        )
        .bind(id)
        .bind(tenant_id)
        .bind(&input.category)
        .bind(&input.title)
        .bind(input.occurred_at)
        .bind(input.occurred_date)
        .bind(&input.company_name)
        .bind(&input.office_name)
        .bind(&input.department)
        .bind(&input.person_name)
        .bind(input.person_id)
        .bind(&input.vehicle_number)
        .bind(&input.location)
        .bind(&input.description)
        .bind(input.assigned_to)
        .bind(&input.progress_notes)
        .bind(&input.allowance)
        .bind(input.damage_amount)
        .bind(input.compensation_amount)
        .bind(&input.confirmation_notice)
        .bind(&input.disciplinary_content)
        .bind(input.road_service_cost)
        .bind(&input.counterparty)
        .bind(&input.counterparty_insurance)
        .bind(&input.custom_fields)
        .bind(input.due_date)
        .fetch_optional(&mut *tc.conn)
        .await
    }

    async fn soft_delete(&self, tenant_id: Uuid, id: Uuid) -> Result<bool, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let result = sqlx::query(
            "UPDATE trouble_tickets SET deleted_at = NOW(), updated_at = NOW() WHERE id = $1 AND tenant_id = $2 AND deleted_at IS NULL",
        )
        .bind(id)
        .bind(tenant_id)
        .execute(&mut *tc.conn)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn update_status(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        status_id: Uuid,
    ) -> Result<Option<TroubleTicket>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, TroubleTicket>(
            r#"UPDATE trouble_tickets SET
                status_id = $3, updated_at = NOW()
            WHERE id = $1 AND tenant_id = $2 AND deleted_at IS NULL
            RETURNING id, tenant_id, ticket_no, category, title,
                occurred_at, occurred_date,
                company_name, office_name, department,
                person_name, person_id, vehicle_number,
                location, description,
                status_id, assigned_to,
                progress_notes, allowance,
                damage_amount::text, compensation_amount::text,
                confirmation_notice, disciplinary_content,
                road_service_cost::text,
                counterparty, counterparty_insurance,
                custom_fields, due_date, overdue_notified_at,
                created_by, created_at, updated_at, deleted_at"#,
        )
        .bind(id)
        .bind(tenant_id)
        .bind(status_id)
        .fetch_optional(&mut *tc.conn)
        .await
    }
}
