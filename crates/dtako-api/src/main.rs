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
use alc_dtako::repo::{
    PgDtakoCsvProxyRepository, PgDtakoDailyHoursRepository, PgDtakoDriversRepository,
    PgDtakoEventClassificationsRepository, PgDtakoLogsRepository, PgDtakoOperationsRepository,
    PgDtakoRestraintReportPdfRepository, PgDtakoRestraintReportRepository,
    PgDtakoScraperRepository, PgDtakoUploadRepository, PgDtakoVehiclesRepository,
    PgDtakoWorkTimesRepository,
};
use alc_dtako::DtakoState;

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "dtako_api=info,tower_http=info".into()),
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

    // R2 storage (optional)
    let dtako_storage = match (
        std::env::var("DTAKO_R2_BUCKET"),
        std::env::var("CF_ACCOUNT_ID"),
        std::env::var("DTAKO_R2_ACCESS_KEY"),
        std::env::var("DTAKO_R2_SECRET_KEY"),
    ) {
        (Ok(bucket), Ok(account_id), Ok(access_key), Ok(secret_key)) => {
            match alc_storage::R2Backend::new(bucket, account_id, access_key, secret_key, None) {
                Ok(backend) => {
                    tracing::info!("DTAKO R2 storage configured");
                    Some(Arc::new(backend) as Arc<dyn alc_core::storage::StorageBackend>)
                }
                Err(e) => {
                    tracing::warn!("Failed to create R2 backend: {e}");
                    None
                }
            }
        }
        _ => {
            tracing::info!("DTAKO R2 storage not configured (env vars missing)");
            None
        }
    };

    let state = DtakoState {
        dtako_csv_proxy: Arc::new(PgDtakoCsvProxyRepository::new(pool.clone())),
        dtako_daily_hours: Arc::new(PgDtakoDailyHoursRepository::new(pool.clone())),
        dtako_drivers: Arc::new(PgDtakoDriversRepository::new(pool.clone())),
        dtako_event_classifications: Arc::new(PgDtakoEventClassificationsRepository::new(
            pool.clone(),
        )),
        dtako_logs: Arc::new(PgDtakoLogsRepository::new(pool.clone())),
        dtako_operations: Arc::new(PgDtakoOperationsRepository::new(pool.clone())),
        dtako_restraint_report: Arc::new(PgDtakoRestraintReportRepository::new(pool.clone())),
        dtako_restraint_report_pdf: Arc::new(PgDtakoRestraintReportPdfRepository::new(
            pool.clone(),
        )),
        dtako_scraper: Arc::new(PgDtakoScraperRepository::new(pool.clone())),
        dtako_upload: Arc::new(PgDtakoUploadRepository::new(pool.clone())),
        dtako_vehicles: Arc::new(PgDtakoVehiclesRepository::new(pool.clone())),
        dtako_work_times: Arc::new(PgDtakoWorkTimesRepository::new(pool.clone())),
        dtako_storage,
    };

    let tenant_protected = Router::new()
        .merge(alc_dtako::dtako_csv_proxy::tenant_router())
        .merge(alc_dtako::dtako_daily_hours::tenant_router())
        .merge(alc_dtako::dtako_drivers::tenant_router())
        .merge(alc_dtako::dtako_event_classifications::tenant_router())
        .merge(alc_dtako::dtako_operations::tenant_router())
        .merge(alc_dtako::dtako_restraint_report::tenant_router())
        .merge(alc_dtako::dtako_restraint_report_pdf::tenant_router())
        .merge(alc_dtako::dtako_scraper::tenant_router())
        .merge(alc_dtako::dtako_upload::tenant_router())
        .merge(alc_dtako::dtako_vehicles::tenant_router())
        .merge(alc_dtako::dtako_work_times::tenant_router())
        .nest("/dtako-logs", alc_dtako::dtako_logs::tenant_router())
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
    tracing::info!("dtako-api listening on {addr}");

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
