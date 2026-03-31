pub mod auth;
pub mod bot_admin;
pub mod car_inspections;
pub mod carins_files;
pub mod carrying_items;
pub mod communication_items;
pub mod daily_health;
pub mod devices;
pub mod driver_info;
pub mod dtako_csv_proxy;
pub mod dtako_daily_hours;
pub mod dtako_drivers;
pub mod dtako_event_classifications;
pub mod dtako_operations;
pub mod dtako_restraint_report;
pub mod dtako_restraint_report_pdf;
pub mod dtako_scraper;
pub mod dtako_upload;
pub mod dtako_vehicles;
pub mod dtako_work_times;
pub mod employees;
pub mod equipment_failures;
pub mod guidance_records;
pub mod health_baselines;
pub mod measurements;
pub mod nfc_tags;
pub mod sso_admin;
pub mod tenant_users;
pub mod tenko_call;
pub mod tenko_records;
pub mod tenko_schedules;
pub mod tenko_sessions;
pub mod tenko_webhooks;
pub mod timecard;
pub mod webhook;

// Re-export traits from alc-core
pub use alc_core::repository::{
    AuthRepository, BotAdminRepository, CarInspectionRepository, CarinsFilesRepository,
    CarryingItemsRepository, CommunicationItemsRepository, DailyHealthRepository, DeviceRepository,
    DriverInfoRepository, DtakoCsvProxyRepository, DtakoDailyHoursRepository,
    DtakoDriversRepository, DtakoEventClassificationsRepository, DtakoOperationsRepository,
    DtakoRestraintReportPdfRepository, DtakoRestraintReportRepository, DtakoScraperRepository,
    DtakoUploadRepository, DtakoVehiclesRepository, DtakoWorkTimesRepository, EmployeeRepository,
    EquipmentFailuresRepository, GuidanceRecordsRepository, HealthBaselinesRepository,
    MeasurementsRepository, NfcTagRepository, SsoAdminRepository, TenantUsersRepository,
    TenkoCallRepository, TenkoRecordsRepository, TenkoSchedulesRepository, TenkoSessionRepository,
    TenkoWebhooksRepository, TimecardRepository, WebhookRepository,
};

// Re-export TenantConn from alc-core
pub use alc_core::tenant::TenantConn;

// Re-export Pg implementations
pub use auth::PgAuthRepository;
pub use bot_admin::PgBotAdminRepository;
pub use car_inspections::PgCarInspectionRepository;
pub use carins_files::PgCarinsFilesRepository;
pub use carrying_items::PgCarryingItemsRepository;
pub use communication_items::PgCommunicationItemsRepository;
pub use daily_health::PgDailyHealthRepository;
pub use devices::PgDeviceRepository;
pub use driver_info::PgDriverInfoRepository;
pub use dtako_csv_proxy::PgDtakoCsvProxyRepository;
pub use dtako_daily_hours::PgDtakoDailyHoursRepository;
pub use dtako_drivers::PgDtakoDriversRepository;
pub use dtako_event_classifications::PgDtakoEventClassificationsRepository;
pub use dtako_operations::PgDtakoOperationsRepository;
pub use dtako_restraint_report::PgDtakoRestraintReportRepository;
pub use dtako_restraint_report_pdf::PgDtakoRestraintReportPdfRepository;
pub use dtako_scraper::PgDtakoScraperRepository;
pub use dtako_upload::PgDtakoUploadRepository;
pub use dtako_vehicles::PgDtakoVehiclesRepository;
pub use dtako_work_times::PgDtakoWorkTimesRepository;
pub use employees::PgEmployeeRepository;
pub use equipment_failures::PgEquipmentFailuresRepository;
pub use guidance_records::PgGuidanceRecordsRepository;
pub use health_baselines::PgHealthBaselinesRepository;
pub use measurements::PgMeasurementsRepository;
pub use nfc_tags::PgNfcTagRepository;
pub use sso_admin::PgSsoAdminRepository;
pub use tenant_users::PgTenantUsersRepository;
pub use tenko_call::PgTenkoCallRepository;
pub use tenko_records::PgTenkoRecordsRepository;
pub use tenko_schedules::PgTenkoSchedulesRepository;
pub use tenko_sessions::PgTenkoSessionRepository;
pub use tenko_webhooks::PgTenkoWebhooksRepository;
pub use timecard::PgTimecardRepository;
pub use webhook::PgWebhookRepository;
