use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use crate::db::models::{CarryingItem, CarryingItemVehicleCondition};

use super::TenantConn;

pub use alc_core::repository::carrying_items::*;

pub struct PgCarryingItemsRepository {
    pool: PgPool,
}

impl PgCarryingItemsRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl CarryingItemsRepository for PgCarryingItemsRepository {
    async fn list(&self, tenant_id: Uuid) -> Result<Vec<CarryingItem>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, CarryingItem>(
            "SELECT * FROM alc_api.carrying_items ORDER BY sort_order, created_at",
        )
        .fetch_all(&mut *tc.conn)
        .await
    }

    async fn list_conditions(
        &self,
        tenant_id: Uuid,
        item_ids: &[Uuid],
    ) -> Result<Vec<CarryingItemVehicleCondition>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, CarryingItemVehicleCondition>(
            "SELECT * FROM alc_api.carrying_item_vehicle_conditions WHERE carrying_item_id = ANY($1) ORDER BY category, value",
        )
        .bind(item_ids)
        .fetch_all(&mut *tc.conn)
        .await
    }

    async fn create(
        &self,
        tenant_id: Uuid,
        item_name: &str,
        is_required: bool,
        sort_order: i32,
    ) -> Result<CarryingItem, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, CarryingItem>(
            r#"INSERT INTO alc_api.carrying_items (tenant_id, item_name, is_required, sort_order)
               VALUES ($1, $2, $3, $4)
               RETURNING *"#,
        )
        .bind(tenant_id)
        .bind(item_name)
        .bind(is_required)
        .bind(sort_order)
        .fetch_one(&mut *tc.conn)
        .await
    }

    async fn insert_condition(
        &self,
        tenant_id: Uuid,
        item_id: Uuid,
        category: &str,
        value: &str,
    ) -> Result<Option<CarryingItemVehicleCondition>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, CarryingItemVehicleCondition>(
            r#"INSERT INTO alc_api.carrying_item_vehicle_conditions (carrying_item_id, category, value)
               VALUES ($1, $2, $3)
               ON CONFLICT (carrying_item_id, category, value) DO NOTHING
               RETURNING *"#,
        )
        .bind(item_id)
        .bind(category)
        .bind(value)
        .fetch_optional(&mut *tc.conn)
        .await
    }

    async fn update(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        item_name: Option<&str>,
        is_required: Option<bool>,
        sort_order: Option<i32>,
    ) -> Result<Option<CarryingItem>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, CarryingItem>(
            r#"UPDATE alc_api.carrying_items SET
                   item_name = COALESCE($1, item_name),
                   is_required = COALESCE($2, is_required),
                   sort_order = COALESCE($3, sort_order)
               WHERE id = $4
               RETURNING *"#,
        )
        .bind(item_name)
        .bind(is_required)
        .bind(sort_order)
        .bind(id)
        .fetch_optional(&mut *tc.conn)
        .await
    }

    async fn delete_conditions(&self, tenant_id: Uuid, item_id: Uuid) -> Result<(), sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query(
            "DELETE FROM alc_api.carrying_item_vehicle_conditions WHERE carrying_item_id = $1",
        )
        .bind(item_id)
        .execute(&mut *tc.conn)
        .await?;
        Ok(())
    }

    async fn get_conditions(
        &self,
        tenant_id: Uuid,
        item_id: Uuid,
    ) -> Result<Vec<CarryingItemVehicleCondition>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, CarryingItemVehicleCondition>(
            "SELECT * FROM alc_api.carrying_item_vehicle_conditions WHERE carrying_item_id = $1 ORDER BY category, value",
        )
        .bind(item_id)
        .fetch_all(&mut *tc.conn)
        .await
    }

    async fn delete(&self, tenant_id: Uuid, id: Uuid) -> Result<bool, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let result = sqlx::query("DELETE FROM alc_api.carrying_items WHERE id = $1")
            .bind(id)
            .execute(&mut *tc.conn)
            .await?;
        Ok(result.rows_affected() > 0)
    }
}
