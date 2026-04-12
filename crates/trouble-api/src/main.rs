use std::net::SocketAddr;
use std::sync::Arc;

use axum::{middleware as axum_middleware, Router};
use sqlx::postgres::PgPoolOptions;
use tower_http::{
    cors::{Any, CorsLayer},
    trace::TraceLayer,
};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use axum::Extension;

use alc_core::auth_middleware::require_tenant_header;
use alc_core::repository::bot_admin::BotAdminRepository;
use alc_misc::repo::PgBotAdminRepository;
use alc_notify::clients::lineworks::LineworksBotClient;
use alc_trouble::repo::{
    trouble_categories::PgTroubleCategoriesRepository,
    trouble_comments::PgTroubleCommentsRepository, trouble_files::PgTroubleFilesRepository,
    trouble_notification_prefs::PgTroubleNotificationPrefsRepository,
    trouble_offices::PgTroubleOfficesRepository,
    trouble_progress_statuses::PgTroubleProgressStatusesRepository,
    trouble_schedules::PgTroubleSchedulesRepository, trouble_tickets::PgTroubleTicketsRepository,
    trouble_workflow::PgTroubleWorkflowRepository,
};
use alc_trouble::TroubleState;

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "trouble_api=info,tower_http=info".into()),
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

    // trouble_storage は R2 バケットが設定されている場合のみ有効化
    let trouble_storage: Option<Arc<dyn alc_core::storage::StorageBackend>> =
        std::env::var("TROUBLE_R2_BUCKET").ok().map(|bucket| {
            let account_id = std::env::var("R2_ACCOUNT_ID")
                .expect("R2_ACCOUNT_ID required for TROUBLE_R2_BUCKET");
            let access_key =
                std::env::var("TROUBLE_R2_ACCESS_KEY").expect("TROUBLE_R2_ACCESS_KEY required");
            let secret_key =
                std::env::var("TROUBLE_R2_SECRET_KEY").expect("TROUBLE_R2_SECRET_KEY required");
            tracing::info!("Trouble storage: R2 (bucket={})", bucket);
            Arc::new(
                alc_storage::R2Backend::new(bucket, account_id, access_key, secret_key, None)
                    .expect("Failed to init trouble R2 backend"),
            ) as Arc<dyn alc_core::storage::StorageBackend>
        });

    let state = TroubleState {
        trouble_tickets: Arc::new(PgTroubleTicketsRepository::new(pool.clone())),
        trouble_files: Arc::new(PgTroubleFilesRepository::new(pool.clone())),
        trouble_workflow: Arc::new(PgTroubleWorkflowRepository::new(pool.clone())),
        trouble_comments: Arc::new(PgTroubleCommentsRepository::new(pool.clone())),
        trouble_categories: Arc::new(PgTroubleCategoriesRepository::new(pool.clone())),
        trouble_offices: Arc::new(PgTroubleOfficesRepository::new(pool.clone())),
        trouble_progress_statuses: Arc::new(PgTroubleProgressStatusesRepository::new(pool.clone())),
        trouble_notification_prefs: Arc::new(PgTroubleNotificationPrefsRepository::new(
            pool.clone(),
        )),
        trouble_schedules: Arc::new(PgTroubleSchedulesRepository::new(pool.clone())),
        trouble_storage,
        webhook: None,
        cloud_tasks: None,
        notifier: None,
    };

    // LINE WORKS メンバー一覧用
    let bot_admin: Arc<dyn BotAdminRepository> = Arc::new(PgBotAdminRepository::new(pool.clone()));
    let lw_client = Arc::new(LineworksBotClient::new());

    let tenant_protected = Router::new()
        .merge(alc_trouble::tickets::tenant_router())
        .merge(alc_trouble::files::tenant_router())
        .merge(alc_trouble::workflow::tenant_router())
        .merge(alc_trouble::comments::tenant_router())
        .merge(alc_trouble::categories::tenant_router())
        .merge(alc_trouble::offices::tenant_router())
        .merge(alc_trouble::progress_statuses::tenant_router())
        .merge(alc_trouble::notifications::tenant_router())
        .merge(alc_trouble::schedules::tenant_router())
        .merge(alc_trouble::lineworks_members::tenant_router())
        .layer(Extension(bot_admin))
        .layer(Extension(lw_client))
        .layer(axum_middleware::from_fn(require_tenant_header));

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        .route("/health", axum::routing::get(|| async { "ok" }))
        .merge(alc_trouble::schedules::fire_router())
        .merge(tenant_protected)
        .with_state(state)
        .layer(axum::extract::DefaultBodyLimit::max(20 * 1024 * 1024)) // 20MB
        .layer(cors)
        .layer(TraceLayer::new_for_http());

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!("trouble-api listening on {addr}");

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
