use async_trait::async_trait;
use uuid::Uuid;

use crate::models::{
    CreateTroubleTicket, TroubleTicket, TroubleTicketFilter, TroubleTicketsResponse,
    UpdateTroubleTicket,
};

#[async_trait]
pub trait TroubleTicketsRepository: Send + Sync {
    async fn create(
        &self,
        tenant_id: Uuid,
        input: &CreateTroubleTicket,
        created_by: Option<Uuid>,
        initial_status_id: Option<Uuid>,
    ) -> Result<TroubleTicket, sqlx::Error>;

    async fn list(
        &self,
        tenant_id: Uuid,
        filter: &TroubleTicketFilter,
    ) -> Result<TroubleTicketsResponse, sqlx::Error>;

    async fn get(&self, tenant_id: Uuid, id: Uuid) -> Result<Option<TroubleTicket>, sqlx::Error>;

    async fn update(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        input: &UpdateTroubleTicket,
    ) -> Result<Option<TroubleTicket>, sqlx::Error>;

    async fn soft_delete(&self, tenant_id: Uuid, id: Uuid) -> Result<bool, sqlx::Error>;

    async fn update_status(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        status_id: Uuid,
    ) -> Result<Option<TroubleTicket>, sqlx::Error>;
}
