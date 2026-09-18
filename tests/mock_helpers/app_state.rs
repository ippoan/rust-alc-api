use std::sync::Arc;

use async_trait::async_trait;
use uuid::Uuid;

use alc_core::models::VehicleSettingsDump;
use alc_core::repository::vehicle_settings_dumps::{
    VehicleSettingsDumpInput, VehicleSettingsDumpSummary, VehicleSettingsDumpsRepository,
};
use rust_alc_api::AppState;

use super::*;
use crate::common::mock_storage::MockStorage;

/// VehicleSettingsDumpsRepository のスタブ。本 PR 範囲では handler を
/// mock test ではカバーしないため、読み出しは空 / write はエラーを返す
/// (重要: AppState の型を揃えるためだけのダミー)。
#[derive(Default)]
pub struct MockVehicleSettingsDumpsRepository;

#[async_trait]
impl VehicleSettingsDumpsRepository for MockVehicleSettingsDumpsRepository {
    async fn register(
        &self,
        _tenant_id: Uuid,
        _input: VehicleSettingsDumpInput,
    ) -> Result<VehicleSettingsDump, sqlx::Error> {
        Err(sqlx::Error::RowNotFound)
    }

    async fn list_by_vehicle_cd(
        &self,
        _tenant_id: Uuid,
        _vehicle_cd: &str,
    ) -> Result<Vec<VehicleSettingsDump>, sqlx::Error> {
        Ok(vec![])
    }

    async fn summary_by_vehicle(
        &self,
        _tenant_id: Uuid,
    ) -> Result<Vec<VehicleSettingsDumpSummary>, sqlx::Error> {
        Ok(vec![])
    }

    async fn confirmed_vehicle_cds(&self, _tenant_id: Uuid) -> Result<Vec<String>, sqlx::Error> {
        Ok(vec![])
    }
}

/// CamerasRepository の mock (Refs #556)。camera route が monolith で応答する
/// ことの検証用。`fail_next` を立てると次の呼び出しで sqlx::Error を返す。
#[derive(Default)]
pub struct MockCamerasRepository {
    pub fail_next: std::sync::atomic::AtomicBool,
}

impl MockCamerasRepository {
    fn check_fail(&self) -> Result<(), sqlx::Error> {
        if self
            .fail_next
            .swap(false, std::sync::atomic::Ordering::SeqCst)
        {
            return Err(sqlx::Error::RowNotFound);
        }
        Ok(())
    }
}

fn mock_camera(tenant_id: Uuid, id: Uuid) -> alc_core::repository::cameras::Camera {
    alc_core::repository::cameras::Camera {
        id,
        tenant_id,
        office_id: None,
        name: "test-cam".to_string(),
        ip: "192.168.0.10".to_string(),
        onvif_port: 2020,
        model: "Tapo".to_string(),
        active: true,
        active_down_ticket_id: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    }
}

#[async_trait]
impl alc_core::repository::CamerasRepository for MockCamerasRepository {
    async fn list(
        &self,
        _tenant_id: Uuid,
    ) -> Result<Vec<alc_core::repository::cameras::Camera>, sqlx::Error> {
        self.check_fail()?;
        Ok(vec![])
    }

    async fn get(
        &self,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<Option<alc_core::repository::cameras::Camera>, sqlx::Error> {
        self.check_fail()?;
        Ok(Some(mock_camera(tenant_id, id)))
    }

    async fn create(
        &self,
        tenant_id: Uuid,
        _input: &alc_core::repository::cameras::CreateCamera,
    ) -> Result<alc_core::repository::cameras::Camera, sqlx::Error> {
        self.check_fail()?;
        Ok(mock_camera(tenant_id, Uuid::new_v4()))
    }

    async fn update(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        _input: &alc_core::repository::cameras::UpdateCamera,
    ) -> Result<Option<alc_core::repository::cameras::Camera>, sqlx::Error> {
        self.check_fail()?;
        Ok(Some(mock_camera(tenant_id, id)))
    }

    async fn delete(&self, _tenant_id: Uuid, _id: Uuid) -> Result<bool, sqlx::Error> {
        self.check_fail()?;
        Ok(true)
    }

    async fn insert_health_log(
        &self,
        tenant_id: Uuid,
        camera_id: Uuid,
        input: &alc_core::repository::cameras::CreateCameraHealthLog,
    ) -> Result<alc_core::repository::cameras::CameraHealthLog, sqlx::Error> {
        self.check_fail()?;
        Ok(alc_core::repository::cameras::CameraHealthLog {
            id: 1,
            tenant_id,
            camera_id,
            alive: input.alive,
            latency_ms: input.latency_ms,
            error: input.error.clone(),
            checked_at: chrono::Utc::now(),
            source_device_id: input.source_device_id.clone(),
        })
    }

    async fn recent_health_logs(
        &self,
        _tenant_id: Uuid,
        _camera_id: Uuid,
        _limit: i64,
    ) -> Result<Vec<alc_core::repository::cameras::CameraHealthLog>, sqlx::Error> {
        self.check_fail()?;
        Ok(vec![])
    }

    async fn statuses(
        &self,
        _tenant_id: Uuid,
    ) -> Result<Vec<alc_core::repository::cameras::CameraStatusRow>, sqlx::Error> {
        self.check_fail()?;
        Ok(vec![])
    }

    async fn set_active_down_ticket(
        &self,
        _tenant_id: Uuid,
        _camera_id: Uuid,
        _ticket_id: Option<Uuid>,
    ) -> Result<(), sqlx::Error> {
        self.check_fail()?;
        Ok(())
    }
}

/// VehiclesRepository のスタブ (Refs #651)。本タスク (#c651-2、車両マスタ土台) の
/// 範囲では handler を mock test ではカバーしない (実 DB 統合テスト
/// `tests/maintenance_test.rs` 側で検証する) ため、読み出しは空 / write はエラーを
/// 返す (重要: MaintenanceState の型を揃えるためだけのダミー。
/// `MockVehicleSettingsDumpsRepository` と同じ方針)。
#[derive(Default)]
pub struct MockVehiclesRepository;

#[async_trait]
impl alc_maintenance::vehicles::VehiclesRepository for MockVehiclesRepository {
    async fn list(
        &self,
        _tenant_id: Uuid,
        filter: &alc_maintenance::models::VehicleListFilter,
    ) -> Result<alc_maintenance::models::VehicleListResponse, sqlx::Error> {
        Ok(alc_maintenance::models::VehicleListResponse {
            items: vec![],
            total: 0,
            page: filter.page.unwrap_or(1),
            per_page: filter.per_page.unwrap_or(20),
        })
    }

    async fn create(
        &self,
        _tenant_id: Uuid,
        _input: &alc_maintenance::models::CreateMaintenanceVehicle,
    ) -> Result<alc_maintenance::models::MaintenanceVehicle, sqlx::Error> {
        Err(sqlx::Error::RowNotFound)
    }

    async fn get(
        &self,
        _tenant_id: Uuid,
        _id: Uuid,
    ) -> Result<Option<alc_maintenance::models::MaintenanceVehicle>, sqlx::Error> {
        Ok(None)
    }

    async fn update(
        &self,
        _tenant_id: Uuid,
        _id: Uuid,
        _input: &alc_maintenance::models::UpdateMaintenanceVehicle,
    ) -> Result<Option<alc_maintenance::models::MaintenanceVehicle>, sqlx::Error> {
        Ok(None)
    }

    async fn soft_delete(&self, _tenant_id: Uuid, _id: Uuid) -> Result<bool, sqlx::Error> {
        Ok(false)
    }

    async fn link_carins(
        &self,
        _tenant_id: Uuid,
        _id: Uuid,
        _car_id: &str,
    ) -> Result<Option<alc_maintenance::models::MaintenanceVehicle>, sqlx::Error> {
        Ok(None)
    }

    async fn unlink_carins(&self, _tenant_id: Uuid, _id: Uuid) -> Result<bool, sqlx::Error> {
        Ok(false)
    }

    async fn fetch_car_id_by_cert_no(
        &self,
        _tenant_id: Uuid,
        _cert_no: &str,
    ) -> Result<Option<String>, sqlx::Error> {
        Ok(None)
    }

    async fn carins_candidates(
        &self,
        _tenant_id: Uuid,
        _normalized_registration_number: &str,
    ) -> Result<Vec<alc_maintenance::models::CarinsCandidate>, sqlx::Error> {
        Ok(vec![])
    }
}

/// MaintenanceCategoriesRepository のスタブ (Refs #651 — generic master の 5 番目の
/// 利用者)。`MockVehiclesRepository` と同じ方針で、handler の mock test は用意せず
/// (検証は実 DB 統合テスト `tests/maintenance_test.rs` 側)、MaintenanceState の型を
/// 揃えるためだけのダミー — 読み出しは空 / write はエラーを返す。
#[derive(Default)]
pub struct MockMaintenanceCategoriesRepository;

#[async_trait]
impl alc_maintenance::categories::MaintenanceCategoriesRepository
    for MockMaintenanceCategoriesRepository
{
    async fn list(
        &self,
        _tenant_id: Uuid,
    ) -> Result<Vec<alc_maintenance::models::MaintenanceCategory>, sqlx::Error> {
        Ok(vec![])
    }

    async fn create(
        &self,
        _tenant_id: Uuid,
        _input: &alc_maintenance::models::CreateMaintenanceCategory,
    ) -> Result<alc_maintenance::models::MaintenanceCategory, sqlx::Error> {
        Err(sqlx::Error::RowNotFound)
    }

    async fn delete(&self, _tenant_id: Uuid, _id: Uuid) -> Result<bool, sqlx::Error> {
        Ok(false)
    }

    async fn update_sort_order(
        &self,
        _tenant_id: Uuid,
        _id: Uuid,
        _sort_order: i32,
    ) -> Result<Option<alc_maintenance::models::MaintenanceCategory>, sqlx::Error> {
        Ok(None)
    }
}

/// maintenance ドメインの mock MaintenanceState (Refs #651)。carins 照合は既存の
/// `MockCarInspectionRepository` (デフォルトで matched_by="none" 相当) をそのまま使う。
pub fn setup_mock_maintenance_state() -> alc_maintenance::MaintenanceState {
    alc_maintenance::MaintenanceState {
        vehicles: Arc::new(MockVehiclesRepository),
        car_inspections: Arc::new(MockCarInspectionRepository::default()),
        categories: Arc::new(MockMaintenanceCategoriesRepository),
    }
}

/// camera ドメインの mock CameraState (Refs #556)。差し替えたい field を上書き
/// してから `spawn_mock_server_with_camera` に渡す。
pub fn setup_mock_camera_state() -> alc_camera::CameraState {
    alc_camera::CameraState {
        cameras: Arc::new(MockCamerasRepository::default()),
        down_ticket_sink: Arc::new(crate::common::TestDownTicketSink),
        down_threshold: alc_camera::DEFAULT_DOWN_THRESHOLD,
    }
}

/// DB 不要の mock AppState を構築。
/// pool: None — mock repo が全ハンドラを処理するため DB 接続不要。
/// テスト側で `state.xxx` の `fail_next` を設定して DB エラー注入可能。
pub fn setup_mock_app_state() -> AppState {
    // tracing 初期化 (1回だけ)
    let _ = tracing_subscriber::fmt()
        .with_env_filter("info")
        .with_test_writer()
        .try_init();

    let storage: Arc<dyn rust_alc_api::storage::StorageBackend> =
        Arc::new(MockStorage::new("test-bucket"));

    let dtako_storage: Arc<dyn rust_alc_api::storage::StorageBackend> =
        Arc::new(MockStorage::new("dtako-bucket"));

    AppState {
        pool: None,
        api_tokens: Arc::new(MockApiTokensRepository::default()),
        auth: Arc::new(MockAuthRepository::default()),
        bot_admin: Arc::new(MockBotAdminRepository::default()),
        car_inspections: Arc::new(MockCarInspectionRepository::default()),
        carins_files: Arc::new(MockCarinsFilesRepository::default()),
        carrying_items: Arc::new(MockCarryingItemsRepository::default()),
        communication_items: Arc::new(MockCommunicationItemsRepository::default()),
        devices: Arc::new(MockDeviceRepository::default()),
        dtako_csv_proxy: Arc::new(MockDtakoCsvProxyRepository::default()),
        dtako_daily_hours: Arc::new(MockDtakoDailyHoursRepository::default()),
        dtako_logs: Arc::new(MockDtakoLogsRepository::default()),
        dtako_drivers: Arc::new(MockDtakoDriversRepository::default()),
        dtako_event_classifications: Arc::new(MockDtakoEventClassificationsRepository::default()),
        dtako_operations: Arc::new(MockDtakoOperationsRepository::default()),
        dtako_restraint_report: Arc::new(MockDtakoRestraintReportRepository::default()),
        dtako_restraint_report_pdf: Arc::new(MockDtakoRestraintReportPdfRepository::default()),
        dtako_scraper: Arc::new(MockDtakoScraperRepository::default()),
        dtako_tickets: Arc::new(MockDtakoTicketsRepository::default()),
        hub_measurements: Arc::new(MockHubMeasurementsRepository::default()),
        dtako_upload: Arc::new(MockDtakoUploadRepository::default()),
        dtako_vehicles: Arc::new(MockDtakoVehiclesRepository::default()),
        dtako_work_times: Arc::new(MockDtakoWorkTimesRepository::default()),
        dtako_y_time_export: Arc::new(MockDtakoYTimeExportRepository::default()),
        vehicle_settings_dumps: Arc::new(MockVehicleSettingsDumpsRepository::default()),
        employees: Arc::new(MockEmployeeRepository::default()),
        guidance_records: Arc::new(MockGuidanceRecordsRepository::default()),
        items: Arc::new(MockItemsRepository::default()),
        item_files: Arc::new(MockItemFilesRepository::default()),
        measurements: Arc::new(MockMeasurementsRepository::default()),
        nfc_tags: Arc::new(MockNfcTagRepository::default()),
        sso_admin: Arc::new(MockSsoAdminRepository::default()),
        tenant_users: Arc::new(MockTenantUsersRepository::default()),
        timecard: Arc::new(MockTimecardRepository::default()),
        storage,
        carins_storage: None,
        dtako_storage: Some(dtako_storage),
        fcm: None,
        notify_recipients: Arc::new(MockNotifyRecipientRepository::default()),
        notify_groups: Arc::new(MockNotifyGroupRepository::default()),
        notify_documents: Arc::new(MockNotifyDocumentRepository::default()),
        notify_deliveries: Arc::new(MockNotifyDeliveryRepository::default()),
        notify_line_config: Arc::new(MockNotifyLineConfigRepository::default()),
        lineworks_channels: Arc::new(MockLineworksChannelsRepository::default()),
        notify_storage: None,
        redact_broadcaster: None,
        realtime_bus: None,
        device_pair_client: None,
        webhook: None,
    }
}

/// tenko ドメインの mock TenkoState (Refs #513)。tenko 系 mock test は
/// 差し替えたい field を上書きしてから `spawn_test_server_with_tenko` に渡す。
pub fn setup_mock_tenko_state() -> alc_tenko::TenkoState {
    alc_tenko::TenkoState {
        tenko_call: Arc::new(MockTenkoCallRepository::default()),
        tenko_records: Arc::new(MockTenkoRecordsRepository::default()),
        tenko_schedules: Arc::new(MockTenkoSchedulesRepository::default()),
        tenko_sessions: Arc::new(MockTenkoSessionRepository::default()),
        tenko_webhooks: Arc::new(MockTenkoWebhooksRepository::default()),
        daily_health: Arc::new(MockDailyHealthRepository::default()),
        health_baselines: Arc::new(MockHealthBaselinesRepository::default()),
        equipment_failures: Arc::new(MockEquipmentFailuresRepository::default()),
        driver_info: Arc::new(MockDriverInfoRepository::default()),
        webhook: None,
    }
}

/// mock 用 spawn wrapper: デフォルトの mock TenkoState を合成して起動する。
pub async fn spawn_mock_server(state: AppState) -> String {
    crate::common::spawn_test_server_with_states(
        state,
        setup_mock_tenko_state(),
        setup_mock_trouble_state(),
        setup_mock_camera_state(),
        setup_mock_maintenance_state(),
    )
    .await
}

/// tenko_state を差し替えたい mock テスト用 (trouble/camera/maintenance はデフォルト mock)。
pub async fn spawn_mock_server_with_tenko(
    state: AppState,
    tenko_state: alc_tenko::TenkoState,
) -> String {
    crate::common::spawn_test_server_with_states(
        state,
        tenko_state,
        setup_mock_trouble_state(),
        setup_mock_camera_state(),
        setup_mock_maintenance_state(),
    )
    .await
}

/// trouble_state を差し替えたい mock テスト用 (tenko/camera/maintenance はデフォルト mock)。
pub async fn spawn_mock_server_with_trouble(
    state: AppState,
    trouble_state: alc_trouble::TroubleState,
) -> String {
    crate::common::spawn_test_server_with_states(
        state,
        setup_mock_tenko_state(),
        trouble_state,
        setup_mock_camera_state(),
        setup_mock_maintenance_state(),
    )
    .await
}

/// camera_state を差し替えたい mock テスト用 (tenko/trouble/maintenance はデフォルト mock、Refs #556)。
pub async fn spawn_mock_server_with_camera(
    state: AppState,
    camera_state: alc_camera::CameraState,
) -> String {
    crate::common::spawn_test_server_with_states(
        state,
        setup_mock_tenko_state(),
        setup_mock_trouble_state(),
        camera_state,
        setup_mock_maintenance_state(),
    )
    .await
}

/// maintenance_state を差し替えたい mock テスト用 (tenko/trouble/camera はデフォルト mock、Refs #651)。
pub async fn spawn_mock_server_with_maintenance(
    state: AppState,
    maintenance_state: alc_maintenance::MaintenanceState,
) -> String {
    crate::common::spawn_test_server_with_states(
        state,
        setup_mock_tenko_state(),
        setup_mock_trouble_state(),
        setup_mock_camera_state(),
        maintenance_state,
    )
    .await
}

/// mock trouble state (Refs #513 Phase B)。差し替えたい field を上書きしてから
/// `spawn_mock_server_with_trouble` に渡す。
pub fn setup_mock_trouble_state() -> alc_trouble::TroubleState {
    alc_trouble::TroubleState {
        trouble_tickets: Arc::new(MockTroubleTicketsRepository::default()),
        trouble_files: Arc::new(MockTroubleFilesRepository::default()),
        trouble_workflow: Arc::new(MockTroubleWorkflowRepository::default()),
        trouble_categories: Arc::new(MockTroubleCategoriesRepository::default()),
        trouble_offices: Arc::new(MockTroubleOfficesRepository::default()),
        trouble_progress_statuses: Arc::new(MockTroubleProgressStatusesRepository::default()),
        trouble_notification_prefs: Arc::new(MockTroubleNotificationPrefsRepository::default()),
        trouble_schedules: Arc::new(MockTroubleSchedulesRepository::default()),
        trouble_tasks: Arc::new(MockTroubleTasksRepository::default()),
        trouble_task_types: Arc::new(MockTroubleTaskTypesRepository::default()),
        trouble_task_statuses: Arc::new(MockTroubleTaskStatusesRepository::default()),
        trouble_field_layouts: Arc::new(MockTroubleFieldLayoutsRepository::default()),
        trouble_storage: Some(Arc::new(MockStorage::new("trouble-bucket"))),
        webhook: None,
        cloud_tasks: None,
        notifier: None,
        employees: None,
    }
}
