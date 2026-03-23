mod auth;
pub mod compare;
pub mod csv_parser;
mod db;
pub mod fcm;
mod middleware;
mod routes;
mod storage;
pub mod webhook;

use std::sync::Arc;

use axum::{Extension, Router};
use sqlx::postgres::PgPoolOptions;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;

use crate::auth::google::GoogleTokenVerifier;
use crate::auth::jwt::JwtSecret;
use crate::storage::StorageBackend;

#[derive(Clone)]
pub struct AppState {
    pub pool: sqlx::PgPool,
    pub storage: Arc<dyn StorageBackend>,
    /// carins ファイル用 R2 ストレージ (carins-files バケット)
    pub carins_storage: Option<Arc<dyn StorageBackend>>,
    /// dtako (digitacho) 用 R2 ストレージ (ohishi-dtako バケット)
    pub dtako_storage: Option<Arc<dyn StorageBackend>>,
    pub fcm: Option<Arc<fcm::FcmSender>>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    dotenvy::dotenv().ok();

    let database_url =
        std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let port: u16 = std::env::var("PORT")
        .unwrap_or_else(|_| "8080".into())
        .parse()
        .expect("PORT must be a number");

    // Google OAuth + JWT 設定
    let google_client_id =
        std::env::var("GOOGLE_CLIENT_ID").expect("GOOGLE_CLIENT_ID must be set");
    let jwt_secret =
        std::env::var("JWT_SECRET").expect("JWT_SECRET must be set");

    let google_client_secret =
        std::env::var("GOOGLE_CLIENT_SECRET").expect("GOOGLE_CLIENT_SECRET must be set");

    let google_verifier = GoogleTokenVerifier::new(google_client_id, google_client_secret);
    let jwt_secret = JwtSecret(jwt_secret);

    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(&database_url)
        .await?;

    sqlx::migrate!("./migrations").run(&pool).await?;

    // Storage backend selection
    let storage_backend = std::env::var("STORAGE_BACKEND").unwrap_or_else(|_| "gcs".into());
    let storage: Arc<dyn StorageBackend> = match storage_backend.as_str() {
        "r2" => {
            let bucket = std::env::var("R2_BUCKET")
                .expect("R2_BUCKET required when STORAGE_BACKEND=r2");
            let account_id = std::env::var("R2_ACCOUNT_ID")
                .expect("R2_ACCOUNT_ID required when STORAGE_BACKEND=r2");
            let access_key = std::env::var("R2_ACCESS_KEY")
                .expect("R2_ACCESS_KEY required when STORAGE_BACKEND=r2");
            let secret_key = std::env::var("R2_SECRET_KEY")
                .expect("R2_SECRET_KEY required when STORAGE_BACKEND=r2");
            let public_url = std::env::var("R2_PUBLIC_URL_BASE").ok();

            tracing::info!("Storage backend: R2 (bucket={})", bucket);
            Arc::new(
                storage::R2Backend::new(bucket, account_id, access_key, secret_key, public_url)
                    .expect("Failed to initialize R2 backend"),
            )
        }
        _ => {
            let bucket =
                std::env::var("GCS_BUCKET").unwrap_or_else(|_| "alc-face-photos".into());
            tracing::info!("Storage backend: GCS (bucket={})", bucket);
            Arc::new(storage::GcsBackend::new(bucket))
        }
    };

    // carins ファイル用 R2 (carins-files バケット、別 API トークン)
    let carins_storage: Option<Arc<dyn StorageBackend>> =
        std::env::var("CARINS_R2_BUCKET").ok().map(|bucket| {
            let account_id = std::env::var("R2_ACCOUNT_ID")
                .expect("R2_ACCOUNT_ID required for CARINS_R2_BUCKET");
            let access_key = std::env::var("CARINS_R2_ACCESS_KEY")
                .expect("CARINS_R2_ACCESS_KEY required");
            let secret_key = std::env::var("CARINS_R2_SECRET_KEY")
                .expect("CARINS_R2_SECRET_KEY required");
            tracing::info!("Carins storage: R2 (bucket={})", bucket);
            Arc::new(
                storage::R2Backend::new(bucket, account_id, access_key, secret_key, None)
                    .expect("Failed to init carins R2 backend"),
            ) as Arc<dyn StorageBackend>
        });

    // dtako (digitacho) 用 R2 (ohishi-dtako バケット、別 API トークン)
    let dtako_storage: Option<Arc<dyn StorageBackend>> =
        std::env::var("DTAKO_R2_BUCKET").ok().map(|bucket| {
            let account_id = std::env::var("R2_ACCOUNT_ID")
                .expect("R2_ACCOUNT_ID required for DTAKO_R2_BUCKET");
            let access_key = std::env::var("DTAKO_R2_ACCESS_KEY")
                .expect("DTAKO_R2_ACCESS_KEY required");
            let secret_key = std::env::var("DTAKO_R2_SECRET_KEY")
                .expect("DTAKO_R2_SECRET_KEY required");
            tracing::info!("Dtako storage: R2 (bucket={})", bucket);
            Arc::new(
                storage::R2Backend::new(bucket, account_id, access_key, secret_key, None)
                    .expect("Failed to init dtako R2 backend"),
            ) as Arc<dyn StorageBackend>
        });

    // FCM (optional — disabled if FCM_PROJECT_ID is not set)
    let fcm = std::env::var("FCM_PROJECT_ID").ok().map(|project_id| {
        tracing::info!("FCM enabled (project={})", project_id);
        Arc::new(fcm::FcmSender::new(project_id))
    });

    let state = AppState { pool: pool.clone(), storage, carins_storage, dtako_storage, fcm };

    // 点呼予定超過チェック バックグラウンドタスク
    let overdue_pool = pool;
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
        loop {
            interval.tick().await;
            if let Err(e) = webhook::check_overdue_schedules(&overdue_pool).await {
                tracing::error!("Overdue check failed: {e}");
            }
        }
    });

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    // Scraper URL (optional — dtako-scraper Cloud Run)
    let scraper_url = std::env::var("SCRAPER_URL")
        .unwrap_or_else(|_| "http://localhost:8081".into());
    tracing::info!("Scraper URL: {}", scraper_url);

    let app = Router::new()
        .nest("/api", routes::router())
        .layer(Extension(google_verifier))
        .layer(Extension(jwt_secret))
        .layer(Extension(routes::dtako_scraper::ScraperUrl(scraper_url)))
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port)).await?;
    tracing::info!("listening on 0.0.0.0:{port}");
    axum::serve(listener, app).await?;

    Ok(())
}
