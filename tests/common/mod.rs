#[macro_use]
pub mod test_macros;
pub mod mock_storage;

use std::sync::Arc;

use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;

use rust_alc_api::auth::jwt::{create_access_token, JwtSecret};
use rust_alc_api::db::models::User;
use rust_alc_api::db::repository::{PgEmployeeRepository, PgNfcTagRepository};
use rust_alc_api::AppState;

use mock_storage::MockStorage;

pub const TEST_JWT_SECRET: &str = "test-jwt-secret-for-integration-tests-2026";

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

/// DB にテストユーザーを直接作成し、(user_id, raw_refresh_token) を返す
pub async fn create_test_user_in_db(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    email: &str,
    role: &str,
) -> (Uuid, String) {
    use rust_alc_api::auth::jwt::{
        create_refresh_token, hash_refresh_token, refresh_token_expires_at,
    };

    let user_id = Uuid::new_v4();
    let google_sub = format!("test-gsub-{}", Uuid::new_v4().simple());
    let (raw_token, token_hash) = create_refresh_token();
    let expires_at = refresh_token_expires_at();

    sqlx::query(
        r#"
        INSERT INTO users (id, tenant_id, google_sub, email, name, role, refresh_token_hash, refresh_token_expires_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        "#,
    )
    .bind(user_id)
    .bind(tenant_id)
    .bind(&google_sub)
    .bind(email)
    .bind("Test User")
    .bind(role)
    .bind(&token_hash)
    .bind(expires_at)
    .execute(pool)
    .await
    .expect("Failed to create test user in DB");

    (user_id, raw_token)
}

/// 特定ユーザー ID で JWT を発行 (refresh/logout テスト用)
pub fn create_test_jwt_for_user(user_id: Uuid, tenant_id: Uuid, email: &str, role: &str) -> String {
    let secret = JwtSecret(TEST_JWT_SECRET.to_string());
    let user = User {
        id: user_id,
        tenant_id,
        google_sub: Some("test-google-sub".to_string()),
        lineworks_id: None,
        email: email.to_string(),
        name: "Test User".to_string(),
        role: role.to_string(),
        refresh_token_hash: None,
        refresh_token_expires_at: None,
        created_at: chrono::Utc::now(),
    };
    create_access_token(&user, &secret, None).expect("Failed to create test JWT")
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

    let employees = Arc::new(PgEmployeeRepository::new(pool.clone()));
    let nfc_tags = Arc::new(PgNfcTagRepository::new(pool.clone()));

    AppState {
        pool,
        employees,
        nfc_tags,
        storage,
        carins_storage: None,
        dtako_storage: Some(dtako_storage),
        fcm: Some(mock_fcm),
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

    let employees = Arc::new(PgEmployeeRepository::new(pool.clone()));
    let nfc_tags = Arc::new(PgNfcTagRepository::new(pool.clone()));

    AppState {
        pool,
        employees,
        nfc_tags,
        storage,
        carins_storage: None,
        dtako_storage: Some(dtako_storage),
        fcm: None,
    }
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

    let employees = Arc::new(PgEmployeeRepository::new(pool.clone()));
    let nfc_tags = Arc::new(PgNfcTagRepository::new(pool.clone()));

    AppState {
        pool,
        employees,
        nfc_tags,
        storage,
        carins_storage: None,
        dtako_storage: Some(dtako_storage),
        fcm: Some(failing_fcm),
    }
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

/// テスト用 JWT を発行
pub fn create_test_jwt(tenant_id: Uuid, role: &str) -> String {
    let secret = JwtSecret(TEST_JWT_SECRET.to_string());
    let user = User {
        id: Uuid::new_v4(),
        tenant_id,
        google_sub: Some("test-google-sub".to_string()),
        lineworks_id: None,
        email: "test@example.com".to_string(),
        name: "Test User".to_string(),
        role: role.to_string(),
        refresh_token_hash: None,
        refresh_token_expires_at: None,
        created_at: chrono::Utc::now(),
    };
    create_access_token(&user, &secret, None).expect("Failed to create test JWT")
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
pub async fn spawn_test_server(state: AppState) -> String {
    spawn_test_server_with_scraper(state, "http://localhost:9999").await
}

/// テスト用 axum サーバーを起動し、base URL を返す (scraper URL 指定)
pub async fn spawn_test_server_with_scraper(state: AppState, scraper_url: &str) -> String {
    use axum::{Extension, Router};
    use rust_alc_api::auth::google::GoogleTokenVerifier;
    use rust_alc_api::auth::jwt::JwtSecret;
    use rust_alc_api::routes::dtako_scraper::ScraperUrl;
    use tower_http::cors::{Any, CorsLayer};

    let jwt_secret = JwtSecret(TEST_JWT_SECRET.to_string());
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
        .nest("/api", rust_alc_api::routes::router())
        .layer(Extension(google_verifier))
        .layer(Extension(jwt_secret))
        .layer(Extension(ScraperUrl(scraper_url.to_string())))
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
