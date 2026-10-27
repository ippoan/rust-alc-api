// Re-export traits from alc-core
pub use alc_core::repository::{
    AuthRepository, BotAdminRepository, CarInspectionRepository, CarinsFilesRepository,
    DeviceRepository, DtakoRestraintReportPdfRepository, DtakoRestraintReportRepository,
    DtakoTicketsRepository, HubMeasurementsRepository, LineworksChannelsRepository,
    NfcTagRepository, NotifyDeliveryRepository, NotifyDocumentRepository,
    NotifyRecipientRepository, SsoAdminRepository, TenantUsersRepository, TimecardRepository,
    WebhookRepository,
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
pub use alc_core::repository::hub_measurements;
pub use alc_devices::repo::devices;
pub use alc_dtako::repo::{dtako_restraint_report, dtako_restraint_report_pdf, dtako_tickets};
pub use alc_misc::repo::{bot_admin, sso_admin, tenant_users, timecard, webhook};
pub use alc_tenko::repo::{
    daily_health, driver_info, equipment_failures, health_baselines, overdue, tenko_call,
    tenko_records, tenko_schedules, tenko_sessions, tenko_webhooks,
};

// Re-export notify submodules and Pg implementations
pub use alc_core::repository::{
    lineworks_channels, notify_deliveries, notify_documents, notify_recipients,
};
pub use alc_notify::repo::{
    PgLineworksChannelsRepository, PgNotifyDeliveryRepository, PgNotifyDocumentRepository,
    PgNotifyRecipientRepository,
};

// Re-export Pg implementations
pub use alc_carins::repo::{
    PgCarInspectionRepository, PgCarinsFilesRepository, PgNfcTagRepository,
};
pub use alc_core::repo::PgAuthRepository;
pub use alc_devices::repo::PgDeviceRepository;
pub use alc_devices::repo::PgHubMeasurementsRepository;
pub use alc_dtako::repo::{
    PgDtakoRestraintReportPdfRepository, PgDtakoRestraintReportRepository, PgDtakoTicketsRepository,
};
pub use alc_misc::repo::{
    PgBotAdminRepository, PgSsoAdminRepository, PgTenantUsersRepository, PgTimecardRepository,
    PgWebhookRepository,
};
pub use alc_tenko::repo::{
    PgDailyHealthRepository, PgDriverInfoRepository, PgEquipmentFailuresRepository,
    PgHealthBaselinesRepository, PgTenkoCallRepository, PgTenkoOverdueRepository,
    PgTenkoRecordsRepository, PgTenkoSchedulesRepository, PgTenkoSessionRepository,
    PgTenkoWebhooksRepository,
};
