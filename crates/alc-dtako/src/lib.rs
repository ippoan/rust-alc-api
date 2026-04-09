#[cfg(test)]
#[macro_use]
mod test_macros;

pub mod archive_reader;
pub mod repo;

pub mod dtako_csv_proxy;
pub mod dtako_daily_hours;
pub mod dtako_drivers;
pub mod dtako_event_classifications;
pub mod dtako_logs;
pub mod dtako_operations;
pub mod dtako_restraint_report;
pub mod dtako_restraint_report_pdf;
pub mod dtako_scraper;
pub mod dtako_upload;
pub mod dtako_vehicles;
pub mod dtako_work_times;

use std::sync::Arc;

use alc_core::repository::{
    DtakoCsvProxyRepository, DtakoDailyHoursRepository, DtakoDriversRepository,
    DtakoEventClassificationsRepository, DtakoLogsRepository, DtakoOperationsRepository,
    DtakoRestraintReportPdfRepository, DtakoRestraintReportRepository, DtakoScraperRepository,
    DtakoUploadRepository, DtakoVehiclesRepository, DtakoWorkTimesRepository,
};
use alc_core::storage::StorageBackend;

/// dtako-api 用の最小 State。
/// モノリスでは `FromRef<AppState>` 経由で自動変換される。
#[derive(Clone)]
pub struct DtakoState {
    pub dtako_csv_proxy: Arc<dyn DtakoCsvProxyRepository>,
    pub dtako_daily_hours: Arc<dyn DtakoDailyHoursRepository>,
    pub dtako_drivers: Arc<dyn DtakoDriversRepository>,
    pub dtako_event_classifications: Arc<dyn DtakoEventClassificationsRepository>,
    pub dtako_logs: Arc<dyn DtakoLogsRepository>,
    pub dtako_operations: Arc<dyn DtakoOperationsRepository>,
    pub dtako_restraint_report: Arc<dyn DtakoRestraintReportRepository>,
    pub dtako_restraint_report_pdf: Arc<dyn DtakoRestraintReportPdfRepository>,
    pub dtako_scraper: Arc<dyn DtakoScraperRepository>,
    pub dtako_upload: Arc<dyn DtakoUploadRepository>,
    pub dtako_vehicles: Arc<dyn DtakoVehiclesRepository>,
    pub dtako_work_times: Arc<dyn DtakoWorkTimesRepository>,
    pub dtako_storage: Option<Arc<dyn StorageBackend>>,
}

impl axum::extract::FromRef<alc_core::AppState> for DtakoState {
    fn from_ref(state: &alc_core::AppState) -> Self {
        Self {
            dtako_csv_proxy: state.dtako_csv_proxy.clone(),
            dtako_daily_hours: state.dtako_daily_hours.clone(),
            dtako_drivers: state.dtako_drivers.clone(),
            dtako_event_classifications: state.dtako_event_classifications.clone(),
            dtako_logs: state.dtako_logs.clone(),
            dtako_operations: state.dtako_operations.clone(),
            dtako_restraint_report: state.dtako_restraint_report.clone(),
            dtako_restraint_report_pdf: state.dtako_restraint_report_pdf.clone(),
            dtako_scraper: state.dtako_scraper.clone(),
            dtako_upload: state.dtako_upload.clone(),
            dtako_vehicles: state.dtako_vehicles.clone(),
            dtako_work_times: state.dtako_work_times.clone(),
            dtako_storage: state.dtako_storage.clone(),
        }
    }
}
