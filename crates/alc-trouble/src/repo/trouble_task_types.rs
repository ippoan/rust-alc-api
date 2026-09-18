use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use crate::models::{CreateTroubleCategory, TroubleCategory};
use alc_core::master_data::{self, MasterTable};

pub use crate::repository::trouble_task_types::*;

pub struct PgTroubleTaskTypesRepository {
    pool: PgPool,
}

/// SQL に埋め込むテーブル名はこのリテラルだけ (Refs #651 — SQL injection の境界)。
/// Row/Create 型は `trouble_categories.rs` と同じ `TroubleCategory`/
/// `CreateTroubleCategory` を再利用している (テーブルは別の trouble_task_types) —
/// `MasterCreateInput for CreateTroubleCategory` の impl もそちら 1 箇所のみ。
impl MasterTable for PgTroubleTaskTypesRepository {
    const TABLE: &'static str = "trouble_task_types";
}

impl PgTroubleTaskTypesRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl TroubleTaskTypesRepository for PgTroubleTaskTypesRepository {
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
