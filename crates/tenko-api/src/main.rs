use std::net::SocketAddr;
use std::sync::Arc;

use axum::{middleware as axum_middleware, Router};
use sqlx::postgres::PgPoolOptions;
use tower_http::{
    cors::{Any, CorsLayer},
    trace::TraceLayer,
};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use alc_core::auth_middleware::require_tenant_header;
use alc_tenko::repo::{
    PgDailyHealthRepository, PgEquipmentFailuresRepository, PgHealthBaselinesRepository,
    PgTenkoCallRepository, PgTenkoRecordsRepository, PgTenkoSchedulesRepository,
    PgTenkoSessionRepository, PgTenkoWebhooksRepository,
};
use alc_tenko::TenkoState;

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "tenko_api=info,tower_http=info".into()),
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

    let state = TenkoState {
        tenko_call: Arc::new(PgTenkoCallRepository::new(pool.clone())),
        tenko_records: Arc::new(PgTenkoRecordsRepository::new(pool.clone())),
        tenko_schedules: Arc::new(PgTenkoSchedulesRepository::new(pool.clone())),
        tenko_sessions: Arc::new(PgTenkoSessionRepository::new(pool.clone())),
        tenko_webhooks: Arc::new(PgTenkoWebhooksRepository::new(pool.clone())),
        daily_health: Arc::new(PgDailyHealthRepository::new(pool.clone())),
        health_baselines: Arc::new(PgHealthBaselinesRepository::new(pool.clone())),
        equipment_failures: Arc::new(PgEquipmentFailuresRepository::new(pool.clone())),
        webhook: None,
    };

    let tenant_protected = Router::new()
        .merge(alc_tenko::tenko_schedules::tenant_router())
        .merge(alc_tenko::tenko_sessions::tenant_router())
        .merge(alc_tenko::tenko_records::tenant_router())
        .merge(alc_tenko::health_baselines::tenant_router())
        .merge(alc_tenko::equipment_failures::tenant_router())
        .merge(alc_tenko::tenko_webhooks::tenant_router())
        .merge(alc_tenko::tenko_call::tenant_router())
        .merge(alc_tenko::daily_health::tenant_router())
        .layer(axum_middleware::from_fn(require_tenant_header));

    let public_routes = Router::new().merge(alc_tenko::tenko_call::public_router());

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        .route("/health", axum::routing::get(|| async { "ok" }))
        .merge(public_routes)
        .merge(tenant_protected)
        .with_state(state)
        .layer(cors)
        .layer(TraceLayer::new_for_http());

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!("tenko-api listening on {addr}");

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
