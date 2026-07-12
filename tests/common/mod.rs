// tests/common は複数の test binary から `#[path = "../common/mod.rs"] mod common;`
// として include されるため、binary ごとに使う API が違う。ここを許容しないと
// 単体 binary から見た dead_code / unused_imports 警告が大量に出る。
#![allow(dead_code, unused_imports)]

#[macro_use]
pub mod test_macros;
pub mod mock_storage;

use std::sync::Arc;

use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;

use rust_alc_api::db::repository::{
    PgApiTokensRepository, PgAuthRepository, PgBotAdminRepository, PgCarInspectionRepository,
    PgCarinsFilesRepository, PgCarryingItemsRepository, PgCommunicationItemsRepository,
    PgDailyHealthRepository, PgDeviceRepository, PgDriverInfoRepository, PgDtakoCsvProxyRepository,
    PgDtakoDailyHoursRepository, PgDtakoDriversRepository, PgDtakoEventClassificationsRepository,
    PgDtakoLogsRepository, PgDtakoOperationsRepository, PgDtakoRestraintReportPdfRepository,
    PgDtakoRestraintReportRepository, PgDtakoScraperRepository, PgDtakoTicketsRepository,
    PgDtakoUploadRepository, PgDtakoVehiclesRepository, PgDtakoWorkTimesRepository,
    PgDtakoYTimeExportRepository, PgEmployeeRepository, PgEquipmentFailuresRepository,
    PgGuidanceRecordsRepository, PgHealthBaselinesRepository, PgHubMeasurementsRepository,
    PgItemFilesRepository, PgItemsRepository, PgLineworksChannelsRepository,
    PgMeasurementsRepository, PgNfcTagRepository, PgNotifyDeliveryRepository,
    PgNotifyDocumentRepository, PgNotifyGroupRepository, PgNotifyLineConfigRepository,
    PgNotifyRecipientRepository, PgSsoAdminRepository, PgTenantUsersRepository,
    PgTenkoCallRepository, PgTenkoRecordsRepository, PgTenkoSchedulesRepository,
    PgTenkoSessionRepository, PgTenkoWebhooksRepository, PgTimecardRepository,
    PgTroubleCategoriesRepository, PgTroubleFieldLayoutsRepository, PgTroubleFilesRepository,
    PgTroubleNotificationPrefsRepository, PgTroubleOfficesRepository,
    PgTroubleProgressStatusesRepository, PgTroubleSchedulesRepository,
    PgTroubleTaskStatusesRepository, PgTroubleTaskTypesRepository, PgTroubleTasksRepository,
    PgTroubleTicketsRepository, PgTroubleWorkflowRepository, PgVehicleSettingsDumpsRepository,
};
use rust_alc_api::AppState;

use mock_storage::MockStorage;

/// テスト用 SSO_ENCRYPTION_KEY (保存 secret の AES-256-GCM 暗号鍵素材)。
/// 旧名 TEST_JWT_SECRET — JWT の発行・検証は #479 PR-3 で rust から全撤去された
/// ため、残る用途は SSO_ENCRYPTION_KEY の値のみ (名前もそれに揃えた)。
pub const TEST_ENCRYPTION_KEY: &str = "test-jwt-secret-for-integration-tests-2026";

/// テスト用 `INTERNAL_SHARED_SECRET` (`internal_shared_secret_router` 経由の
/// `X-Internal-Shared-Secret` 検証に使う、dtako_tickets/dtako_operations の
/// internal_router テストが参照する)。
pub const TEST_INTERNAL_SHARED_SECRET: &str = "test-internal-shared-secret-2026";

/// env::set_var を使うテスト同士の直列化用ロック
/// (env var はプロセスグローバルなので並列実行すると競合する)
pub static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// ALTER TABLE RENAME を使うテスト同士の直列化用ロック
/// プロセス内 Mutex + ファイルロック (flock) でバイナリ間も直列化
pub static DB_RENAME_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// ファイルロック (flock) でバイナリ間の直列化 (RENAME/trigger テスト用)
/// DB_RENAME_LOCK (プロセス内) と併用する。drop で自動解放。
pub struct FileLockGuard(std::fs::File);

impl Drop for FileLockGuard {
    fn drop(&mut self) {
        use std::os::unix::io::AsRawFd;
        unsafe {
            libc::flock(self.0.as_raw_fd(), libc::LOCK_UN);
        }
    }
}

pub fn db_rename_flock() -> FileLockGuard {
    use std::os::unix::io::AsRawFd;
    let path = format!("{}/target/.db-rename.lock", env!("CARGO_MANIFEST_DIR"));
    let file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .open(&path)
        .expect("Failed to open lock file");
    let rc = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) };
    assert_eq!(rc, 0, "flock failed");
    FileLockGuard(file)
}

/// email_domain='example.com' を使う Google login テストの直列化用ロック
/// (複数テナントが同じ email_domain を持つと google login ハンドラが混乱する)
pub static GOOGLE_LOGIN_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// テスト用従業員を作成し、JSON レスポンスを返す
pub async fn create_test_employee(
    client: &reqwest::Client,
    base_url: &str,
    auth_header: &str,
    name: &str,
    code: &str,
) -> serde_json::Value {
    let res = client
        .post(format!("{base_url}/api/employees"))
        .header("Authorization", auth_header)
        .json(&serde_json::json!({ "name": name, "code": code }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 201, "Failed to create test employee");
    res.json().await.unwrap()
}

/// テスト用測定を作成し、JSON レスポンスを返す
pub async fn create_test_measurement(
    client: &reqwest::Client,
    base_url: &str,
    auth_header: &str,
    employee_id: &str,
) -> serde_json::Value {
    let res = client
        .post(format!("{base_url}/api/measurements"))
        .header("Authorization", auth_header)
        .json(&serde_json::json!({
            "employee_id": employee_id,
            "alcohol_value": 0.0,
            "result_type": "pass"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 201, "Failed to create test measurement");
    res.json().await.unwrap()
}

/// テスト用 opaque token を組み立てる (base64(JSON))。
///
/// #479 PR-3 で rust 側の JWT 発行・検証は全撤去されたため、テストハーネスの
/// Bearer token は JWT ではなく「identity JSON を base64 した opaque token」に
/// 置き換えた。`test_proxy_inject` (下記) がこれを decode して本番 proxy と
/// 同じ identity ヘッダー (X-Tenant-ID / X-User-*) に変換する。
fn encode_test_token(user_id: Uuid, tenant_id: Uuid, email: &str, role: &str) -> String {
    use base64::Engine;
    let payload = serde_json::json!({
        "tenant_id": tenant_id,
        "sub": user_id,
        "email": email,
        "name": "Test User",
        "role": role,
    });
    base64::engine::general_purpose::STANDARD.encode(payload.to_string())
}

/// 特定ユーザー ID でテスト用 token を発行 (logout 等、user_id 固定が必要なテスト用)
pub fn create_test_jwt_for_user(user_id: Uuid, tenant_id: Uuid, email: &str, role: &str) -> String {
    encode_test_token(user_id, tenant_id, email, role)
}

/// テスト用 MockFcmSender (送信を記録するだけ)
pub struct MockFcmSender {
    pub sent: std::sync::Mutex<Vec<(String, std::collections::HashMap<String, String>)>>,
}

impl MockFcmSender {
    pub fn new() -> Self {
        Self {
            sent: std::sync::Mutex::new(Vec::new()),
        }
    }
}

#[async_trait::async_trait]
impl rust_alc_api::fcm::FcmSenderTrait for MockFcmSender {
    async fn send_data_message(
        &self,
        fcm_token: &str,
        data: std::collections::HashMap<String, String>,
    ) -> Result<(), rust_alc_api::fcm::FcmError> {
        self.sent
            .lock()
            .unwrap()
            .push((fcm_token.to_string(), data));
        Ok(())
    }
}

/// テスト用 DB URL (docker-compose の test-db に接続)
pub fn test_database_url() -> String {
    std::env::var("TEST_DATABASE_URL").unwrap_or_else(|_| {
        "postgresql://postgres:test@localhost:54322/postgres?options=-c search_path=alc_api"
            .to_string()
    })
}

/// テスト用 AppState を構築 (DB 接続 + モックストレージ)
pub async fn setup_app_state() -> AppState {
    // tracing 初期化 (1回だけ。カバレッジ計測で tracing マクロ引数を評価させるため)
    let _ = tracing_subscriber::fmt()
        .with_env_filter("warn")
        .with_test_writer()
        .try_init();

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&test_database_url())
        .await
        .expect("Failed to connect to test DB");

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("Failed to run migrations");

    let storage: Arc<dyn rust_alc_api::storage::StorageBackend> =
        Arc::new(MockStorage::new("test-bucket"));

    let dtako_storage: Arc<dyn rust_alc_api::storage::StorageBackend> =
        Arc::new(MockStorage::new("dtako-bucket"));

    let mock_fcm: Arc<dyn rust_alc_api::fcm::FcmSenderTrait> = Arc::new(MockFcmSender::new());

    let state = build_app_state(pool, storage, Some(dtako_storage), Some(mock_fcm));
    state
}

fn build_app_state(
    pool: sqlx::PgPool,
    storage: Arc<dyn rust_alc_api::storage::StorageBackend>,
    dtako_storage: Option<Arc<dyn rust_alc_api::storage::StorageBackend>>,
    fcm: Option<Arc<dyn rust_alc_api::fcm::FcmSenderTrait>>,
) -> AppState {
    let api_tokens = Arc::new(PgApiTokensRepository::new(pool.clone()));
    let auth = Arc::new(PgAuthRepository::new(pool.clone()));
    let bot_admin = Arc::new(PgBotAdminRepository::new(pool.clone()));
    let car_inspections = Arc::new(PgCarInspectionRepository::new(pool.clone()));
    let carins_files = Arc::new(PgCarinsFilesRepository::new(pool.clone()));
    let carrying_items = Arc::new(PgCarryingItemsRepository::new(pool.clone()));
    let communication_items = Arc::new(PgCommunicationItemsRepository::new(pool.clone()));
    let devices = Arc::new(PgDeviceRepository::new(pool.clone()));
    let dtako_csv_proxy = Arc::new(PgDtakoCsvProxyRepository::new(pool.clone()));
    let dtako_daily_hours = Arc::new(PgDtakoDailyHoursRepository::new(pool.clone()));
    let dtako_logs = Arc::new(PgDtakoLogsRepository::new(pool.clone()));
    let dtako_drivers = Arc::new(PgDtakoDriversRepository::new(pool.clone()));
    let dtako_event_classifications =
        Arc::new(PgDtakoEventClassificationsRepository::new(pool.clone()));
    let dtako_operations = Arc::new(PgDtakoOperationsRepository::new(pool.clone()));
    let dtako_restraint_report = Arc::new(PgDtakoRestraintReportRepository::new(pool.clone()));
    let dtako_restraint_report_pdf =
        Arc::new(PgDtakoRestraintReportPdfRepository::new(pool.clone()));
    let dtako_scraper = Arc::new(PgDtakoScraperRepository::new(pool.clone()));
    let dtako_tickets = Arc::new(PgDtakoTicketsRepository::new(pool.clone()));
    let dtako_upload = Arc::new(PgDtakoUploadRepository::new(pool.clone()));
    let dtako_vehicles = Arc::new(PgDtakoVehiclesRepository::new(pool.clone()));
    let dtako_work_times = Arc::new(PgDtakoWorkTimesRepository::new(pool.clone()));
    let dtako_y_time_export = Arc::new(PgDtakoYTimeExportRepository::new(pool.clone()));
    let vehicle_settings_dumps = Arc::new(PgVehicleSettingsDumpsRepository::new(pool.clone()));
    let employees = Arc::new(PgEmployeeRepository::new(pool.clone()));
    let guidance_records = Arc::new(PgGuidanceRecordsRepository::new(pool.clone()));
    let hub_measurements = Arc::new(PgHubMeasurementsRepository::new(pool.clone()));
    let items = Arc::new(PgItemsRepository::new(pool.clone()));
    let item_files = Arc::new(PgItemFilesRepository::new(pool.clone()));
    let measurements = Arc::new(PgMeasurementsRepository::new(pool.clone()));
    let nfc_tags = Arc::new(PgNfcTagRepository::new(pool.clone()));
    let sso_admin = Arc::new(PgSsoAdminRepository::new(pool.clone()));
    let tenant_users = Arc::new(PgTenantUsersRepository::new(pool.clone()));
    let timecard = Arc::new(PgTimecardRepository::new(pool.clone()));
    let notify_recipients = Arc::new(PgNotifyRecipientRepository::new(pool.clone()));
    let notify_groups = Arc::new(PgNotifyGroupRepository::new(pool.clone()));
    let notify_documents = Arc::new(PgNotifyDocumentRepository::new(pool.clone()));
    let notify_deliveries = Arc::new(PgNotifyDeliveryRepository::new(pool.clone()));
    let notify_line_config = Arc::new(PgNotifyLineConfigRepository::new(pool.clone()));
    let lineworks_channels = Arc::new(PgLineworksChannelsRepository::new(pool.clone()));

    AppState {
        pool: Some(pool),
        api_tokens,
        auth,
        bot_admin,
        car_inspections,
        carins_files,
        carrying_items,
        communication_items,
        devices,
        dtako_csv_proxy,
        dtako_daily_hours,
        dtako_logs,
        dtako_drivers,
        dtako_event_classifications,
        dtako_operations,
        dtako_restraint_report,
        dtako_restraint_report_pdf,
        dtako_scraper,
        dtako_tickets,
        dtako_upload,
        dtako_vehicles,
        dtako_work_times,
        dtako_y_time_export,
        vehicle_settings_dumps,
        employees,
        guidance_records,
        hub_measurements,
        items,
        item_files,
        measurements,
        nfc_tags,
        sso_admin,
        tenant_users,
        timecard,
        storage,
        carins_storage: None,
        dtako_storage,
        fcm,
        notify_recipients,
        notify_groups,
        notify_documents,
        notify_deliveries,
        notify_line_config,
        lineworks_channels,
        notify_storage: None,
        redact_broadcaster: None,
        realtime_bus: None,
        device_pair_client: None,
        webhook: None,
    }
}

/// テスト用 FailingFcmSender (常にエラーを返す)
pub struct FailingFcmSender;

#[async_trait::async_trait]
impl rust_alc_api::fcm::FcmSenderTrait for FailingFcmSender {
    async fn send_data_message(
        &self,
        _fcm_token: &str,
        _data: std::collections::HashMap<String, String>,
    ) -> Result<(), rust_alc_api::fcm::FcmError> {
        Err(rust_alc_api::fcm::FcmError::Send("test error".to_string()))
    }
}

/// テスト用 AppState を構築 (FCM なし)
pub async fn setup_app_state_no_fcm() -> AppState {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("warn")
        .with_test_writer()
        .try_init();

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&test_database_url())
        .await
        .expect("Failed to connect to test DB");

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("Failed to run migrations");

    let storage: Arc<dyn rust_alc_api::storage::StorageBackend> =
        Arc::new(MockStorage::new("test-bucket"));

    let dtako_storage: Arc<dyn rust_alc_api::storage::StorageBackend> =
        Arc::new(MockStorage::new("dtako-bucket"));

    build_app_state(pool, storage, Some(dtako_storage), None)
}

/// テスト用 AppState を構築 (FailingFcmSender)
pub async fn setup_app_state_failing_fcm() -> AppState {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("warn")
        .with_test_writer()
        .try_init();

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&test_database_url())
        .await
        .expect("Failed to connect to test DB");

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("Failed to run migrations");

    let storage: Arc<dyn rust_alc_api::storage::StorageBackend> =
        Arc::new(MockStorage::new("test-bucket"));

    let dtako_storage: Arc<dyn rust_alc_api::storage::StorageBackend> =
        Arc::new(MockStorage::new("dtako-bucket"));

    let failing_fcm: Arc<dyn rust_alc_api::fcm::FcmSenderTrait> = Arc::new(FailingFcmSender);

    build_app_state(pool, storage, Some(dtako_storage), Some(failing_fcm))
}

/// テスト用テナントを作成し、UUID を返す
pub async fn create_test_tenant(pool: &sqlx::PgPool, name: &str) -> Uuid {
    let row: (Uuid,) =
        sqlx::query_as("INSERT INTO tenants (name, slug) VALUES ($1, $2) RETURNING id")
            .bind(name)
            .bind(format!("test-{}", Uuid::new_v4().simple()))
            .fetch_one(pool)
            .await
            .expect("Failed to create test tenant");
    row.0
}

/// テスト用 token を発行 (user_id は都度新規 UUID)
pub fn create_test_jwt(tenant_id: Uuid, role: &str) -> String {
    encode_test_token(Uuid::new_v4(), tenant_id, "test@example.com", role)
}

/// 内部 API 用のテスト token を返す (aud=alc-api-internal)。
///
/// #479 で `require_internal_jwt` は Google OIDC 一本化 (HS256 dual-accept 撤去)
/// されたため、`spawn_test_server` が注入する test_claims モードの
/// `GoogleTokenVerifier` が受理する固定 token を返す。
pub fn create_test_internal_jwt() -> String {
    "test-valid-token".to_string()
}

/// dtako テスト用の最小 ZIP (KUDGURI.csv + KUDGIVT.csv) を生成
pub fn create_test_dtako_zip() -> Vec<u8> {
    create_test_dtako_zip_with_unko_no(1001)
}

/// dtako テスト用の最小 ZIP (unko_no 指定版)
pub fn create_test_dtako_zip_with_unko_no(unko_no: u32) -> Vec<u8> {
    use std::io::Write;

    let kudguri_csv = format!(
        "運行NO,読取日,事業所CD,事業所名,車輌CD,車輌名,乗務員CD1,乗務員名１,対象乗務員区分\n\
                       {unko_no},2026/03/01,OFF01,テスト事業所,VH01,テスト車両,DR01,テスト運転者,1\n"
    );
    let kudgivt_csv = format!(
        "運行NO,読取日,乗務員CD1,乗務員名１,対象乗務員区分,開始日時,イベントCD,イベント名\n\
                       {unko_no},2026/03/01,DR01,テスト運転者,1,2026/03/01 08:00:00,100,出庫\n"
    );
    let kudguri_csv = kudguri_csv.as_str();
    let kudgivt_csv = kudgivt_csv.as_str();

    // Shift-JIS にエンコード
    let (kudguri_bytes, _, _) = encoding_rs::SHIFT_JIS.encode(kudguri_csv);
    let (kudgivt_bytes, _, _) = encoding_rs::SHIFT_JIS.encode(kudgivt_csv);

    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut buf);
        let options = zip::write::SimpleFileOptions::default();
        zip.start_file("KUDGURI.csv", options).unwrap();
        zip.write_all(&kudguri_bytes).unwrap();
        zip.start_file("KUDGIVT.csv", options).unwrap();
        zip.write_all(&kudgivt_bytes).unwrap();
        zip.finish().unwrap();
    }
    buf.into_inner()
}

/// dtako テスト用リッチ ZIP (複数運行・複数日・複数ドライバー・302休息・301休憩) を生成
pub fn create_test_dtako_zip_rich() -> Vec<u8> {
    use std::io::Write;

    // KUDGURI: 3運行、2ドライバー、2日分、出社/退社/距離あり
    let kudguri_csv = "\
運行NO,読取日,運行日,事業所CD,事業所名,車輌CD,車輌名,乗務員CD1,乗務員名１,対象乗務員区分,出社日時,退社日時,出庫日時,帰庫日時,総走行距離,一般道運転時間,高速道運転時間,バイパス運転時間
1001,2026/03/01,2026/03/01,OFF01,テスト事業所,VH01,車両A,DR01,運転者A,1,2026/03/01 08:00:00,2026/03/01 18:00:00,2026/03/01 08:30:00,2026/03/01 17:30:00,150.5,300,60,20
1002,2026/03/01,2026/03/01,OFF01,テスト事業所,VH02,車両B,DR02,運転者B,1,2026/03/01 09:00:00,2026/03/01 19:00:00,2026/03/01 09:30:00,2026/03/01 18:30:00,200.0,350,40,10
1003,2026/03/02,2026/03/02,OFF01,テスト事業所,VH01,車両A,DR01,運転者A,1,2026/03/02 07:00:00,2026/03/02 17:00:00,2026/03/02 07:30:00,2026/03/02 16:30:00,120.0,280,50,15
";

    // KUDGIVT: 複数イベント種別 (100=出庫, 200=運転, 300=荷役, 301=休憩, 302=休息分割)
    let kudgivt_csv = "\
運行NO,読取日,乗務員CD1,乗務員名１,対象乗務員区分,開始日時,終了日時,イベントCD,イベント名,区間時間,区間距離
1001,2026/03/01,DR01,運転者A,1,2026/03/01 08:00:00,2026/03/01 08:30:00,100,出庫,30,0
1001,2026/03/01,DR01,運転者A,1,2026/03/01 08:30:00,2026/03/01 12:00:00,200,運転,210,75.0
1001,2026/03/01,DR01,運転者A,1,2026/03/01 12:00:00,2026/03/01 13:00:00,301,休憩,60,0
1001,2026/03/01,DR01,運転者A,1,2026/03/01 13:00:00,2026/03/01 15:00:00,300,荷役,120,0
1001,2026/03/01,DR01,運転者A,1,2026/03/01 15:00:00,2026/03/01 17:30:00,200,運転,150,75.5
1001,2026/03/01,DR01,運転者A,1,2026/03/01 17:30:00,2026/03/01 18:00:00,302,休息,30,0
1002,2026/03/01,DR02,運転者B,1,2026/03/01 09:00:00,2026/03/01 09:30:00,100,出庫,30,0
1002,2026/03/01,DR02,運転者B,1,2026/03/01 09:30:00,2026/03/01 14:00:00,200,運転,270,120.0
1002,2026/03/01,DR02,運転者B,1,2026/03/01 14:00:00,2026/03/01 15:00:00,300,荷役,60,0
1002,2026/03/01,DR02,運転者B,1,2026/03/01 15:00:00,2026/03/01 18:30:00,200,運転,210,80.0
1003,2026/03/02,DR01,運転者A,1,2026/03/02 07:00:00,2026/03/02 07:30:00,100,出庫,30,0
1003,2026/03/02,DR01,運転者A,1,2026/03/02 07:30:00,2026/03/02 11:30:00,200,運転,240,60.0
1003,2026/03/02,DR01,運転者A,1,2026/03/02 11:30:00,2026/03/02 12:30:00,301,休憩,60,0
1003,2026/03/02,DR01,運転者A,1,2026/03/02 12:30:00,2026/03/02 14:00:00,300,荷役,90,0
1003,2026/03/02,DR01,運転者A,1,2026/03/02 14:00:00,2026/03/02 16:30:00,200,運転,150,60.0
";

    let (kudguri_bytes, _, _) = encoding_rs::SHIFT_JIS.encode(kudguri_csv);
    let (kudgivt_bytes, _, _) = encoding_rs::SHIFT_JIS.encode(kudgivt_csv);

    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut buf);
        let options = zip::write::SimpleFileOptions::default();
        zip.start_file("KUDGURI.csv", options).unwrap();
        zip.write_all(&kudguri_bytes).unwrap();
        zip.start_file("KUDGIVT.csv", options).unwrap();
        zip.write_all(&kudgivt_bytes).unwrap();
        zip.finish().unwrap();
    }
    buf.into_inner()
}

/// テスト用 axum サーバーを起動し、base URL を返す
/// テスト用 proxy emulation middleware (Refs #434)。
///
/// 本番では CF proxy (alc-app / carins / nuxt-items) が auth-worker `/auth/introspect`
/// で user/device JWT を検証し、検証済み identity を `X-Tenant-ID` / `X-User-*` ヘッダー
/// として注入する。rust-alc-api 自身は JWT 検証を行わず注入 identity を信頼する。
///
/// テストハーネスは従来どおり `Authorization: Bearer <token>` を送るので、この
/// middleware が proxy 役を演じて opaque token (base64(JSON)、`encode_test_token`
/// 参照) を decode → identity ヘッダーに変換し、production の
/// `require_tenant_header` がそれを信頼する形を再現する。Bearer が無い / decode
/// 失敗なら何も注入しない (= bare X-Tenant-ID キオスクテストや無認証 401 テスト
/// は素通り。旧 JWT 検証失敗時と同じ挙動)。
async fn test_proxy_inject(
    mut req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    use base64::Engine;
    let verified: Option<serde_json::Value> = req
        .headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .and_then(|token| base64::engine::general_purpose::STANDARD.decode(token).ok())
        .and_then(|bytes| serde_json::from_slice(&bytes).ok());
    if let Some(claims) = verified {
        let get = |key: &str| claims.get(key).and_then(|v| v.as_str()).map(String::from);
        let h = req.headers_mut();
        if let Some(v) = get("tenant_id").and_then(|s| s.parse().ok()) {
            h.insert("X-Tenant-ID", v);
        }
        if let Some(v) = get("sub").and_then(|s| s.parse().ok()) {
            h.insert("X-User-ID", v);
        }
        if let Some(v) = get("email").and_then(|s| s.parse().ok()) {
            h.insert("X-User-Email", v);
        }
        if let Some(v) = get("role").and_then(|s| s.parse().ok()) {
            h.insert("X-User-Role", v);
        }
        if let Some(v) = get("org_slug").and_then(|s| s.parse().ok()) {
            h.insert("X-Tenant-Slug", v);
        }
    }
    next.run(req).await
}

/// テスト用 `InternalOidcTrust` (test_claims モード)。`create_test_internal_jwt()`
/// が返す "test-valid-token" だけを受理する (Refs #479 — OIDC 一本化後の
/// internal route テスト用)。
fn internal_oidc_trust_for_tests() -> rust_alc_api::middleware::auth::InternalOidcTrust {
    use rust_alc_api::auth::google::{GoogleClaims, GoogleTokenVerifier};
    rust_alc_api::middleware::auth::InternalOidcTrust {
        verifier: GoogleTokenVerifier::with_test_claims(
            rust_alc_api::auth::jwt::INTERNAL_AUD.to_string(),
            GoogleClaims {
                sub: "test-internal-sa".to_string(),
                email: String::new(),
                name: String::new(),
                picture: None,
                email_verified: false,
                aud: rust_alc_api::auth::jwt::INTERNAL_AUD.to_string(),
                iss: "https://accounts.google.com".to_string(),
                exp: 9999999999,
            },
        ),
    }
}

/// テスト用 axum サーバーを起動し、base URL を返す
/// pool ベースの Pg TenkoState を合成する (integration test 用、Refs #513)
pub fn pg_tenko_state(state: &AppState) -> alc_tenko::TenkoState {
    let pool = state
        .pool
        .clone()
        .expect("spawn_test_server: state.pool is None — mock state は spawn_test_server_with_tenko を使うこと");
    alc_tenko::TenkoState {
        tenko_call: Arc::new(PgTenkoCallRepository::new(pool.clone())),
        tenko_records: Arc::new(PgTenkoRecordsRepository::new(pool.clone())),
        tenko_schedules: Arc::new(PgTenkoSchedulesRepository::new(pool.clone())),
        tenko_sessions: Arc::new(PgTenkoSessionRepository::new(pool.clone())),
        tenko_webhooks: Arc::new(PgTenkoWebhooksRepository::new(pool.clone())),
        daily_health: Arc::new(PgDailyHealthRepository::new(pool.clone())),
        health_baselines: Arc::new(PgHealthBaselinesRepository::new(pool.clone())),
        equipment_failures: Arc::new(PgEquipmentFailuresRepository::new(pool.clone())),
        driver_info: Arc::new(PgDriverInfoRepository::new(pool)),
        webhook: state.webhook.clone(),
    }
}

/// pool から trouble ドメインの Pg TroubleState を合成する (Refs #513 Phase B)。
pub fn pg_trouble_state(state: &AppState) -> alc_trouble::TroubleState {
    let pool = state
        .pool
        .clone()
        .expect("spawn_test_server: state.pool is None — mock state は spawn_test_server_with_states を使うこと");
    alc_trouble::TroubleState {
        trouble_tickets: Arc::new(PgTroubleTicketsRepository::new(pool.clone())),
        trouble_files: Arc::new(PgTroubleFilesRepository::new(pool.clone())),
        trouble_workflow: Arc::new(PgTroubleWorkflowRepository::new(pool.clone())),
        trouble_categories: Arc::new(PgTroubleCategoriesRepository::new(pool.clone())),
        trouble_offices: Arc::new(PgTroubleOfficesRepository::new(pool.clone())),
        trouble_progress_statuses: Arc::new(PgTroubleProgressStatusesRepository::new(pool.clone())),
        trouble_notification_prefs: Arc::new(PgTroubleNotificationPrefsRepository::new(
            pool.clone(),
        )),
        trouble_schedules: Arc::new(PgTroubleSchedulesRepository::new(pool.clone())),
        trouble_tasks: Arc::new(PgTroubleTasksRepository::new(pool.clone())),
        trouble_task_types: Arc::new(PgTroubleTaskTypesRepository::new(pool.clone())),
        trouble_task_statuses: Arc::new(PgTroubleTaskStatusesRepository::new(pool.clone())),
        trouble_field_layouts: Arc::new(PgTroubleFieldLayoutsRepository::new(pool)),
        trouble_storage: Some(Arc::new(MockStorage::new("trouble-bucket"))),
        webhook: state.webhook.clone(),
        cloud_tasks: None,
        notifier: None,
        employees: Some(state.employees.clone()),
    }
}

/// camera ドメインの起票 port テスト実装 (Refs #556)。実 trouble への配線は
/// binary 側の `TroubleDownTicketSink` が持つが、テストの camera route 検証は
/// CRUD/status が主で down 起票経路は fire しないため、ここでは no-op で発番のみ返す。
pub struct TestDownTicketSink;

#[async_trait::async_trait]
impl alc_camera::DownTicketSink for TestDownTicketSink {
    async fn open_down_ticket(
        &self,
        _tenant_id: Uuid,
        _ticket: alc_camera::CameraDownTicket,
    ) -> Result<Uuid, sqlx::Error> {
        Ok(Uuid::new_v4())
    }
}

/// pool から camera ドメインの Pg CameraState を合成する (Refs #556)。
pub fn pg_camera_state(state: &AppState) -> alc_camera::CameraState {
    let pool = state
        .pool
        .clone()
        .expect("spawn_test_server: state.pool is None — mock state は spawn_test_server_with_states を使うこと");
    alc_camera::CameraState {
        cameras: Arc::new(alc_camera::repo::PgCamerasRepository::new(pool)),
        down_ticket_sink: Arc::new(TestDownTicketSink),
        down_threshold: alc_camera::DEFAULT_DOWN_THRESHOLD,
    }
}

pub async fn spawn_test_server(state: AppState) -> String {
    let tenko_state = pg_tenko_state(&state);
    let trouble_state = pg_trouble_state(&state);
    let camera_state = pg_camera_state(&state);
    spawn_test_server_with_states(state, tenko_state, trouble_state, camera_state).await
}

pub async fn spawn_test_server_with_states(
    state: AppState,
    tenko_state: alc_tenko::TenkoState,
    trouble_state: alc_trouble::TroubleState,
    camera_state: alc_camera::CameraState,
) -> String {
    use axum::{Extension, Router};
    use rust_alc_api::auth::google::GoogleTokenVerifier;
    use tower_http::cors::{Any, CorsLayer};

    let google_verifier = GoogleTokenVerifier::with_test_claims(
        "test-google-client-id".to_string(),
        rust_alc_api::auth::google::GoogleClaims {
            sub: "test-google-sub-12345".to_string(),
            email: "google-test@example.com".to_string(),
            name: "Google Test User".to_string(),
            picture: None,
            email_verified: true,
            aud: "test-google-client-id".to_string(),
            iss: "https://accounts.google.com".to_string(),
            exp: 9999999999,
        },
    );

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        .nest(
            "/api",
            rust_alc_api::routes::router(
                internal_oidc_trust_for_tests(),
                tenko_state,
                trouble_state,
                camera_state,
            )
            // main.rs と同じ配線 (internal_shared_secret_router を /api 直下にマージ)。
            // テスト用共有 secret は TEST_INTERNAL_SHARED_SECRET で固定する。
            .merge(rust_alc_api::routes::internal_shared_secret_router(Some(
                TEST_INTERNAL_SHARED_SECRET.to_string(),
            ))),
        )
        // テスト用 proxy emulation (Refs #434)。#434 で rust-alc-api は JWT 検証を
        // 撤去し、注入された identity ヘッダー (X-Tenant-ID / X-User-*) を信頼する
        // dumb backend になった。本番では CF proxy が auth-worker introspect で検証
        // して注入する。テストは従来どおり `Authorization: Bearer <token>` を送る
        // ので、ここで opaque token → identity ヘッダーに変換し proxy 役を演じる。
        // `.nest` 直後に layer することで require_tenant_header より外側で先に走る。
        .layer(axum::middleware::from_fn(test_proxy_inject))
        .layer(Extension(google_verifier))
        .layer(cors)
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("Failed to bind test server");
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    format!("http://{addr}")
}
