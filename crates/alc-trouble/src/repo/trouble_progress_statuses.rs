use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use crate::models::{CreateTroubleProgressStatus, TroubleProgressStatus};
use alc_core::master_data::{self, MasterCreateInput, MasterRow, MasterTable};

pub use crate::repository::trouble_progress_statuses::*;

impl MasterCreateInput for CreateTroubleProgressStatus {
    fn name(&self) -> &str {
        &self.name
    }

    fn sort_order(&self) -> Option<i32> {
        self.sort_order
    }
}

/// `tests/mock_helpers` の Mock 実装 (in-memory) が list/create/update_sort_order
/// を共通ロジックに寄せるための実装 (Refs #651)。
impl MasterRow for TroubleProgressStatus {
    fn master_id(&self) -> Uuid {
        self.id
    }

    fn set_master_sort_order(&mut self, sort_order: i32) {
        self.sort_order = sort_order;
    }

    fn new_master_row(
        id: Uuid,
        tenant_id: Uuid,
        name: String,
        sort_order: i32,
        created_at: chrono::DateTime<chrono::Utc>,
    ) -> Self {
        Self {
            id,
            tenant_id,
            name,
            sort_order,
            created_at,
        }
    }
}

pub struct PgTroubleProgressStatusesRepository {
    pool: PgPool,
}

/// SQL に埋め込むテーブル名はこのリテラルだけ (Refs #651 — SQL injection の境界)。
impl MasterTable for PgTroubleProgressStatusesRepository {
    const TABLE: &'static str = "trouble_progress_statuses";
}

impl PgTroubleProgressStatusesRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl TroubleProgressStatusesRepository for PgTroubleProgressStatusesRepository {
    async fn list(&self, tenant_id: Uuid) -> Result<Vec<TroubleProgressStatus>, sqlx::Error> {
        master_data::list::<Self, _>(&self.pool, tenant_id).await
    }

    async fn create(
        &self,
        tenant_id: Uuid,
        input: &CreateTroubleProgressStatus,
    ) -> Result<TroubleProgressStatus, sqlx::Error> {
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
    ) -> Result<Option<TroubleProgressStatus>, sqlx::Error> {
        master_data::update_sort_order::<Self, _>(&self.pool, tenant_id, id, sort_order).await
    }
}
