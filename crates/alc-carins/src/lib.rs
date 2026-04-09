pub mod car_inspection_files;
pub mod car_inspections;
pub mod carins_files;
pub mod nfc_tags;
pub mod repo;

use std::sync::Arc;

use alc_core::repository::car_inspections::CarInspectionRepository;
use alc_core::repository::carins_files::CarinsFilesRepository;
use alc_core::repository::nfc_tags::NfcTagRepository;
use alc_core::storage::StorageBackend;

/// carins-api 用の最小 State。
/// モノリスでは `FromRef<AppState>` 経由で自動変換される。
#[derive(Clone)]
pub struct CarinsState {
    pub car_inspections: Arc<dyn CarInspectionRepository>,
    pub carins_files: Arc<dyn CarinsFilesRepository>,
    pub nfc_tags: Arc<dyn NfcTagRepository>,
    pub storage: Arc<dyn StorageBackend>,
}

impl axum::extract::FromRef<alc_core::AppState> for CarinsState {
    fn from_ref(state: &alc_core::AppState) -> Self {
        Self {
            car_inspections: state.car_inspections.clone(),
            carins_files: state.carins_files.clone(),
            nfc_tags: state.nfc_tags.clone(),
            storage: state
                .carins_storage
                .clone()
                .unwrap_or_else(|| state.storage.clone()),
        }
    }
}
