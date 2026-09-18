use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use crate::models::{CreateTroubleCategory, TroubleCategory};
use alc_core::master_data::{self, MasterCreateInput, MasterTable};

pub use crate::repository::trouble_categories::*;

// `TroubleCategory`/`CreateTroubleCategory` は trouble_task_types.rs (テーブルは
// 別の trouble_task_types だが同じ Row/Create 型を再利用している) からも使うが、
// この impl は crate 内のどこか 1 箇所にあれば十分 (Rust のトレイト解決は
// 定義モジュールを問わない)。
impl MasterCreateInput for CreateTroubleCategory {
    fn name(&self) -> &str {
        &self.name
    }

    fn sort_order(&self) -> Option<i32> {
        self.sort_order
    }
}

pub struct PgTroubleCategoriesRepository {
    pool: PgPool,
}

/// SQL に埋め込むテーブル名はこのリテラルだけ (Refs #651 — SQL injection の境界)。
impl MasterTable for PgTroubleCategoriesRepository {
    const TABLE: &'static str = "trouble_categories";
}

impl PgTroubleCategoriesRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl TroubleCategoriesRepository for PgTroubleCategoriesRepository {
    async fn list(&self, tenant_id: Uuid) -> Result<Vec<TroubleCategory>, sqlx::Error> {
        master_data::list::<Self, _>(&self.pool, tenant_id).await
    }

    async fn create(
        &self,
        tenant_id: Uuid,
        input: &CreateTroubleCategory,
    ) -> Result<TroubleCategory, sqlx::Error> {
        master_data::create::<Self, _, _>(&self.pool, tenant_id, input).await
    }

    async fn delete(&self, tenant_id: Uuid, id: Uuid) -> Result<bool, sqlx::Error> {
        master_data::delete::<Self>(&self.pool, tenant_id, id).await
    }

    async fn update_sort_order(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        sort_order: i32,
    ) -> Result<Option<TroubleCategory>, sqlx::Error> {
        master_data::update_sort_order::<Self, _>(&self.pool, tenant_id, id, sort_order).await
    }
}
