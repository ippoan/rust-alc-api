// Re-export traits from alc-core
pub use alc_core::repository::{
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
    TenkoSessionRepository, TenkoWebhooksRepository, TimecardRepository, TroubleCommentsRepository,
    TroubleFilesRepository, TroubleTicketsRepository, TroubleWorkflowRepository, WebhookRepository,
};

// Re-export TenantConn from alc-core
pub use alc_core::tenant::TenantConn;

// Re-export submodules for backward compatibility (tests use repository::xxx::TypeName)
pub use alc_carins::repo::{car_inspections, carins_files, nfc_tags};
pub use alc_core::repo::auth;
pub use alc_devices::repo::devices;
pub use alc_dtako::repo::{
    dtako_csv_proxy, dtako_daily_hours, dtako_drivers, dtako_event_classifications, dtako_logs,
    dtako_operations, dtako_restraint_report, dtako_restraint_report_pdf, dtako_scraper,
    dtako_upload, dtako_vehicles, dtako_work_times,
};
pub use alc_misc::repo::{
    bot_admin, carrying_items, communication_items, driver_info, employees, guidance_records,
    items, measurements, sso_admin, tenant_users, timecard, webhook,
};
pub use alc_tenko::repo::{
    daily_health, equipment_failures, health_baselines, tenko_call, tenko_records, tenko_schedules,
    tenko_sessions, tenko_webhooks,
};

// Re-export notify submodules and Pg implementations
pub use alc_core::repository::{
    notify_deliveries, notify_documents, notify_line_config, notify_recipients,
};
pub use alc_notify::repo::{
    PgNotifyDeliveryRepository, PgNotifyDocumentRepository, PgNotifyLineConfigRepository,
    PgNotifyRecipientRepository,
};

// Re-export Pg implementations
pub use alc_carins::repo::{
    PgCarInspectionRepository, PgCarinsFilesRepository, PgNfcTagRepository,
};
pub use alc_core::repo::PgAuthRepository;
pub use alc_devices::repo::PgDeviceRepository;
pub use alc_dtako::repo::{
    PgDtakoCsvProxyRepository, PgDtakoDailyHoursRepository, PgDtakoDriversRepository,
    PgDtakoEventClassificationsRepository, PgDtakoLogsRepository, PgDtakoOperationsRepository,
    PgDtakoRestraintReportPdfRepository, PgDtakoRestraintReportRepository,
    PgDtakoScraperRepository, PgDtakoUploadRepository, PgDtakoVehiclesRepository,
    PgDtakoWorkTimesRepository,
};
pub use alc_misc::repo::{
    PgBotAdminRepository, PgCarryingItemsRepository, PgCommunicationItemsRepository,
    PgDriverInfoRepository, PgEmployeeRepository, PgGuidanceRecordsRepository,
    PgItemFilesRepository, PgItemsRepository, PgMeasurementsRepository, PgSsoAdminRepository,
    PgTenantUsersRepository, PgTimecardRepository, PgWebhookRepository,
};
pub use alc_tenko::repo::{
    PgDailyHealthRepository, PgEquipmentFailuresRepository, PgHealthBaselinesRepository,
    PgTenkoCallRepository, PgTenkoRecordsRepository, PgTenkoSchedulesRepository,
    PgTenkoSessionRepository, PgTenkoWebhooksRepository,
};
pub use alc_trouble::repo::{
    trouble_comments::PgTroubleCommentsRepository, trouble_files::PgTroubleFilesRepository,
    trouble_tickets::PgTroubleTicketsRepository, trouble_workflow::PgTroubleWorkflowRepository,
};
