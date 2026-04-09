use std::net::SocketAddr;
use std::sync::Arc;

use axum::{middleware as axum_middleware, Router};
use sqlx::postgres::PgPoolOptions;
use tower_http::{
    cors::{Any, CorsLayer},
    trace::TraceLayer,
};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use alc_carins::repo::{PgCarInspectionRepository, PgCarinsFilesRepository, PgNfcTagRepository};
use alc_carins::CarinsState;
use alc_core::auth_middleware::require_tenant_header;

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "carins_api=info,tower_http=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL is required");
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(8080);

    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(&database_url)
        .await
        .expect("Failed to connect to database");

    // R2 storage for carins files
    let storage = {
        let bucket = std::env::var("CARINS_R2_BUCKET").expect("CARINS_R2_BUCKET is required");
        let account_id =
            std::env::var("CARINS_R2_ACCOUNT_ID").expect("CARINS_R2_ACCOUNT_ID is required");
        let access_key =
            std::env::var("CARINS_R2_ACCESS_KEY").expect("CARINS_R2_ACCESS_KEY is required");
        let secret_key =
            std::env::var("CARINS_R2_SECRET_KEY").expect("CARINS_R2_SECRET_KEY is required");
        let public_url = std::env::var("CARINS_R2_PUBLIC_URL").ok();

        alc_storage::R2Backend::new(bucket, account_id, access_key, secret_key, public_url)
            .expect("Failed to create R2 storage")
    };

    let state = CarinsState {
        car_inspections: Arc::new(PgCarInspectionRepository::new(pool.clone())),
        carins_files: Arc::new(PgCarinsFilesRepository::new(pool.clone())),
        nfc_tags: Arc::new(PgNfcTagRepository::new(pool.clone())),
        storage: Arc::new(storage),
    };

    let tenant_protected = Router::new()
        .merge(alc_carins::car_inspections::tenant_router())
        .merge(alc_carins::car_inspection_files::tenant_router())
        .merge(alc_carins::carins_files::tenant_router())
        .merge(alc_carins::nfc_tags::tenant_router())
        .layer(axum_middleware::from_fn(require_tenant_header));

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        .route("/health", axum::routing::get(|| async { "ok" }))
        .merge(tenant_protected)
        .with_state(state)
        .layer(cors)
        .layer(TraceLayer::new_for_http());

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!("carins-api listening on {addr}");

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
