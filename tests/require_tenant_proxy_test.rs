//! `require_tenant` の proxy-secret gate (Refs #434) の DB 必須分岐の統合テスト。
//!
//! DB 不要な分岐 (gate 有効 + proxy header 欠落/不一致 → 401) は
//! `crates/alc-core/src/auth_middleware.rs` の unit test 側で確認する。
//! ここでは実 DB を使い、「gate 有効 + 正しい proxy secret + 実在 tenant → 200」
//! (= proxy 経由の正規アクセスが通る経路) をカバーする。

#[macro_use]
mod common;

use axum::{middleware as axum_middleware, routing::get, Extension, Router};
use rust_alc_api::auth::jwt::JwtSecret;
use rust_alc_api::middleware::auth::{
    require_tenant, TenantId, TenantProxySecret, TenantValidationPool,
};
use uuid::Uuid;

const PROXY_SECRET: &str = "test-proxy-secret-value-32-bytes";

async fn echo_tenant(Extension(tid): Extension<TenantId>) -> String {
    tid.0.to_string()
}

/// gate を有効化した (proxy secret 設定済み) require_tenant をローカル port に spawn。
async fn spawn(pool: sqlx::PgPool) -> String {
    let app = Router::new()
        .route("/t", get(echo_tenant))
        .layer(axum_middleware::from_fn(require_tenant))
        .layer(Extension(TenantValidationPool(Some(pool))))
        .layer(Extension(TenantProxySecret(PROXY_SECRET.to_string())))
        .layer(Extension(JwtSecret(common::TEST_JWT_SECRET.to_string())));

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

#[tokio::test]
async fn proxy_secret_match_with_existing_tenant_ok() {
    let state = common::setup_app_state().await;
    let tenant = common::create_test_tenant(state.pool(), "RTP proxy ok").await;
    let base = spawn(state.pool().clone()).await;

    let resp = reqwest::Client::new()
        .get(format!("{base}/t"))
        .header("X-Tenant-ID", tenant.to_string())
        .header("X-Tenant-Proxy-Secret", PROXY_SECRET)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.text().await.unwrap(), tenant.to_string());
}
