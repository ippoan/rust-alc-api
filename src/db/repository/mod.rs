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

pub use auth::{AuthRepository, PgAuthRepository};
pub use bot_admin::{BotAdminRepository, PgBotAdminRepository};
pub use car_inspections::{CarInspectionRepository, PgCarInspectionRepository};
pub use carins_files::{CarinsFilesRepository, PgCarinsFilesRepository};
pub use carrying_items::{CarryingItemsRepository, PgCarryingItemsRepository};
pub use communication_items::{CommunicationItemsRepository, PgCommunicationItemsRepository};
pub use daily_health::{DailyHealthRepository, PgDailyHealthRepository};
pub use devices::{DeviceRepository, PgDeviceRepository};
pub use driver_info::{DriverInfoRepository, PgDriverInfoRepository};
pub use dtako_csv_proxy::{DtakoCsvProxyRepository, PgDtakoCsvProxyRepository};
pub use dtako_daily_hours::{DtakoDailyHoursRepository, PgDtakoDailyHoursRepository};
pub use dtako_drivers::{DtakoDriversRepository, PgDtakoDriversRepository};
pub use dtako_event_classifications::{
    DtakoEventClassificationsRepository, PgDtakoEventClassificationsRepository,
};
pub use dtako_operations::{DtakoOperationsRepository, PgDtakoOperationsRepository};
pub use dtako_restraint_report::{
    DtakoRestraintReportRepository, PgDtakoRestraintReportRepository,
};
pub use dtako_restraint_report_pdf::{
    DtakoRestraintReportPdfRepository, PgDtakoRestraintReportPdfRepository,
};
pub use dtako_scraper::{DtakoScraperRepository, PgDtakoScraperRepository};
pub use dtako_upload::{DtakoUploadRepository, PgDtakoUploadRepository};
pub use dtako_vehicles::{DtakoVehiclesRepository, PgDtakoVehiclesRepository};
pub use dtako_work_times::{DtakoWorkTimesRepository, PgDtakoWorkTimesRepository};
pub use employees::{EmployeeRepository, PgEmployeeRepository};
pub use equipment_failures::{EquipmentFailuresRepository, PgEquipmentFailuresRepository};
pub use guidance_records::{GuidanceRecordsRepository, PgGuidanceRecordsRepository};
pub use health_baselines::{HealthBaselinesRepository, PgHealthBaselinesRepository};
pub use measurements::{MeasurementsRepository, PgMeasurementsRepository};
pub use nfc_tags::{NfcTagRepository, PgNfcTagRepository};
pub use sso_admin::{PgSsoAdminRepository, SsoAdminRepository};
pub use tenant_users::{PgTenantUsersRepository, TenantUsersRepository};
pub use tenko_call::{PgTenkoCallRepository, TenkoCallRepository};
pub use tenko_records::{PgTenkoRecordsRepository, TenkoRecordsRepository};
pub use tenko_schedules::{PgTenkoSchedulesRepository, TenkoSchedulesRepository};
pub use tenko_sessions::{PgTenkoSessionRepository, TenkoSessionRepository};
pub use tenko_webhooks::{PgTenkoWebhooksRepository, TenkoWebhooksRepository};
pub use timecard::{PgTimecardRepository, TimecardRepository};

use sqlx::PgPool;

/// テナントスコープの DB コネクション
/// acquire 時に set_current_tenant を自動呼び出しする
pub struct TenantConn {
    pub conn: sqlx::pool::PoolConnection<sqlx::Postgres>,
}

impl TenantConn {
    pub async fn acquire(pool: &PgPool, tenant_id: &str) -> Result<Self, sqlx::Error> {
        let mut conn = pool.acquire().await?;
        super::tenant::set_current_tenant(&mut conn, tenant_id).await?;
        Ok(Self { conn })
    }
}
