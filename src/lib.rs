#[cfg(test)]
#[macro_use]
mod test_macros;

pub mod auth;
pub mod compare;
pub mod csv_parser;
pub mod db;
pub mod fcm;
pub mod middleware;
pub mod routes;
pub mod storage;
pub mod webhook;

use std::sync::Arc;

use db::repository::{
    AuthRepository, BotAdminRepository, CarInspectionRepository, CarinsFilesRepository,
    CarryingItemsRepository, CommunicationItemsRepository, DailyHealthRepository, DeviceRepository,
    DriverInfoRepository, DtakoCsvProxyRepository, DtakoDailyHoursRepository,
    DtakoDriversRepository, DtakoEventClassificationsRepository, DtakoOperationsRepository,
    DtakoRestraintReportPdfRepository, DtakoRestraintReportRepository, DtakoScraperRepository,
    DtakoUploadRepository, DtakoVehiclesRepository, DtakoWorkTimesRepository, EmployeeRepository,
    EquipmentFailuresRepository, GuidanceRecordsRepository, HealthBaselinesRepository,
    MeasurementsRepository, NfcTagRepository, SsoAdminRepository, TenantUsersRepository,
    TenkoCallRepository, TenkoRecordsRepository, TenkoSchedulesRepository, TenkoSessionRepository,
    TenkoWebhooksRepository, TimecardRepository,
};
use storage::StorageBackend;

#[derive(Clone)]
pub struct AppState {
    pub pool: sqlx::PgPool,
    pub auth: Arc<dyn AuthRepository>,
    pub bot_admin: Arc<dyn BotAdminRepository>,
    pub car_inspections: Arc<dyn CarInspectionRepository>,
    pub carins_files: Arc<dyn CarinsFilesRepository>,
    pub carrying_items: Arc<dyn CarryingItemsRepository>,
    pub communication_items: Arc<dyn CommunicationItemsRepository>,
    pub daily_health: Arc<dyn DailyHealthRepository>,
    pub devices: Arc<dyn DeviceRepository>,
    pub driver_info: Arc<dyn DriverInfoRepository>,
    pub dtako_csv_proxy: Arc<dyn DtakoCsvProxyRepository>,
    pub dtako_daily_hours: Arc<dyn DtakoDailyHoursRepository>,
    pub dtako_drivers: Arc<dyn DtakoDriversRepository>,
    pub dtako_event_classifications: Arc<dyn DtakoEventClassificationsRepository>,
    pub dtako_operations: Arc<dyn DtakoOperationsRepository>,
    pub dtako_restraint_report: Arc<dyn DtakoRestraintReportRepository>,
    pub dtako_restraint_report_pdf: Arc<dyn DtakoRestraintReportPdfRepository>,
    pub dtako_scraper: Arc<dyn DtakoScraperRepository>,
    pub dtako_upload: Arc<dyn DtakoUploadRepository>,
    pub dtako_vehicles: Arc<dyn DtakoVehiclesRepository>,
    pub dtako_work_times: Arc<dyn DtakoWorkTimesRepository>,
    pub employees: Arc<dyn EmployeeRepository>,
    pub equipment_failures: Arc<dyn EquipmentFailuresRepository>,
    pub guidance_records: Arc<dyn GuidanceRecordsRepository>,
    pub health_baselines: Arc<dyn HealthBaselinesRepository>,
    pub measurements: Arc<dyn MeasurementsRepository>,
    pub nfc_tags: Arc<dyn NfcTagRepository>,
    pub sso_admin: Arc<dyn SsoAdminRepository>,
    pub tenant_users: Arc<dyn TenantUsersRepository>,
    pub tenko_call: Arc<dyn TenkoCallRepository>,
    pub tenko_records: Arc<dyn TenkoRecordsRepository>,
    pub tenko_schedules: Arc<dyn TenkoSchedulesRepository>,
    pub tenko_sessions: Arc<dyn TenkoSessionRepository>,
    pub tenko_webhooks: Arc<dyn TenkoWebhooksRepository>,
    pub timecard: Arc<dyn TimecardRepository>,
    pub storage: Arc<dyn StorageBackend>,
    pub carins_storage: Option<Arc<dyn StorageBackend>>,
    pub dtako_storage: Option<Arc<dyn StorageBackend>>,
    pub fcm: Option<Arc<dyn fcm::FcmSenderTrait>>,
}
