use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use alc_core::models::{
    CreateWorkflowState, CreateWorkflowTransition, TroubleStatusHistory, TroubleWorkflowState,
    TroubleWorkflowTransition,
};
use alc_core::tenant::TenantConn;

pub use alc_core::repository::trouble_workflow::*;

pub struct PgTroubleWorkflowRepository {
    pool: PgPool,
}

impl PgTroubleWorkflowRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl TroubleWorkflowRepository for PgTroubleWorkflowRepository {
    async fn list_states(&self, tenant_id: Uuid) -> Result<Vec<TroubleWorkflowState>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, TroubleWorkflowState>(
            "SELECT * FROM trouble_workflow_states WHERE tenant_id = $1 ORDER BY sort_order",
        )
        .bind(tenant_id)
        .fetch_all(&mut *tc.conn)
        .await
    }

    async fn create_state(
        &self,
        tenant_id: Uuid,
        input: &CreateWorkflowState,
    ) -> Result<TroubleWorkflowState, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, TroubleWorkflowState>(
            r#"INSERT INTO trouble_workflow_states (tenant_id, name, label, color, sort_order, is_initial, is_terminal)
            VALUES ($1, $2, $3, COALESCE($4, '#6B7280'), COALESCE($5, 0), COALESCE($6, FALSE), COALESCE($7, FALSE))
            RETURNING *"#,
        )
        .bind(tenant_id)
        .bind(&input.name)
        .bind(&input.label)
        .bind(&input.color)
        .bind(input.sort_order)
        .bind(input.is_initial)
        .bind(input.is_terminal)
        .fetch_one(&mut *tc.conn)
        .await
    }

    async fn delete_state(&self, tenant_id: Uuid, id: Uuid) -> Result<bool, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let result =
            sqlx::query("DELETE FROM trouble_workflow_states WHERE id = $1 AND tenant_id = $2")
                .bind(id)
                .bind(tenant_id)
                .execute(&mut *tc.conn)
                .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn list_transitions(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<TroubleWorkflowTransition>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, TroubleWorkflowTransition>(
            "SELECT * FROM trouble_workflow_transitions WHERE tenant_id = $1",
        )
        .bind(tenant_id)
        .fetch_all(&mut *tc.conn)
        .await
    }

    async fn create_transition(
        &self,
        tenant_id: Uuid,
        input: &CreateWorkflowTransition,
    ) -> Result<TroubleWorkflowTransition, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, TroubleWorkflowTransition>(
            r#"INSERT INTO trouble_workflow_transitions (tenant_id, from_state_id, to_state_id, label)
            VALUES ($1, $2, $3, $4)
            RETURNING *"#,
        )
        .bind(tenant_id)
        .bind(input.from_state_id)
        .bind(input.to_state_id)
        .bind(&input.label)
        .fetch_one(&mut *tc.conn)
        .await
    }

    async fn delete_transition(&self, tenant_id: Uuid, id: Uuid) -> Result<bool, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let result = sqlx::query(
            "DELETE FROM trouble_workflow_transitions WHERE id = $1 AND tenant_id = $2",
        )
        .bind(id)
        .bind(tenant_id)
        .execute(&mut *tc.conn)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn is_transition_allowed(
        &self,
        tenant_id: Uuid,
        from_state_id: Option<Uuid>,
        to_state_id: Uuid,
    ) -> Result<bool, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;

        // from_state_id が None (初期状態からの遷移) の場合、to_state が initial なら OK
        if from_state_id.is_none() {
            let is_initial = sqlx::query_scalar::<_, bool>(
                "SELECT is_initial FROM trouble_workflow_states WHERE id = $1 AND tenant_id = $2",
            )
            .bind(to_state_id)
            .bind(tenant_id)
            .fetch_optional(&mut *tc.conn)
            .await?;
            return Ok(is_initial.unwrap_or(false));
        }

        let exists = sqlx::query_scalar::<_, bool>(
            r#"SELECT EXISTS(
                SELECT 1 FROM trouble_workflow_transitions
                WHERE tenant_id = $1 AND from_state_id = $2 AND to_state_id = $3
            )"#,
        )
        .bind(tenant_id)
        .bind(from_state_id.unwrap())
        .bind(to_state_id)
        .fetch_one(&mut *tc.conn)
        .await?;

        Ok(exists)
    }

    async fn get_initial_state(
        &self,
        tenant_id: Uuid,
    ) -> Result<Option<TroubleWorkflowState>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, TroubleWorkflowState>(
            "SELECT * FROM trouble_workflow_states WHERE tenant_id = $1 AND is_initial = TRUE LIMIT 1",
        )
        .bind(tenant_id)
        .fetch_optional(&mut *tc.conn)
        .await
    }

    async fn record_history(
        &self,
        tenant_id: Uuid,
        ticket_id: Uuid,
        from_state_id: Option<Uuid>,
        to_state_id: Uuid,
        changed_by: Option<Uuid>,
        comment: Option<String>,
    ) -> Result<TroubleStatusHistory, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, TroubleStatusHistory>(
            r#"INSERT INTO trouble_status_history (tenant_id, ticket_id, from_state_id, to_state_id, changed_by, comment)
            VALUES ($1, $2, $3, $4, $5, COALESCE($6, ''))
            RETURNING *"#,
        )
        .bind(tenant_id)
        .bind(ticket_id)
        .bind(from_state_id)
        .bind(to_state_id)
        .bind(changed_by)
        .bind(comment)
        .fetch_one(&mut *tc.conn)
        .await
    }

    async fn list_history(
        &self,
        tenant_id: Uuid,
        ticket_id: Uuid,
    ) -> Result<Vec<TroubleStatusHistory>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, TroubleStatusHistory>(
            "SELECT * FROM trouble_status_history WHERE ticket_id = $1 AND tenant_id = $2 ORDER BY created_at",
        )
        .bind(ticket_id)
        .bind(tenant_id)
        .fetch_all(&mut *tc.conn)
        .await
    }

    async fn setup_defaults(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<TroubleWorkflowState>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;

        // 既存チェック
        let existing = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM trouble_workflow_states WHERE tenant_id = $1",
        )
        .bind(tenant_id)
        .fetch_one(&mut *tc.conn)
        .await?;

        if existing > 0 {
            return sqlx::query_as::<_, TroubleWorkflowState>(
                "SELECT * FROM trouble_workflow_states WHERE tenant_id = $1 ORDER BY sort_order",
            )
            .bind(tenant_id)
            .fetch_all(&mut *tc.conn)
            .await;
        }

        // デフォルト状態を作成
        let defaults = [
            ("new", "新規", "#3B82F6", 1, true, false),
            ("in_progress", "対応中", "#F59E0B", 2, false, false),
            ("resolved", "解決", "#10B981", 3, false, false),
            ("closed", "完了", "#6B7280", 4, false, true),
        ];

        let mut states = Vec::new();
        for (name, label, color, order, initial, terminal) in &defaults {
            let s = sqlx::query_as::<_, TroubleWorkflowState>(
                r#"INSERT INTO trouble_workflow_states (tenant_id, name, label, color, sort_order, is_initial, is_terminal)
                VALUES ($1, $2, $3, $4, $5, $6, $7)
                RETURNING *"#,
            )
            .bind(tenant_id)
            .bind(name)
            .bind(label)
            .bind(color)
            .bind(order)
            .bind(initial)
            .bind(terminal)
            .fetch_one(&mut *tc.conn)
            .await?;
            states.push(s);
        }

        // デフォルト遷移を作成
        let transitions = [
            (0, 1, "対応開始"),
            (1, 2, "解決"),
            (2, 3, "完了"),
            (1, 0, "差し戻し"),
            (2, 1, "再対応"),
        ];

        for (from_idx, to_idx, label) in &transitions {
            let _ = sqlx::query(
                r#"INSERT INTO trouble_workflow_transitions (tenant_id, from_state_id, to_state_id, label)
                VALUES ($1, $2, $3, $4)"#,
            )
            .bind(tenant_id)
            .bind(states[*from_idx].id)
            .bind(states[*to_idx].id)
            .bind(label)
            .execute(&mut *tc.conn)
            .await;
        }

        Ok(states)
    }
}
