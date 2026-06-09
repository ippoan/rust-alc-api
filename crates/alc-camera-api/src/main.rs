//! 監視カメラ死活管理 API バイナリ (Refs #345)。
//!
//! gateway が `/cameras*` をこのサービスへ proxy する。カメラマスタの CRUD、
//! device からのヘルスログ受信、ステータス集計を提供し、連続失敗を alc-trouble に
//! 自動起票する。

use std::net::SocketAddr;
use std::sync::Arc;

use axum::{middleware as axum_middleware, Router};
use sqlx::postgres::PgPoolOptions;
use tower_http::{
    cors::{Any, CorsLayer},
    trace::TraceLayer,
};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use alc_camera::repo::PgCamerasRepository;
use alc_camera::{CameraState, DEFAULT_DOWN_THRESHOLD};
use alc_core::auth_middleware::require_tenant_header;
use alc_trouble::repo::trouble_tickets::PgTroubleTicketsRepository;

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "alc_camera_api=info,alc_camera=info,tower_http=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL is required");
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(8080);
    let down_threshold: usize = std::env::var("CAMERA_DOWN_THRESHOLD")
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|n| *n >= 1)
        .unwrap_or(DEFAULT_DOWN_THRESHOLD);

    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(&database_url)
        .await
        .expect("Failed to connect to database");

    let state = CameraState {
        cameras: Arc::new(PgCamerasRepository::new(pool.clone())),
        trouble_tickets: Arc::new(PgTroubleTicketsRepository::new(pool.clone())),
        down_threshold,
    };

    let tenant_protected = Router::new()
        .merge(alc_camera::handlers::tenant_router())
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
    tracing::info!("alc-camera-api listening on {addr} (down_threshold={down_threshold})");

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
