use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use crate::db::models::{
    CreateEquipmentFailure, EquipmentFailure, EquipmentFailureFilter, EquipmentFailuresResponse,
    UpdateEquipmentFailure,
};

use super::TenantConn;

pub use alc_core::repository::equipment_failures::*;

pub struct PgEquipmentFailuresRepository {
    pool: PgPool,
}

impl PgEquipmentFailuresRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl EquipmentFailuresRepository for PgEquipmentFailuresRepository {
    async fn create(
        &self,
        tenant_id: Uuid,
        input: &CreateEquipmentFailure,
    ) -> Result<EquipmentFailure, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, EquipmentFailure>(
            r#"
            INSERT INTO equipment_failures (
                tenant_id, failure_type, description, affected_device,
                detected_at, detected_by, session_id
            )
            VALUES ($1, $2, $3, $4, COALESCE($5, NOW()), $6, $7)
            RETURNING *
            "#,
        )
        .bind(tenant_id)
        .bind(&input.failure_type)
        .bind(&input.description)
        .bind(&input.affected_device)
        .bind(input.detected_at)
        .bind(&input.detected_by)
        .bind(input.session_id)
        .fetch_one(&mut *tc.conn)
        .await
    }

    async fn list(
        &self,
        tenant_id: Uuid,
        filter: &EquipmentFailureFilter,
    ) -> Result<EquipmentFailuresResponse, sqlx::Error> {
        let per_page = filter.per_page.unwrap_or(50).min(100);
        let page = filter.page.unwrap_or(1).max(1);
        let offset = (page - 1) * per_page;

        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;

        let mut conditions = vec!["tenant_id = $1".to_string()];
        let mut param_idx = 2u32;

        if filter.failure_type.is_some() {
            conditions.push(format!("failure_type = ${param_idx}"));
            param_idx += 1;
        }
        if let Some(resolved) = filter.resolved {
            if resolved {
                conditions.push("resolved_at IS NOT NULL".to_string());
            } else {
                conditions.push("resolved_at IS NULL".to_string());
            }
        }
        if filter.session_id.is_some() {
            conditions.push(format!("session_id = ${param_idx}"));
            param_idx += 1;
        }
        if filter.date_from.is_some() {
            conditions.push(format!("detected_at >= ${param_idx}"));
            param_idx += 1;
        }
        if filter.date_to.is_some() {
            conditions.push(format!("detected_at <= ${param_idx}"));
            param_idx += 1;
        }

        let where_clause = conditions.join(" AND ");

        // Count
        let count_sql = format!("SELECT COUNT(*) FROM equipment_failures WHERE {where_clause}");
        let mut count_query = sqlx::query_scalar::<_, i64>(&count_sql).bind(tenant_id);
        if let Some(ref ft) = filter.failure_type {
            count_query = count_query.bind(ft);
        }
        if let Some(sid) = filter.session_id {
            count_query = count_query.bind(sid);
        }
        if let Some(df) = filter.date_from {
            count_query = count_query.bind(df);
        }
        if let Some(dt) = filter.date_to {
            count_query = count_query.bind(dt);
        }
        let total = count_query.fetch_one(&mut *tc.conn).await?;

        // List
        let sql = format!(
            "SELECT * FROM equipment_failures WHERE {where_clause} ORDER BY detected_at DESC LIMIT ${param_idx} OFFSET ${}",
            param_idx + 1
        );
        let mut query = sqlx::query_as::<_, EquipmentFailure>(&sql).bind(tenant_id);
        if let Some(ref ft) = filter.failure_type {
            query = query.bind(ft);
        }
        if let Some(sid) = filter.session_id {
            query = query.bind(sid);
        }
        if let Some(df) = filter.date_from {
            query = query.bind(df);
        }
        if let Some(dt) = filter.date_to {
            query = query.bind(dt);
        }
        query = query.bind(per_page).bind(offset);

        let failures = query.fetch_all(&mut *tc.conn).await?;

        Ok(EquipmentFailuresResponse {
            failures,
            total,
            page,
            per_page,
        })
    }

    async fn get(
        &self,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<Option<EquipmentFailure>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, EquipmentFailure>(
            "SELECT * FROM equipment_failures WHERE id = $1 AND tenant_id = $2",
        )
        .bind(id)
        .bind(tenant_id)
        .fetch_optional(&mut *tc.conn)
        .await
    }

    async fn resolve(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        input: &UpdateEquipmentFailure,
    ) -> Result<Option<EquipmentFailure>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, EquipmentFailure>(
            r#"
            UPDATE equipment_failures SET
                resolved_at = NOW(),
                resolution_notes = $3,
                updated_at = NOW()
            WHERE id = $1 AND tenant_id = $2
            RETURNING *
            "#,
        )
        .bind(id)
        .bind(tenant_id)
        .bind(&input.resolution_notes)
        .fetch_optional(&mut *tc.conn)
        .await
    }

    async fn list_for_csv(
        &self,
        tenant_id: Uuid,
        filter: &EquipmentFailureFilter,
    ) -> Result<Vec<EquipmentFailure>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;

        let mut conditions = vec!["tenant_id = $1".to_string()];
        let mut param_idx = 2u32;

        if filter.date_from.is_some() {
            conditions.push(format!("detected_at >= ${param_idx}"));
            param_idx += 1;
        }
        if filter.date_to.is_some() {
            conditions.push(format!("detected_at <= ${param_idx}"));
            param_idx += 1;
        }
        let _ = param_idx;

        let where_clause = conditions.join(" AND ");
        let sql = format!(
            "SELECT * FROM equipment_failures WHERE {where_clause} ORDER BY detected_at DESC"
        );
        let mut query = sqlx::query_as::<_, EquipmentFailure>(&sql).bind(tenant_id);
        if let Some(df) = filter.date_from {
            query = query.bind(df);
        }
        if let Some(dt) = filter.date_to {
            query = query.bind(dt);
        }

        query.fetch_all(&mut *tc.conn).await
    }
}
