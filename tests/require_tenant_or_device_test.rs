//! `require_tenant_or_device` ミドルウェア (Refs #434) の DB 必須分岐の統合テスト。
//!
//! DB 不要な分岐 (bare X-Tenant-ID → 401 / device token 欠落 → 401 / pool=None
//! fail-closed) は `crates/alc-core/src/auth_middleware.rs` の unit test 側で確認する。
//! ここでは実 DB を使い、JWT + 実在 tenant 検証と実 device token 検証
//! (`alc_api.verify_device_token`, migration 116) を網羅する。

#[macro_use]
mod common;

use axum::{middleware as axum_middleware, routing::get, Extension, Router};
use rust_alc_api::auth::jwt::JwtSecret;
use rust_alc_api::middleware::auth::{require_tenant_or_device, TenantId, TenantValidationPool};
use uuid::Uuid;

async fn echo_tenant(Extension(tid): Extension<TenantId>) -> String {
    tid.0.to_string()
}

/// 指定 pool で require_tenant_or_device を layer した最小アプリをローカル port に
/// spawn し、base URL を返す。本番 route には配線せず、ミドルウェア単体を検証する。
async fn spawn(pool: sqlx::PgPool) -> String {
    let app = Router::new()
        .route("/t", get(echo_tenant))
        .layer(axum_middleware::from_fn(require_tenant_or_device))
        .layer(Extension(TenantValidationPool(Some(pool))))
        .layer(Extension(JwtSecret(common::TEST_JWT_SECRET.to_string())));

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

/// status='active' な device を直接 INSERT して settings_token を仕込む
/// (テストは postgres superuser = BYPASSRLS なので set_current_tenant 不要)。
async fn insert_active_device(pool: &sqlx::PgPool, tenant_id: Uuid, token: Uuid) {
    sqlx::query(
        "INSERT INTO devices (tenant_id, device_name, device_type, status, settings_token) \
         VALUES ($1, 'test-kiosk', 'kiosk', 'active', $2)",
    )
    .bind(tenant_id)
    .bind(token)
    .execute(pool)
    .await
    .expect("insert test device");
}

#[tokio::test]
async fn jwt_valid_existing_tenant_ok() {
    let state = common::setup_app_state().await;
    let tenant = common::create_test_tenant(state.pool(), "RTOD jwt ok").await;
    let jwt = common::create_test_jwt(tenant, "admin");
    let base = spawn(state.pool().clone()).await;

    let resp = reqwest::Client::new()
        .get(format!("{base}/t"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.text().await.unwrap(), tenant.to_string());
}

#[tokio::test]
async fn jwt_valid_missing_tenant_unauthorized() {
    // JWT は正当だが tenant が DB に存在しない → 401 (揮発性 staging 回復フロー)。
    let state = common::setup_app_state().await;
    let jwt = common::create_test_jwt(Uuid::new_v4(), "admin");
    let base = spawn(state.pool().clone()).await;

    let resp = reqwest::Client::new()
        .get(format!("{base}/t"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn valid_device_token_ok() {
    let state = common::setup_app_state().await;
    let tenant = common::create_test_tenant(state.pool(), "RTOD dev ok").await;
    let token = Uuid::new_v4();
    insert_active_device(state.pool(), tenant, token).await;
    let base = spawn(state.pool().clone()).await;

    let resp = reqwest::Client::new()
        .get(format!("{base}/t"))
        .header("X-Tenant-ID", tenant.to_string())
        .header("X-Device-Token", token.to_string())
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.text().await.unwrap(), tenant.to_string());
}

#[tokio::test]
async fn wrong_device_token_unauthorized() {
    // tenant は実在し device もあるが、提示された token が一致しない → 401。
    let state = common::setup_app_state().await;
    let tenant = common::create_test_tenant(state.pool(), "RTOD dev wrong").await;
    insert_active_device(state.pool(), tenant, Uuid::new_v4()).await;
    let base = spawn(state.pool().clone()).await;

    let resp = reqwest::Client::new()
        .get(format!("{base}/t"))
        .header("X-Tenant-ID", tenant.to_string())
        .header("X-Device-Token", Uuid::new_v4().to_string())
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn db_error_fail_closed_unauthorized() {
    // 検証クエリが失敗 (pool クローズ) しても fail-closed で 401 (Err 分岐)。
    let state = common::setup_app_state().await;
    let broken = state.pool().clone();
    broken.close().await;
    let base = spawn(broken).await;

    let resp = reqwest::Client::new()
        .get(format!("{base}/t"))
        .header("X-Tenant-ID", Uuid::new_v4().to_string())
        .header("X-Device-Token", Uuid::new_v4().to_string())
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}
