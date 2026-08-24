use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::models::{CreateTroubleTask, TroubleTask, UpdateTroubleTask};

#[derive(Debug, Default, Clone)]
pub struct TroubleTasksFilter {
    pub ticket_id: Option<Uuid>,
    pub status: Option<String>,
    pub task_type: Option<String>,
    pub assigned_to: Option<Uuid>,
    pub q: Option<String>,
    pub due_from: Option<DateTime<Utc>>,
    pub due_to: Option<DateTime<Utc>>,
    pub occurred_from: Option<DateTime<Utc>>,
    pub occurred_to: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy)]
pub enum TroubleTasksSortBy {
    CreatedAt,
    OccurredAt,
    DueDate,
    NextActionDue,
    Status,
}

impl TroubleTasksSortBy {
    pub fn column(self) -> &'static str {
        match self {
            Self::CreatedAt => "created_at",
            Self::OccurredAt => "occurred_at",
            Self::DueDate => "due_date",
            Self::NextActionDue => "next_action_due",
            Self::Status => "status",
        }
    }
}

#[async_trait]
pub trait TroubleTasksRepository: Send + Sync {
    async fn create(
        &self,
        tenant_id: Uuid,
        ticket_id: Uuid,
        created_by: Option<Uuid>,
        input: &CreateTroubleTask,
    ) -> Result<TroubleTask, sqlx::Error>;

    async fn list_by_ticket(
        &self,
        tenant_id: Uuid,
        ticket_id: Uuid,
    ) -> Result<Vec<TroubleTask>, sqlx::Error>;

    async fn get(&self, tenant_id: Uuid, id: Uuid) -> Result<Option<TroubleTask>, sqlx::Error>;

    async fn update(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        input: &UpdateTroubleTask,
    ) -> Result<Option<TroubleTask>, sqlx::Error>;

    async fn delete(&self, tenant_id: Uuid, id: Uuid) -> Result<bool, sqlx::Error>;

    /// `task_ids` に並べた順で `sort_order` を 0 起点に採番し直し、更新後の
    /// 一覧を返す。id が 1 つでもこのチケットに属さない (または重複している)
    /// 場合は何も更新せず `None` (Refs ippoan/nuxt-trouble#240)。
    async fn reorder(
        &self,
        tenant_id: Uuid,
        ticket_id: Uuid,
        task_ids: &[Uuid],
    ) -> Result<Option<Vec<TroubleTask>>, sqlx::Error>;

    async fn list_all(
        &self,
        tenant_id: Uuid,
        filter: &TroubleTasksFilter,
        sort_by: TroubleTasksSortBy,
        sort_desc: bool,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<TroubleTask>, sqlx::Error>;

    async fn count_all(
        &self,
        tenant_id: Uuid,
        filter: &TroubleTasksFilter,
    ) -> Result<i64, sqlx::Error>;
}
