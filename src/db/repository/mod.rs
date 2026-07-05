// Re-export traits from alc-core
pub use alc_core::repository::{
    ApiTokensRepository, AuthRepository, BotAdminRepository, CarInspectionRepository,
    CarinsFilesRepository, CarryingItemsRepository, CommunicationItemsRepository, DeviceRepository,
    DtakoCsvProxyRepository, DtakoDailyHoursRepository, DtakoDriversRepository,
    DtakoEventClassificationsRepository, DtakoLogsRepository, DtakoOperationsRepository,
    DtakoRestraintReportPdfRepository, DtakoRestraintReportRepository, DtakoScraperRepository,
    DtakoTicketsRepository, DtakoUploadRepository, DtakoVehiclesRepository,
    DtakoWorkTimesRepository, DtakoYTimeExportRepository, EmployeeRepository,
    GuidanceRecordsRepository, ItemFilesRepository, ItemsRepository, LineworksChannelsRepository,
    MeasurementsRepository, NfcTagRepository, NotifyDeliveryRepository, NotifyDocumentRepository,
    NotifyGroupRepository, NotifyLineConfigRepository, NotifyRecipientRepository,
    SsoAdminRepository, TenantUsersRepository, TimecardRepository, VehicleSettingsDumpsRepository,
    WebhookRepository,
};

// Re-export trouble traits from alc-trouble (Refs #513 Phase B)
pub use alc_trouble::repository::{
    TroubleCategoriesRepository, TroubleFieldLayoutsRepository, TroubleFilesRepository,
    TroubleNotificationPrefsRepository, TroubleOfficesRepository,
    TroubleProgressStatusesRepository, TroubleSchedulesRepository, TroubleTaskStatusesRepository,
    TroubleTaskTypesRepository, TroubleTasksFilter, TroubleTasksRepository, TroubleTasksSortBy,
    TroubleTicketsRepository, TroubleWorkflowRepository,
};

// Re-export tenko traits from alc-tenko (Refs #513)
pub use alc_tenko::repository::{
    DailyHealthRepository, DriverInfoRepository, EquipmentFailuresRepository,
    HealthBaselinesRepository, TenkoCallRepository, TenkoRecordsRepository,
    TenkoSchedulesRepository, TenkoSessionRepository, TenkoWebhooksRepository,
};

// Re-export TenantConn from alc-core
pub use alc_core::tenant::TenantConn;

// Re-export submodules for backward compatibility (tests use repository::xxx::TypeName)
pub use alc_carins::repo::{car_inspections, carins_files, nfc_tags};
pub use alc_core::repo::auth;
pub use alc_core::repository::api_tokens;
pub use alc_devices::repo::devices;
pub use alc_dtako::repo::{
    dtako_csv_proxy, dtako_daily_hours, dtako_drivers, dtako_event_classifications, dtako_logs,
    dtako_operations, dtako_restraint_report, dtako_restraint_report_pdf, dtako_scraper,
    dtako_tickets, dtako_upload, dtako_vehicles, dtako_work_times, dtako_y_time_export,
    vehicle_settings_dumps,
};
pub use alc_misc::repo::{
    bot_admin, carrying_items, communication_items, employees, guidance_records, items,
    measurements, sso_admin, tenant_users, timecard, webhook,
};
pub use alc_tenko::repo::{
    daily_health, driver_info, equipment_failures, health_baselines, overdue, tenko_call,
    tenko_records, tenko_schedules, tenko_sessions, tenko_webhooks,
};

// Re-export notify submodules and Pg implementations
pub use alc_core::repository::{
    lineworks_channels, notify_deliveries, notify_documents, notify_groups, notify_line_config,
    notify_recipients,
};
pub use alc_notify::repo::{
    PgLineworksChannelsRepository, PgNotifyDeliveryRepository, PgNotifyDocumentRepository,
    PgNotifyGroupRepository, PgNotifyLineConfigRepository, PgNotifyRecipientRepository,
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
    PgDtakoScraperRepository, PgDtakoTicketsRepository, PgDtakoUploadRepository,
    PgDtakoVehiclesRepository, PgDtakoWorkTimesRepository, PgDtakoYTimeExportRepository,
    PgVehicleSettingsDumpsRepository,
};
pub use alc_misc::repo::{
    PgApiTokensRepository, PgBotAdminRepository, PgCarryingItemsRepository,
    PgCommunicationItemsRepository, PgEmployeeRepository, PgGuidanceRecordsRepository,
    PgItemFilesRepository, PgItemsRepository, PgMeasurementsRepository, PgSsoAdminRepository,
    PgTenantUsersRepository, PgTimecardRepository, PgWebhookRepository,
};
pub use alc_tenko::repo::{
    PgDailyHealthRepository, PgDriverInfoRepository, PgEquipmentFailuresRepository,
    PgHealthBaselinesRepository, PgTenkoCallRepository, PgTenkoOverdueRepository,
    PgTenkoRecordsRepository, PgTenkoSchedulesRepository, PgTenkoSessionRepository,
    PgTenkoWebhooksRepository,
};
pub use alc_trouble::repo::{
    trouble_categories::PgTroubleCategoriesRepository,
    trouble_field_layouts::PgTroubleFieldLayoutsRepository,
    trouble_files::PgTroubleFilesRepository,
    trouble_notification_prefs::PgTroubleNotificationPrefsRepository,
    trouble_offices::PgTroubleOfficesRepository,
    trouble_progress_statuses::PgTroubleProgressStatusesRepository,
    trouble_schedules::PgTroubleSchedulesRepository,
    trouble_task_statuses::PgTroubleTaskStatusesRepository,
    trouble_task_types::PgTroubleTaskTypesRepository, trouble_tasks::PgTroubleTasksRepository,
    trouble_tickets::PgTroubleTicketsRepository, trouble_workflow::PgTroubleWorkflowRepository,
};
