use async_trait::async_trait;
use uuid::Uuid;

use crate::models::{
    CreateWorkflowState, CreateWorkflowTransition, TroubleStatusHistory, TroubleWorkflowState,
    TroubleWorkflowTransition,
};

#[async_trait]
pub trait TroubleWorkflowRepository: Send + Sync {
    async fn list_states(&self, tenant_id: Uuid) -> Result<Vec<TroubleWorkflowState>, sqlx::Error>;

    async fn create_state(
        &self,
        tenant_id: Uuid,
        input: &CreateWorkflowState,
    ) -> Result<TroubleWorkflowState, sqlx::Error>;

    async fn delete_state(&self, tenant_id: Uuid, id: Uuid) -> Result<bool, sqlx::Error>;

    async fn list_transitions(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<TroubleWorkflowTransition>, sqlx::Error>;

    async fn create_transition(
        &self,
        tenant_id: Uuid,
        input: &CreateWorkflowTransition,
    ) -> Result<TroubleWorkflowTransition, sqlx::Error>;

    async fn delete_transition(&self, tenant_id: Uuid, id: Uuid) -> Result<bool, sqlx::Error>;

    async fn is_transition_allowed(
        &self,
        tenant_id: Uuid,
        from_state_id: Option<Uuid>,
        to_state_id: Uuid,
    ) -> Result<bool, sqlx::Error>;

    async fn get_initial_state(
        &self,
        tenant_id: Uuid,
    ) -> Result<Option<TroubleWorkflowState>, sqlx::Error>;

    async fn record_history(
        &self,
        tenant_id: Uuid,
        ticket_id: Uuid,
        from_state_id: Option<Uuid>,
        to_state_id: Uuid,
        changed_by: Option<Uuid>,
        comment: Option<String>,
    ) -> Result<TroubleStatusHistory, sqlx::Error>;

    async fn list_history(
        &self,
        tenant_id: Uuid,
        ticket_id: Uuid,
    ) -> Result<Vec<TroubleStatusHistory>, sqlx::Error>;

    async fn setup_defaults(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<TroubleWorkflowState>, sqlx::Error>;
}
