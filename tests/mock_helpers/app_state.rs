use std::sync::Arc;

use rust_alc_api::AppState;

use super::*;
use crate::common::mock_storage::MockStorage;

/// DB 不要の mock AppState を構築。
/// pool: None — mock repo が全ハンドラを処理するため DB 接続不要。
/// テスト側で `state.xxx` の `fail_next` を設定して DB エラー注入可能。
pub fn setup_mock_app_state() -> AppState {
    // tracing 初期化 (1回だけ)
    let _ = tracing_subscriber::fmt()
        .with_env_filter("warn")
        .with_test_writer()
        .try_init();

    let storage: Arc<dyn rust_alc_api::storage::StorageBackend> =
        Arc::new(MockStorage::new("test-bucket"));

    let dtako_storage: Arc<dyn rust_alc_api::storage::StorageBackend> =
        Arc::new(MockStorage::new("dtako-bucket"));

    AppState {
        pool: None,
        auth: Arc::new(MockAuthRepository::default()),
        bot_admin: Arc::new(MockBotAdminRepository::default()),
        car_inspections: Arc::new(MockCarInspectionRepository::default()),
        carins_files: Arc::new(MockCarinsFilesRepository::default()),
        carrying_items: Arc::new(MockCarryingItemsRepository::default()),
        communication_items: Arc::new(MockCommunicationItemsRepository::default()),
        daily_health: Arc::new(MockDailyHealthRepository::default()),
        devices: Arc::new(MockDeviceRepository::default()),
        driver_info: Arc::new(MockDriverInfoRepository::default()),
        dtako_csv_proxy: Arc::new(MockDtakoCsvProxyRepository::default()),
        dtako_daily_hours: Arc::new(MockDtakoDailyHoursRepository::default()),
        dtako_logs: Arc::new(MockDtakoLogsRepository::default()),
        dtako_drivers: Arc::new(MockDtakoDriversRepository::default()),
        dtako_event_classifications: Arc::new(MockDtakoEventClassificationsRepository::default()),
        dtako_operations: Arc::new(MockDtakoOperationsRepository::default()),
        dtako_restraint_report: Arc::new(MockDtakoRestraintReportRepository::default()),
        dtako_restraint_report_pdf: Arc::new(MockDtakoRestraintReportPdfRepository::default()),
        dtako_scraper: Arc::new(MockDtakoScraperRepository::default()),
        dtako_upload: Arc::new(MockDtakoUploadRepository::default()),
        dtako_vehicles: Arc::new(MockDtakoVehiclesRepository::default()),
        dtako_work_times: Arc::new(MockDtakoWorkTimesRepository::default()),
        employees: Arc::new(MockEmployeeRepository::default()),
        equipment_failures: Arc::new(MockEquipmentFailuresRepository::default()),
        guidance_records: Arc::new(MockGuidanceRecordsRepository::default()),
        health_baselines: Arc::new(MockHealthBaselinesRepository::default()),
        measurements: Arc::new(MockMeasurementsRepository::default()),
        nfc_tags: Arc::new(MockNfcTagRepository::default()),
        sso_admin: Arc::new(MockSsoAdminRepository::default()),
        tenant_users: Arc::new(MockTenantUsersRepository::default()),
        tenko_call: Arc::new(MockTenkoCallRepository::default()),
        tenko_records: Arc::new(MockTenkoRecordsRepository::default()),
        tenko_schedules: Arc::new(MockTenkoSchedulesRepository::default()),
        tenko_sessions: Arc::new(MockTenkoSessionRepository::default()),
        tenko_webhooks: Arc::new(MockTenkoWebhooksRepository::default()),
        timecard: Arc::new(MockTimecardRepository::default()),
        storage,
        carins_storage: None,
        dtako_storage: Some(dtako_storage),
        fcm: None,
        notify_recipients: Arc::new(MockNotifyRecipientRepository::default()),
        notify_documents: Arc::new(MockNotifyDocumentRepository::default()),
        notify_deliveries: Arc::new(MockNotifyDeliveryRepository::default()),
        notify_line_config: Arc::new(MockNotifyLineConfigRepository::default()),
        notify_storage: None,
        webhook: None,
    }
}
