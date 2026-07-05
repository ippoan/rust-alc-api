#[cfg(test)]
#[macro_use]
mod test_macros;

// Auth modules (formerly alc-auth crate)
pub mod auth_google;
pub mod auth_jwt;
pub mod auth_lineworks;
pub mod auth_middleware;

pub mod constant_time;
pub mod device_pair_client;
pub mod fcm;
pub mod middleware;
pub mod models;
pub mod realtime_bus;
pub mod redact_broadcast;
pub mod repo;
pub mod repository;
pub mod serde_helpers;
pub mod storage;
pub mod tenant;
pub mod webhook;

use std::sync::Arc;

use repository::{
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
    SsoAdminRepository, TenantUsersRepository, TimecardRepository, TroubleCategoriesRepository,
    TroubleFieldLayoutsRepository, TroubleFilesRepository, TroubleNotificationPrefsRepository,
    TroubleOfficesRepository, TroubleProgressStatusesRepository, TroubleSchedulesRepository,
    TroubleTaskStatusesRepository, TroubleTaskTypesRepository, TroubleTasksRepository,
    TroubleTicketsRepository, TroubleWorkflowRepository, VehicleSettingsDumpsRepository,
};
use storage::StorageBackend;

#[derive(Clone)]
pub struct AppState {
    pub pool: Option<sqlx::PgPool>,
    pub api_tokens: Arc<dyn ApiTokensRepository>,
    pub auth: Arc<dyn AuthRepository>,
    pub bot_admin: Arc<dyn BotAdminRepository>,
    pub car_inspections: Arc<dyn CarInspectionRepository>,
    pub carins_files: Arc<dyn CarinsFilesRepository>,
    pub carrying_items: Arc<dyn CarryingItemsRepository>,
    pub communication_items: Arc<dyn CommunicationItemsRepository>,
    pub devices: Arc<dyn DeviceRepository>,
    pub dtako_csv_proxy: Arc<dyn DtakoCsvProxyRepository>,
    pub dtako_daily_hours: Arc<dyn DtakoDailyHoursRepository>,
    pub dtako_logs: Arc<dyn DtakoLogsRepository>,
    pub dtako_drivers: Arc<dyn DtakoDriversRepository>,
    pub dtako_event_classifications: Arc<dyn DtakoEventClassificationsRepository>,
    pub dtako_operations: Arc<dyn DtakoOperationsRepository>,
    pub dtako_restraint_report: Arc<dyn DtakoRestraintReportRepository>,
    pub dtako_restraint_report_pdf: Arc<dyn DtakoRestraintReportPdfRepository>,
    pub dtako_scraper: Arc<dyn DtakoScraperRepository>,
    pub dtako_tickets: Arc<dyn DtakoTicketsRepository>,
    pub dtako_upload: Arc<dyn DtakoUploadRepository>,
    pub dtako_vehicles: Arc<dyn DtakoVehiclesRepository>,
    pub dtako_work_times: Arc<dyn DtakoWorkTimesRepository>,
    pub dtako_y_time_export: Arc<dyn DtakoYTimeExportRepository>,
    pub vehicle_settings_dumps: Arc<dyn VehicleSettingsDumpsRepository>,
    pub employees: Arc<dyn EmployeeRepository>,
    pub guidance_records: Arc<dyn GuidanceRecordsRepository>,
    pub items: Arc<dyn ItemsRepository>,
    pub item_files: Arc<dyn ItemFilesRepository>,
    pub measurements: Arc<dyn MeasurementsRepository>,
    pub nfc_tags: Arc<dyn NfcTagRepository>,
    pub sso_admin: Arc<dyn SsoAdminRepository>,
    pub tenant_users: Arc<dyn TenantUsersRepository>,
    pub timecard: Arc<dyn TimecardRepository>,
    pub storage: Arc<dyn StorageBackend>,
    pub carins_storage: Option<Arc<dyn StorageBackend>>,
    pub dtako_storage: Option<Arc<dyn StorageBackend>>,
    pub fcm: Option<Arc<dyn fcm::FcmSenderTrait>>,
    pub webhook: Option<Arc<dyn webhook::WebhookService>>,
    pub notify_recipients: Arc<dyn NotifyRecipientRepository>,
    pub notify_groups: Arc<dyn NotifyGroupRepository>,
    pub notify_documents: Arc<dyn NotifyDocumentRepository>,
    pub notify_deliveries: Arc<dyn NotifyDeliveryRepository>,
    pub notify_line_config: Arc<dyn NotifyLineConfigRepository>,
    pub lineworks_channels: Arc<dyn LineworksChannelsRepository>,
    pub notify_storage: Option<Arc<dyn StorageBackend>>,
    /// `RedactBroadcaster::from_env()` で env vars (`NOTIFY_REDACT_BROADCAST_URL` /
    /// `NOTIFY_REDACT_BROADCAST_SECRET`) が揃った時のみ Some。background_redaction.rs
    /// が terminal 状態で呼び、Cloudflare Worker (notify-realtime-bus) の DO 経由で
    /// admin ブラウザに WS push する。未設定なら no-op (Phase 3 デプロイ前は空でも安全)。
    pub redact_broadcaster: Option<Arc<redact_broadcast::RedactBroadcaster>>,
    /// 任意の Serialize ペイロードを同 notify-realtime-bus Worker に流す汎用クライアント。
    /// `RedactBroadcaster` と同じ env vars / 同じ Worker / 同じ secret を共有するが、
    /// payload 型を選ばないので redact 以外の async job 完了通知 (Y時間 export 等)
    /// にも使える。env vars 未設定なら None で no-op (POST /jobs 系は 503 返却で
    /// 検知すること、silent に compute しないため)。
    pub realtime_bus: Option<Arc<realtime_bus::RealtimeBus>>,
    pub trouble_tickets: Arc<dyn TroubleTicketsRepository>,
    pub trouble_files: Arc<dyn TroubleFilesRepository>,
    pub trouble_workflow: Arc<dyn TroubleWorkflowRepository>,
    pub trouble_categories: Arc<dyn TroubleCategoriesRepository>,
    pub trouble_offices: Arc<dyn TroubleOfficesRepository>,
    pub trouble_progress_statuses: Arc<dyn TroubleProgressStatusesRepository>,
    pub trouble_notification_prefs: Arc<dyn TroubleNotificationPrefsRepository>,
    pub trouble_schedules: Arc<dyn TroubleSchedulesRepository>,
    pub trouble_tasks: Arc<dyn TroubleTasksRepository>,
    pub trouble_task_types: Arc<dyn TroubleTaskTypesRepository>,
    pub trouble_task_statuses: Arc<dyn TroubleTaskStatusesRepository>,
    pub trouble_field_layouts: Arc<dyn TroubleFieldLayoutsRepository>,
    pub trouble_storage: Option<Arc<dyn StorageBackend>>,
    /// kiosk 端末 re-pair (再認証) 用、auth-worker `/device/pair-internal` 呼び出し
    /// クライアント (Refs #495)。`AUTH_WORKER_URL` / `INTERNAL_SHARED_SECRET`
    /// が未設定なら None (re-pair endpoint は 404 で "not configured" を表す)。
    /// shared secret は既存 INTERNAL_SHARED_SECRET (dtako ingest 等と共用) を再利用する。
    pub device_pair_client: Option<Arc<dyn device_pair_client::DevicePairClient>>,
}

impl AppState {
    /// pool が必要な統合テスト・本番コード用。None なら panic。
    pub fn pool(&self) -> &sqlx::PgPool {
        self.pool.as_ref().expect("PgPool is required but not set")
    }
}
