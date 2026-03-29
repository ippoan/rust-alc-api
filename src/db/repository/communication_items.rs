use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use crate::db::models::{CommunicationItem, CreateCommunicationItem, UpdateCommunicationItem};

use super::TenantConn;

/// 伝達事項の一覧取得結果 (WITH name join)
#[derive(Debug, serde::Serialize, sqlx::FromRow)]
pub struct CommunicationItemWithName {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub title: String,
    pub content: String,
    pub priority: String,
    pub target_employee_id: Option<Uuid>,
    pub target_employee_name: Option<String>,
    pub is_active: bool,
    pub effective_from: Option<chrono::DateTime<chrono::Utc>>,
    pub effective_until: Option<chrono::DateTime<chrono::Utc>>,
    pub created_by: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[async_trait]
pub trait CommunicationItemsRepository: Send + Sync {
    /// フィルタ付き一覧 (件数 + ページネーション)
    async fn list(
        &self,
        tenant_id: Uuid,
        is_active: Option<bool>,
        target_employee_id: Option<Uuid>,
        per_page: i64,
        offset: i64,
    ) -> Result<(Vec<CommunicationItemWithName>, i64), sqlx::Error>;

    /// 有効期間内のアクティブ一覧
    async fn list_active(
        &self,
        tenant_id: Uuid,
        target_employee_id: Option<Uuid>,
    ) -> Result<Vec<CommunicationItemWithName>, sqlx::Error>;

    /// ID で取得
    async fn get(
        &self,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<Option<CommunicationItem>, sqlx::Error>;

    /// 作成
    async fn create(
        &self,
        tenant_id: Uuid,
        input: &CreateCommunicationItem,
    ) -> Result<CommunicationItem, sqlx::Error>;

    /// 更新
    async fn update(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        input: &UpdateCommunicationItem,
    ) -> Result<Option<CommunicationItem>, sqlx::Error>;

    /// 削除。Returns true if a row was affected.
    async fn delete(&self, tenant_id: Uuid, id: Uuid) -> Result<bool, sqlx::Error>;
}

pub struct PgCommunicationItemsRepository {
    pool: PgPool,
}

impl PgCommunicationItemsRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl CommunicationItemsRepository for PgCommunicationItemsRepository {
    async fn list(
        &self,
        tenant_id: Uuid,
        is_active: Option<bool>,
        target_employee_id: Option<Uuid>,
        per_page: i64,
        offset: i64,
    ) -> Result<(Vec<CommunicationItemWithName>, i64), sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;

        let total: i64 = sqlx::query_scalar(
            r#"SELECT COUNT(*) FROM alc_api.communication_items c
               WHERE ($1::BOOLEAN IS NULL OR c.is_active = $1)
                 AND ($2::UUID IS NULL OR c.target_employee_id = $2 OR c.target_employee_id IS NULL)"#,
        )
        .bind(is_active)
        .bind(target_employee_id)
        .fetch_one(&mut *tc.conn)
        .await?;

        let items = sqlx::query_as::<_, CommunicationItemWithName>(
            r#"SELECT c.*, e.name AS target_employee_name
               FROM alc_api.communication_items c
               LEFT JOIN alc_api.employees e ON e.id = c.target_employee_id
               WHERE ($1::BOOLEAN IS NULL OR c.is_active = $1)
                 AND ($2::UUID IS NULL OR c.target_employee_id = $2 OR c.target_employee_id IS NULL)
               ORDER BY
                 CASE c.priority WHEN 'urgent' THEN 0 WHEN 'normal' THEN 1 ELSE 2 END,
                 c.created_at DESC
               LIMIT $3 OFFSET $4"#,
        )
        .bind(is_active)
        .bind(target_employee_id)
        .bind(per_page)
        .bind(offset)
        .fetch_all(&mut *tc.conn)
        .await?;

        Ok((items, total))
    }

    async fn list_active(
        &self,
        tenant_id: Uuid,
        target_employee_id: Option<Uuid>,
    ) -> Result<Vec<CommunicationItemWithName>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;

        sqlx::query_as::<_, CommunicationItemWithName>(
            r#"SELECT c.*, e.name AS target_employee_name
               FROM alc_api.communication_items c
               LEFT JOIN alc_api.employees e ON e.id = c.target_employee_id
               WHERE c.is_active = true
                 AND (c.effective_from IS NULL OR c.effective_from <= now())
                 AND (c.effective_until IS NULL OR c.effective_until >= now())
                 AND ($1::UUID IS NULL OR c.target_employee_id = $1 OR c.target_employee_id IS NULL)
               ORDER BY
                 CASE c.priority WHEN 'urgent' THEN 0 WHEN 'normal' THEN 1 ELSE 2 END,
                 c.created_at DESC"#,
        )
        .bind(target_employee_id)
        .fetch_all(&mut *tc.conn)
        .await
    }

    async fn get(
        &self,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<Option<CommunicationItem>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;

        sqlx::query_as::<_, CommunicationItem>(
            "SELECT * FROM alc_api.communication_items WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&mut *tc.conn)
        .await
    }

    async fn create(
        &self,
        tenant_id: Uuid,
        input: &CreateCommunicationItem,
    ) -> Result<CommunicationItem, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;

        sqlx::query_as::<_, CommunicationItem>(
            r#"INSERT INTO alc_api.communication_items
                   (tenant_id, title, content, priority, target_employee_id, effective_from, effective_until, created_by)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
               RETURNING *"#,
        )
        .bind(tenant_id)
        .bind(&input.title)
        .bind(input.content.as_deref().unwrap_or(""))
        .bind(input.priority.as_deref().unwrap_or("normal"))
        .bind(input.target_employee_id)
        .bind(input.effective_from)
        .bind(input.effective_until)
        .bind(&input.created_by)
        .fetch_one(&mut *tc.conn)
        .await
    }

    async fn update(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        input: &UpdateCommunicationItem,
    ) -> Result<Option<CommunicationItem>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;

        sqlx::query_as::<_, CommunicationItem>(
            r#"UPDATE alc_api.communication_items SET
                   title = COALESCE($1, title),
                   content = COALESCE($2, content),
                   priority = COALESCE($3, priority),
                   target_employee_id = CASE WHEN $5 THEN $4 ELSE target_employee_id END,
                   is_active = COALESCE($6, is_active),
                   effective_from = CASE WHEN $8 THEN $7 ELSE effective_from END,
                   effective_until = CASE WHEN $10 THEN $9 ELSE effective_until END,
                   updated_at = now()
               WHERE id = $11
               RETURNING *"#,
        )
        .bind(&input.title)
        .bind(&input.content)
        .bind(&input.priority)
        .bind(input.target_employee_id)
        .bind(input.target_employee_id.is_some())
        .bind(input.is_active)
        .bind(input.effective_from)
        .bind(input.effective_from.is_some())
        .bind(input.effective_until)
        .bind(input.effective_until.is_some())
        .bind(id)
        .fetch_optional(&mut *tc.conn)
        .await
    }

    async fn delete(&self, tenant_id: Uuid, id: Uuid) -> Result<bool, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;

        let result = sqlx::query("DELETE FROM alc_api.communication_items WHERE id = $1")
            .bind(id)
            .execute(&mut *tc.conn)
            .await?;
        Ok(result.rows_affected() > 0)
    }
}
