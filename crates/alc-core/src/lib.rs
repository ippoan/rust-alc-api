#[cfg(test)]
#[macro_use]
mod test_macros;

// Auth modules (formerly alc-auth crate)
pub mod auth_google;
pub mod auth_jwt;
pub mod auth_line;
pub mod auth_lineworks;
pub mod auth_middleware;

pub mod fcm;
pub mod middleware;
pub mod models;
pub mod repo;
pub mod repository;
pub mod storage;
pub mod tenant;
pub mod webhook;

use std::sync::Arc;

use repository::{
    AuthRepository, BotAdminRepository, CarInspectionRepository, CarinsFilesRepository,
    CarryingItemsRepository, CommunicationItemsRepository, DailyHealthRepository, DeviceRepository,
    DriverInfoRepository, DtakoCsvProxyRepository, DtakoDailyHoursRepository,
    DtakoDriversRepository, DtakoEventClassificationsRepository, DtakoLogsRepository,
    DtakoOperationsRepository, DtakoRestraintReportPdfRepository, DtakoRestraintReportRepository,
    DtakoScraperRepository, DtakoUploadRepository, DtakoVehiclesRepository,
    DtakoWorkTimesRepository, EmployeeRepository, EquipmentFailuresRepository,
    GuidanceRecordsRepository, HealthBaselinesRepository, ItemFilesRepository, ItemsRepository,
    MeasurementsRepository, NfcTagRepository, NotifyDeliveryRepository, NotifyDocumentRepository,
    NotifyLineConfigRepository, NotifyRecipientRepository, SsoAdminRepository,
    TenantUsersRepository, TenkoCallRepository, TenkoRecordsRepository, TenkoSchedulesRepository,
    TenkoSessionRepository, TenkoWebhooksRepository, TimecardRepository,
    TroubleCategoriesRepository, TroubleCommentsRepository, TroubleFilesRepository,
    TroubleOfficesRepository, TroubleTicketsRepository, TroubleWorkflowRepository,
};
use storage::StorageBackend;

#[derive(Clone)]
pub struct AppState {
    pub pool: Option<sqlx::PgPool>,
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
    pub dtako_logs: Arc<dyn DtakoLogsRepository>,
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
    pub items: Arc<dyn ItemsRepository>,
    pub item_files: Arc<dyn ItemFilesRepository>,
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
    pub webhook: Option<Arc<dyn webhook::WebhookService>>,
    pub notify_recipients: Arc<dyn NotifyRecipientRepository>,
    pub notify_documents: Arc<dyn NotifyDocumentRepository>,
    pub notify_deliveries: Arc<dyn NotifyDeliveryRepository>,
    pub notify_line_config: Arc<dyn NotifyLineConfigRepository>,
    pub notify_storage: Option<Arc<dyn StorageBackend>>,
    pub trouble_tickets: Arc<dyn TroubleTicketsRepository>,
    pub trouble_files: Arc<dyn TroubleFilesRepository>,
    pub trouble_workflow: Arc<dyn TroubleWorkflowRepository>,
    pub trouble_comments: Arc<dyn TroubleCommentsRepository>,
    pub trouble_categories: Arc<dyn TroubleCategoriesRepository>,
    pub trouble_offices: Arc<dyn TroubleOfficesRepository>,
    pub trouble_storage: Option<Arc<dyn StorageBackend>>,
}

impl AppState {
    /// pool が必要な統合テスト・本番コード用。None なら panic。
    pub fn pool(&self) -> &sqlx::PgPool {
        self.pool.as_ref().expect("PgPool is required but not set")
    }
}
