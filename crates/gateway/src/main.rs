mod auth;
mod config;
mod proxy;
mod routes;

use std::net::SocketAddr;
use std::time::Duration;

use axum::{routing::get, Router};
use reqwest::Client;
use tower_http::{
    cors::{Any, CorsLayer},
    trace::TraceLayer,
};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::config::Config;
use crate::proxy::ProxyState;

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "gateway=info,tower_http=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let config = Config::from_env();

    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .pool_max_idle_per_host(20)
        .build()
        .expect("Failed to create HTTP client");

    let proxy_state = ProxyState {
        client,
        backend_url: config.backend_url.clone(),
        jwt_secret: config.jwt_secret,
    };

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        .route("/health", get(health))
        .fallback(proxy::proxy_handler)
        .with_state(proxy_state)
        .layer(cors)
        .layer(TraceLayer::new_for_http());

    let addr = SocketAddr::from(([0, 0, 0, 0], config.port));
    tracing::info!(
        "Gateway listening on {addr}, backend: {}",
        config.backend_url
    );

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn health() -> &'static str {
    "ok"
}
