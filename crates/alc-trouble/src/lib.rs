pub mod comments;
pub mod files;
pub mod repo;
pub mod tickets;
pub mod workflow;

use std::sync::Arc;

use alc_core::repository::{
    TroubleCommentsRepository, TroubleFilesRepository, TroubleTicketsRepository,
    TroubleWorkflowRepository,
};
use alc_core::storage::StorageBackend;
use alc_core::webhook::WebhookService;

/// trouble 用の最小 State。
/// モノリスでは `FromRef<AppState>` 経由で自動変換される。
#[derive(Clone)]
pub struct TroubleState {
    pub trouble_tickets: Arc<dyn TroubleTicketsRepository>,
    pub trouble_files: Arc<dyn TroubleFilesRepository>,
    pub trouble_workflow: Arc<dyn TroubleWorkflowRepository>,
    pub trouble_comments: Arc<dyn TroubleCommentsRepository>,
    pub trouble_storage: Option<Arc<dyn StorageBackend>>,
    pub webhook: Option<Arc<dyn WebhookService>>,
}

impl axum::extract::FromRef<alc_core::AppState> for TroubleState {
    fn from_ref(state: &alc_core::AppState) -> Self {
        Self {
            trouble_tickets: state.trouble_tickets.clone(),
            trouble_files: state.trouble_files.clone(),
            trouble_workflow: state.trouble_workflow.clone(),
            trouble_comments: state.trouble_comments.clone(),
            trouble_storage: state.trouble_storage.clone(),
            webhook: state.webhook.clone(),
        }
    }
}
