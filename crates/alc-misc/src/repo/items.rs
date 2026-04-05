use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use alc_core::models::{CreateItem, Item, ItemFile, UpdateItem};
use alc_core::tenant::TenantConn;

pub use alc_core::repository::items::*;

pub struct PgItemsRepository {
    pool: PgPool,
}

impl PgItemsRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ItemsRepository for PgItemsRepository {
    async fn list(
        &self,
        tenant_id: Uuid,
        parent_id: Option<Uuid>,
        owner_type: &str,
    ) -> Result<Vec<Item>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, Item>(
            r#"SELECT * FROM alc_api.items
               WHERE parent_id IS NOT DISTINCT FROM $1
                 AND owner_type = $2
               ORDER BY item_type DESC, name"#,
        )
        .bind(parent_id)
        .bind(owner_type)
        .fetch_all(&mut *tc.conn)
        .await
    }

    async fn get(&self, tenant_id: Uuid, id: Uuid) -> Result<Option<Item>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, Item>("SELECT * FROM alc_api.items WHERE id = $1")
            .bind(id)
            .fetch_optional(&mut *tc.conn)
            .await
    }

    async fn create(&self, tenant_id: Uuid, item: &CreateItem) -> Result<Item, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, Item>(
            r#"INSERT INTO alc_api.items
               (tenant_id, parent_id, owner_type, owner_user_id, item_type, name, barcode, category, description, image_url, url, quantity)
               VALUES ($1, $2, COALESCE($3, 'org'), $4, COALESCE($5, 'item'), $6, COALESCE($7, ''), COALESCE($8, ''), COALESCE($9, ''), COALESCE($10, ''), COALESCE($11, ''), COALESCE($12, 1))
               RETURNING *"#,
        )
        .bind(tenant_id)
        .bind(item.parent_id)
        .bind(&item.owner_type)
        .bind(item.owner_user_id)
        .bind(&item.item_type)
        .bind(&item.name)
        .bind(&item.barcode)
        .bind(&item.category)
        .bind(&item.description)
        .bind(&item.image_url)
        .bind(&item.url)
        .bind(item.quantity)
        .fetch_one(&mut *tc.conn)
        .await
    }

    async fn update(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        item: &UpdateItem,
    ) -> Result<Option<Item>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, Item>(
            r#"UPDATE alc_api.items SET
                   name = COALESCE($1, name),
                   barcode = COALESCE($2, barcode),
                   category = COALESCE($3, category),
                   description = COALESCE($4, description),
                   image_url = COALESCE($5, image_url),
                   url = COALESCE($6, url),
                   quantity = COALESCE($7, quantity),
                   updated_at = NOW()
               WHERE id = $8
               RETURNING *"#,
        )
        .bind(&item.name)
        .bind(&item.barcode)
        .bind(&item.category)
        .bind(&item.description)
        .bind(&item.image_url)
        .bind(&item.url)
        .bind(item.quantity)
        .bind(id)
        .fetch_optional(&mut *tc.conn)
        .await
    }

    async fn delete(&self, tenant_id: Uuid, id: Uuid) -> Result<bool, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let result = sqlx::query("DELETE FROM alc_api.items WHERE id = $1")
            .bind(id)
            .execute(&mut *tc.conn)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn move_item(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        new_parent_id: Option<Uuid>,
    ) -> Result<Option<Item>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, Item>(
            r#"UPDATE alc_api.items SET parent_id = $1, updated_at = NOW()
               WHERE id = $2
               RETURNING *"#,
        )
        .bind(new_parent_id)
        .bind(id)
        .fetch_optional(&mut *tc.conn)
        .await
    }

    async fn change_ownership(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        new_owner_type: &str,
    ) -> Result<Option<Item>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, Item>(
            r#"UPDATE alc_api.items SET owner_type = $1, updated_at = NOW()
               WHERE id = $2
               RETURNING *"#,
        )
        .bind(new_owner_type)
        .bind(id)
        .fetch_optional(&mut *tc.conn)
        .await
    }

    async fn convert_type(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        new_item_type: &str,
    ) -> Result<Option<(Item, i64)>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;

        // Count children that will be moved (only relevant when converting folder→item)
        let children_moved: i64 = if new_item_type == "item" {
            let row: (i64,) =
                sqlx::query_as("SELECT COUNT(*) FROM alc_api.items WHERE parent_id = $1")
                    .bind(id)
                    .fetch_one(&mut *tc.conn)
                    .await?;
            // Move children to parent's parent
            if row.0 > 0 {
                // Get current item to find its parent_id
                let current =
                    sqlx::query_as::<_, Item>("SELECT * FROM alc_api.items WHERE id = $1")
                        .bind(id)
                        .fetch_optional(&mut *tc.conn)
                        .await?;
                if let Some(current) = current {
                    sqlx::query(
                        "UPDATE alc_api.items SET parent_id = $1, updated_at = NOW() WHERE parent_id = $2",
                    )
                    .bind(current.parent_id)
                    .bind(id)
                    .execute(&mut *tc.conn)
                    .await?;
                }
            }
            row.0
        } else {
            0
        };

        let item = sqlx::query_as::<_, Item>(
            r#"UPDATE alc_api.items SET item_type = $1, updated_at = NOW()
               WHERE id = $2
               RETURNING *"#,
        )
        .bind(new_item_type)
        .bind(id)
        .fetch_optional(&mut *tc.conn)
        .await?;

        Ok(item.map(|i| (i, children_moved)))
    }

    async fn search_by_barcode(
        &self,
        tenant_id: Uuid,
        barcode: &str,
    ) -> Result<Vec<Item>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, Item>("SELECT * FROM alc_api.items WHERE barcode = $1 ORDER BY name")
            .bind(barcode)
            .fetch_all(&mut *tc.conn)
            .await
    }
}

pub struct PgItemFilesRepository {
    pool: PgPool,
}

impl PgItemFilesRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ItemFilesRepository for PgItemFilesRepository {
    async fn create(
        &self,
        tenant_id: Uuid,
        filename: &str,
        content_type: &str,
        size_bytes: i64,
    ) -> Result<ItemFile, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, ItemFile>(
            r#"INSERT INTO alc_api.item_files (tenant_id, filename, content_type, size_bytes)
               VALUES ($1, $2, $3, $4)
               RETURNING *"#,
        )
        .bind(tenant_id)
        .bind(filename)
        .bind(content_type)
        .bind(size_bytes)
        .fetch_one(&mut *tc.conn)
        .await
    }

    async fn get(&self, tenant_id: Uuid, id: Uuid) -> Result<Option<ItemFile>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, ItemFile>("SELECT * FROM alc_api.item_files WHERE id = $1")
            .bind(id)
            .fetch_optional(&mut *tc.conn)
            .await
    }
}
