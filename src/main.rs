use std::sync::Arc;

use axum::{Extension, Router};
use sqlx::postgres::PgPoolOptions;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;

use rust_alc_api::auth::google::GoogleTokenVerifier;
use rust_alc_api::auth::jwt::JwtSecret;
use rust_alc_api::db::repository::{
    PgAuthRepository, PgBotAdminRepository, PgCarInspectionRepository, PgCarinsFilesRepository,
    PgCarryingItemsRepository, PgCommunicationItemsRepository, PgDailyHealthRepository,
    PgDeviceRepository, PgDriverInfoRepository, PgDtakoCsvProxyRepository,
    PgDtakoDailyHoursRepository, PgDtakoDriversRepository, PgDtakoEventClassificationsRepository,
    PgDtakoOperationsRepository, PgDtakoRestraintReportPdfRepository,
    PgDtakoRestraintReportRepository, PgDtakoScraperRepository, PgDtakoUploadRepository,
    PgDtakoVehiclesRepository, PgDtakoWorkTimesRepository, PgEmployeeRepository,
    PgEquipmentFailuresRepository, PgGuidanceRecordsRepository, PgHealthBaselinesRepository,
    PgMeasurementsRepository, PgNfcTagRepository, PgSsoAdminRepository, PgTenantUsersRepository,
    PgTenkoCallRepository, PgTenkoRecordsRepository, PgTenkoSchedulesRepository,
    PgTenkoSessionRepository, PgTenkoWebhooksRepository, PgTimecardRepository,
};
use rust_alc_api::storage::StorageBackend;
use rust_alc_api::AppState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    dotenvy::dotenv().ok();

    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let port: u16 = std::env::var("PORT")
        .unwrap_or_else(|_| "8080".into())
        .parse()
        .expect("PORT must be a number");

    // Google OAuth + JWT 設定
    let google_client_id = std::env::var("GOOGLE_CLIENT_ID").expect("GOOGLE_CLIENT_ID must be set");
    let jwt_secret = std::env::var("JWT_SECRET").expect("JWT_SECRET must be set");

    let google_client_secret =
        std::env::var("GOOGLE_CLIENT_SECRET").expect("GOOGLE_CLIENT_SECRET must be set");

    let google_verifier = GoogleTokenVerifier::new(google_client_id, google_client_secret);
    let jwt_secret = JwtSecret(jwt_secret);

    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(&database_url)
        .await?;

    // Storage backend selection
    let storage_backend = std::env::var("STORAGE_BACKEND").unwrap_or_else(|_| "gcs".into());
    let storage: Arc<dyn StorageBackend> = match storage_backend.as_str() {
        "r2" => {
            let bucket =
                std::env::var("R2_BUCKET").expect("R2_BUCKET required when STORAGE_BACKEND=r2");
            let account_id = std::env::var("R2_ACCOUNT_ID")
                .expect("R2_ACCOUNT_ID required when STORAGE_BACKEND=r2");
            let access_key = std::env::var("R2_ACCESS_KEY")
                .expect("R2_ACCESS_KEY required when STORAGE_BACKEND=r2");
            let secret_key = std::env::var("R2_SECRET_KEY")
                .expect("R2_SECRET_KEY required when STORAGE_BACKEND=r2");
            let public_url = std::env::var("R2_PUBLIC_URL_BASE").ok();

            tracing::info!("Storage backend: R2 (bucket={})", bucket);
            Arc::new(
                rust_alc_api::storage::R2Backend::new(
                    bucket, account_id, access_key, secret_key, public_url,
                )
                .expect("Failed to initialize R2 backend"),
            )
        }
        _ => {
            let bucket = std::env::var("GCS_BUCKET").unwrap_or_else(|_| "alc-face-photos".into());
            tracing::info!("Storage backend: GCS (bucket={})", bucket);
            Arc::new(rust_alc_api::storage::GcsBackend::new(bucket))
        }
    };

    // carins ファイル用 R2 (carins-files バケット、別 API トークン)
    let carins_storage: Option<Arc<dyn StorageBackend>> =
        std::env::var("CARINS_R2_BUCKET").ok().map(|bucket| {
            let account_id = std::env::var("R2_ACCOUNT_ID")
                .expect("R2_ACCOUNT_ID required for CARINS_R2_BUCKET");
            let access_key =
                std::env::var("CARINS_R2_ACCESS_KEY").expect("CARINS_R2_ACCESS_KEY required");
            let secret_key =
                std::env::var("CARINS_R2_SECRET_KEY").expect("CARINS_R2_SECRET_KEY required");
            tracing::info!("Carins storage: R2 (bucket={})", bucket);
            Arc::new(
                rust_alc_api::storage::R2Backend::new(
                    bucket, account_id, access_key, secret_key, None,
                )
                .expect("Failed to init carins R2 backend"),
            ) as Arc<dyn StorageBackend>
        });

    // dtako (digitacho) 用 R2 (ohishi-dtako バケット、別 API トークン)
    let dtako_storage: Option<Arc<dyn StorageBackend>> =
        std::env::var("DTAKO_R2_BUCKET").ok().map(|bucket| {
            let account_id =
                std::env::var("R2_ACCOUNT_ID").expect("R2_ACCOUNT_ID required for DTAKO_R2_BUCKET");
            let access_key =
                std::env::var("DTAKO_R2_ACCESS_KEY").expect("DTAKO_R2_ACCESS_KEY required");
            let secret_key =
                std::env::var("DTAKO_R2_SECRET_KEY").expect("DTAKO_R2_SECRET_KEY required");
            tracing::info!("Dtako storage: R2 (bucket={})", bucket);
            Arc::new(
                rust_alc_api::storage::R2Backend::new(
                    bucket, account_id, access_key, secret_key, None,
                )
                .expect("Failed to init dtako R2 backend"),
            ) as Arc<dyn StorageBackend>
        });

    // FCM (optional — disabled if FCM_PROJECT_ID is not set)
    let fcm = std::env::var("FCM_PROJECT_ID").ok().map(|project_id| {
        tracing::info!("FCM enabled (project={})", project_id);
        Arc::new(rust_alc_api::fcm::FcmSender::new(project_id))
            as Arc<dyn rust_alc_api::fcm::FcmSenderTrait>
    });

    let auth = Arc::new(PgAuthRepository::new(pool.clone()));
    let bot_admin = Arc::new(PgBotAdminRepository::new(pool.clone()));
    let car_inspections = Arc::new(PgCarInspectionRepository::new(pool.clone()));
    let carins_files = Arc::new(PgCarinsFilesRepository::new(pool.clone()));
    let carrying_items = Arc::new(PgCarryingItemsRepository::new(pool.clone()));
    let communication_items = Arc::new(PgCommunicationItemsRepository::new(pool.clone()));
    let daily_health = Arc::new(PgDailyHealthRepository::new(pool.clone()));
    let devices = Arc::new(PgDeviceRepository::new(pool.clone()));
    let driver_info = Arc::new(PgDriverInfoRepository::new(pool.clone()));
    let dtako_csv_proxy = Arc::new(PgDtakoCsvProxyRepository::new(pool.clone()));
    let dtako_daily_hours = Arc::new(PgDtakoDailyHoursRepository::new(pool.clone()));
    let dtako_drivers = Arc::new(PgDtakoDriversRepository::new(pool.clone()));
    let dtako_event_classifications =
        Arc::new(PgDtakoEventClassificationsRepository::new(pool.clone()));
    let dtako_operations = Arc::new(PgDtakoOperationsRepository::new(pool.clone()));
    let dtako_restraint_report = Arc::new(PgDtakoRestraintReportRepository::new(pool.clone()));
    let dtako_restraint_report_pdf =
        Arc::new(PgDtakoRestraintReportPdfRepository::new(pool.clone()));
    let dtako_scraper = Arc::new(PgDtakoScraperRepository::new(pool.clone()));
    let dtako_upload = Arc::new(PgDtakoUploadRepository::new(pool.clone()));
    let dtako_vehicles = Arc::new(PgDtakoVehiclesRepository::new(pool.clone()));
    let dtako_work_times = Arc::new(PgDtakoWorkTimesRepository::new(pool.clone()));
    let employees = Arc::new(PgEmployeeRepository::new(pool.clone()));
    let equipment_failures = Arc::new(PgEquipmentFailuresRepository::new(pool.clone()));
    let guidance_records = Arc::new(PgGuidanceRecordsRepository::new(pool.clone()));
    let health_baselines = Arc::new(PgHealthBaselinesRepository::new(pool.clone()));
    let measurements = Arc::new(PgMeasurementsRepository::new(pool.clone()));
    let nfc_tags = Arc::new(PgNfcTagRepository::new(pool.clone()));
    let sso_admin = Arc::new(PgSsoAdminRepository::new(pool.clone()));
    let tenant_users = Arc::new(PgTenantUsersRepository::new(pool.clone()));
    let tenko_call = Arc::new(PgTenkoCallRepository::new(pool.clone()));
    let tenko_records = Arc::new(PgTenkoRecordsRepository::new(pool.clone()));
    let tenko_schedules = Arc::new(PgTenkoSchedulesRepository::new(pool.clone()));
    let tenko_sessions = Arc::new(PgTenkoSessionRepository::new(pool.clone()));
    let tenko_webhooks = Arc::new(PgTenkoWebhooksRepository::new(pool.clone()));
    let timecard = Arc::new(PgTimecardRepository::new(pool.clone()));

    let state = AppState {
        pool: Some(pool.clone()),
        auth,
        bot_admin,
        car_inspections,
        carins_files,
        carrying_items,
        communication_items,
        daily_health,
        devices,
        driver_info,
        dtako_csv_proxy,
        dtako_daily_hours,
        dtako_drivers,
        dtako_event_classifications,
        dtako_operations,
        dtako_restraint_report,
        dtako_restraint_report_pdf,
        dtako_scraper,
        dtako_upload,
        dtako_vehicles,
        dtako_work_times,
        employees,
        equipment_failures,
        guidance_records,
        health_baselines,
        measurements,
        nfc_tags,
        sso_admin,
        tenant_users,
        tenko_call,
        tenko_records,
        tenko_schedules,
        tenko_sessions,
        tenko_webhooks,
        timecard,
        storage,
        carins_storage,
        dtako_storage,
        fcm,
    };

    // 点呼予定超過チェック バックグラウンドタスク
    let overdue_pool = pool;
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
        loop {
            interval.tick().await;
            if let Err(e) = rust_alc_api::webhook::check_overdue_schedules(&overdue_pool).await {
                tracing::error!("Overdue check failed: {e}");
            }
        }
    });

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    // Scraper URL (optional — dtako-scraper Cloud Run)
    let scraper_url =
        std::env::var("SCRAPER_URL").unwrap_or_else(|_| "http://localhost:8081".into());
    tracing::info!("Scraper URL: {}", scraper_url);

    let app = Router::new()
        .nest("/api", rust_alc_api::routes::router())
        .layer(Extension(google_verifier))
        .layer(Extension(jwt_secret))
        .layer(Extension(rust_alc_api::routes::dtako_scraper::ScraperUrl(
            scraper_url,
        )))
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port)).await?;
    tracing::info!("listening on 0.0.0.0:{port}");
    axum::serve(listener, app).await?;

    Ok(())
}
