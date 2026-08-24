use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use crate::models::{CreateTroubleTask, TroubleTask, UpdateTroubleTask};
use alc_core::tenant::TenantConn;

pub use crate::repository::trouble_tasks::*;

pub struct PgTroubleTasksRepository {
    pool: PgPool,
}

impl PgTroubleTasksRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl TroubleTasksRepository for PgTroubleTasksRepository {
    async fn create(
        &self,
        tenant_id: Uuid,
        ticket_id: Uuid,
        created_by: Option<Uuid>,
        input: &CreateTroubleTask,
    ) -> Result<TroubleTask, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, TroubleTask>(
            r#"INSERT INTO trouble_tasks (tenant_id, ticket_id, task_type, title, description, assigned_to, due_date, sort_order, created_by, next_action, next_action_detail, next_action_by, next_action_due, occurred_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7,
                COALESCE($8, (SELECT COALESCE(MAX(sort_order) + 1, 0) FROM trouble_tasks WHERE tenant_id = $1 AND ticket_id = $2)),
                $9, COALESCE($10, ''), COALESCE($11, ''), $12, $13, $14)
            RETURNING *"#,
        )
        .bind(tenant_id)
        .bind(ticket_id)
        .bind(&input.task_type)
        .bind(&input.title)
        .bind(&input.description)
        .bind(input.assigned_to)
        .bind(input.due_date)
        .bind(input.sort_order)
        .bind(created_by)
        .bind(&input.next_action)
        .bind(&input.next_action_detail)
        .bind(&input.next_action_by)
        .bind(input.next_action_due)
        .bind(input.occurred_at)
        .fetch_one(&mut *tc.conn)
        .await
    }

    async fn list_by_ticket(
        &self,
        tenant_id: Uuid,
        ticket_id: Uuid,
    ) -> Result<Vec<TroubleTask>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, TroubleTask>(
            "SELECT * FROM trouble_tasks WHERE ticket_id = $1 AND tenant_id = $2 ORDER BY sort_order, created_at",
        )
        .bind(ticket_id)
        .bind(tenant_id)
        .fetch_all(&mut *tc.conn)
        .await
    }

    async fn get(&self, tenant_id: Uuid, id: Uuid) -> Result<Option<TroubleTask>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, TroubleTask>(
            "SELECT * FROM trouble_tasks WHERE id = $1 AND tenant_id = $2",
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
        input: &UpdateTroubleTask,
    ) -> Result<Option<TroubleTask>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, TroubleTask>(
            r#"UPDATE trouble_tasks SET
                task_type = COALESCE($3, task_type),
                title = COALESCE($4, title),
                description = COALESCE($5, description),
                status = COALESCE($6, status),
                assigned_to = CASE WHEN $7::boolean THEN $8 ELSE assigned_to END,
                due_date = CASE WHEN $9::boolean THEN $10 ELSE due_date END,
                completed_at = CASE WHEN $11::boolean THEN $12 ELSE completed_at END,
                sort_order = COALESCE($13, sort_order),
                next_action = COALESCE($14, next_action),
                next_action_detail = COALESCE($15, next_action_detail),
                next_action_by = CASE WHEN $16::boolean THEN $17 ELSE next_action_by END,
                next_action_due = CASE WHEN $18::boolean THEN $19 ELSE next_action_due END,
                occurred_at = CASE WHEN $20::boolean THEN $21 ELSE occurred_at END,
                print_page_break_before = COALESCE($22, print_page_break_before),
                updated_at = now()
            WHERE id = $1 AND tenant_id = $2
            RETURNING *"#,
        )
        .bind(id)
        .bind(tenant_id)
        .bind(&input.task_type)
        .bind(&input.title)
        .bind(&input.description)
        .bind(&input.status)
        .bind(input.assigned_to.is_some())
        .bind(input.assigned_to.as_ref().and_then(|v| *v))
        .bind(input.due_date.is_some())
        .bind(input.due_date.as_ref().and_then(|v| *v))
        .bind(input.completed_at.is_some())
        .bind(input.completed_at.as_ref().and_then(|v| *v))
        .bind(input.sort_order)
        .bind(&input.next_action)
        .bind(&input.next_action_detail)
        .bind(input.next_action_by.is_some())
        .bind(input.next_action_by.as_ref().and_then(|v| v.as_deref()))
        .bind(input.next_action_due.is_some())
        .bind(input.next_action_due.as_ref().and_then(|v| *v))
        .bind(input.occurred_at.is_some())
        .bind(input.occurred_at.as_ref().and_then(|v| *v))
        .bind(input.print_page_break_before)
        .fetch_optional(&mut *tc.conn)
        .await
    }

    async fn delete(&self, tenant_id: Uuid, id: Uuid) -> Result<bool, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let result = sqlx::query("DELETE FROM trouble_tasks WHERE id = $1 AND tenant_id = $2")
            .bind(id)
            .bind(tenant_id)
            .execute(&mut *tc.conn)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn reorder(
        &self,
        tenant_id: Uuid,
        ticket_id: Uuid,
        task_ids: &[Uuid],
    ) -> Result<Option<Vec<TroubleTask>>, sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        alc_core::tenant::set_current_tenant(&mut tx, &tenant_id.to_string()).await?;

        // 渡された順 (ORDINALITY) をそのまま 0 起点の sort_order にする。
        // 1 文で全行を更新するので、行ごとの PUT を並べる形の競合が起きない。
        let res = sqlx::query(
            r#"UPDATE trouble_tasks t
               SET sort_order = (v.ord - 1)::INT, updated_at = now()
               FROM unnest($3::uuid[]) WITH ORDINALITY AS v(id, ord)
               WHERE t.id = v.id AND t.ticket_id = $1 AND t.tenant_id = $2"#,
        )
        .bind(ticket_id)
        .bind(tenant_id)
        .bind(task_ids)
        .execute(&mut *tx)
        .await?;

        // このチケットに属さない id / 重複 id があると更新行数が足りない
        // (重複は同一行を 2 回数えないため)。部分適用を残さず巻き戻す。
        if res.rows_affected() != task_ids.len() as u64 {
            tx.rollback().await?;
            return Ok(None);
        }

        let tasks = sqlx::query_as::<_, TroubleTask>(
            "SELECT * FROM trouble_tasks WHERE ticket_id = $1 AND tenant_id = $2 ORDER BY sort_order, created_at",
        )
        .bind(ticket_id)
        .bind(tenant_id)
        .fetch_all(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(Some(tasks))
    }

    async fn list_all(
        &self,
        tenant_id: Uuid,
        filter: &TroubleTasksFilter,
        sort_by: TroubleTasksSortBy,
        sort_desc: bool,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<TroubleTask>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;

        let (where_sql, next_idx) = build_where_sql(filter);
        let direction = if sort_desc { "DESC" } else { "ASC" };
        let sort_col = sort_by.column();

        let list_sql = format!(
            "SELECT * FROM trouble_tasks WHERE {where_sql} \
             ORDER BY {sort_col} {direction} NULLS LAST, created_at DESC \
             LIMIT ${limit_idx} OFFSET ${offset_idx}",
            limit_idx = next_idx,
            offset_idx = next_idx + 1,
        );

        let mut q = sqlx::query_as::<_, TroubleTask>(&list_sql).bind(tenant_id);
        q = bind_filters(q, filter);
        q = q.bind(limit).bind(offset);
        q.fetch_all(&mut *tc.conn).await
    }

    async fn count_all(
        &self,
        tenant_id: Uuid,
        filter: &TroubleTasksFilter,
    ) -> Result<i64, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let (where_sql, _next_idx) = build_where_sql(filter);
        let count_sql = format!("SELECT COUNT(*) FROM trouble_tasks WHERE {where_sql}");
        let mut q = sqlx::query_scalar::<_, i64>(&count_sql).bind(tenant_id);
        q = bind_filters_scalar(q, filter);
        q.fetch_one(&mut *tc.conn).await
    }
}

/// Build `WHERE` clause and return `(sql, next_param_index)`.
/// `$1` is reserved for `tenant_id`; filter binds start at `$2`.
fn build_where_sql(filter: &TroubleTasksFilter) -> (String, u32) {
    let mut clauses = vec!["tenant_id = $1".to_string()];
    let mut idx: u32 = 2;

    if filter.ticket_id.is_some() {
        clauses.push(format!("ticket_id = ${idx}"));
        idx += 1;
    }
    if filter.status.is_some() {
        clauses.push(format!("status = ${idx}"));
        idx += 1;
    }
    if filter.task_type.is_some() {
        clauses.push(format!("task_type = ${idx}"));
        idx += 1;
    }
    if filter.assigned_to.is_some() {
        clauses.push(format!("assigned_to = ${idx}"));
        idx += 1;
    }
    if filter.q.is_some() {
        clauses.push(format!(
            "(title ILIKE '%' || ${idx} || '%' OR description ILIKE '%' || ${idx} || '%')"
        ));
        idx += 1;
    }
    if filter.due_from.is_some() {
        clauses.push(format!("due_date >= ${idx}"));
        idx += 1;
    }
    if filter.due_to.is_some() {
        clauses.push(format!("due_date <= ${idx}"));
        idx += 1;
    }
    if filter.occurred_from.is_some() {
        clauses.push(format!("occurred_at >= ${idx}"));
        idx += 1;
    }
    if filter.occurred_to.is_some() {
        clauses.push(format!("occurred_at <= ${idx}"));
        idx += 1;
    }

    (clauses.join(" AND "), idx)
}

fn bind_filters<'q>(
    mut q: sqlx::query::QueryAs<'q, sqlx::Postgres, TroubleTask, sqlx::postgres::PgArguments>,
    filter: &'q TroubleTasksFilter,
) -> sqlx::query::QueryAs<'q, sqlx::Postgres, TroubleTask, sqlx::postgres::PgArguments> {
    if let Some(v) = filter.ticket_id {
        q = q.bind(v);
    }
    if let Some(ref v) = filter.status {
        q = q.bind(v);
    }
    if let Some(ref v) = filter.task_type {
        q = q.bind(v);
    }
    if let Some(v) = filter.assigned_to {
        q = q.bind(v);
    }
    if let Some(ref v) = filter.q {
        q = q.bind(v);
    }
    if let Some(v) = filter.due_from {
        q = q.bind(v);
    }
    if let Some(v) = filter.due_to {
        q = q.bind(v);
    }
    if let Some(v) = filter.occurred_from {
        q = q.bind(v);
    }
    if let Some(v) = filter.occurred_to {
        q = q.bind(v);
    }
    q
}

fn bind_filters_scalar<'q>(
    mut q: sqlx::query::QueryScalar<'q, sqlx::Postgres, i64, sqlx::postgres::PgArguments>,
    filter: &'q TroubleTasksFilter,
) -> sqlx::query::QueryScalar<'q, sqlx::Postgres, i64, sqlx::postgres::PgArguments> {
    if let Some(v) = filter.ticket_id {
        q = q.bind(v);
    }
    if let Some(ref v) = filter.status {
        q = q.bind(v);
    }
    if let Some(ref v) = filter.task_type {
        q = q.bind(v);
    }
    if let Some(v) = filter.assigned_to {
        q = q.bind(v);
    }
    if let Some(ref v) = filter.q {
        q = q.bind(v);
    }
    if let Some(v) = filter.due_from {
        q = q.bind(v);
    }
    if let Some(v) = filter.due_to {
        q = q.bind(v);
    }
    if let Some(v) = filter.occurred_from {
        q = q.bind(v);
    }
    if let Some(v) = filter.occurred_to {
        q = q.bind(v);
    }
    q
}
