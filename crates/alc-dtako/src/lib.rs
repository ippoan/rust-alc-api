pub mod repo;

pub mod dtako_tickets;

use std::sync::Arc;

use alc_core::repository::DtakoTicketsRepository;
use alc_core::storage::StorageBackend;

/// dtako-api 用の最小 State。
/// モノリスでは `FromRef<AppState>` 経由で自動変換される。
#[derive(Clone)]
pub struct DtakoState {
    pub dtako_tickets: Arc<dyn DtakoTicketsRepository>,
    pub dtako_storage: Option<Arc<dyn StorageBackend>>,
}

impl axum::extract::FromRef<alc_core::AppState> for DtakoState {
    fn from_ref(state: &alc_core::AppState) -> Self {
        Self {
            dtako_tickets: state.dtako_tickets.clone(),
            dtako_storage: state.dtako_storage.clone(),
        }
    }
}
