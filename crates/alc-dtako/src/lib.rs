#[cfg(test)]
#[macro_use]
mod test_macros;

pub mod repo;

pub mod dtako_restraint_report;
pub mod dtako_restraint_report_pdf;
pub mod dtako_tickets;
pub mod dtako_y_time_export;

use std::sync::Arc;

use alc_core::repository::{
    DtakoRestraintReportPdfRepository, DtakoRestraintReportRepository, DtakoTicketsRepository,
    DtakoYTimeExportRepository,
};
use alc_core::storage::StorageBackend;

/// dtako-api 用の最小 State。
/// モノリスでは `FromRef<AppState>` 経由で自動変換される。
#[derive(Clone)]
pub struct DtakoState {
    pub dtako_restraint_report: Arc<dyn DtakoRestraintReportRepository>,
    pub dtako_restraint_report_pdf: Arc<dyn DtakoRestraintReportPdfRepository>,
    pub dtako_tickets: Arc<dyn DtakoTicketsRepository>,
    pub dtako_y_time_export: Arc<dyn DtakoYTimeExportRepository>,
    pub dtako_storage: Option<Arc<dyn StorageBackend>>,
}

impl axum::extract::FromRef<alc_core::AppState> for DtakoState {
    fn from_ref(state: &alc_core::AppState) -> Self {
        Self {
            dtako_restraint_report: state.dtako_restraint_report.clone(),
            dtako_restraint_report_pdf: state.dtako_restraint_report_pdf.clone(),
            dtako_tickets: state.dtako_tickets.clone(),
            dtako_y_time_export: state.dtako_y_time_export.clone(),
            dtako_storage: state.dtako_storage.clone(),
        }
    }
}
