use std::sync::Arc;

use axum::http::header::{
    HeaderValue, REFERRER_POLICY, STRICT_TRANSPORT_SECURITY, X_CONTENT_TYPE_OPTIONS,
    X_FRAME_OPTIONS,
};
use axum::http::Method;
use axum::Router;
use sqlx::postgres::PgPoolOptions;
use tower_http::cors::{AllowOrigin, Any, CorsLayer};
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;

use rust_alc_api::db::repository::{PgAuthRepository, PgBotAdminRepository};
use rust_alc_api::storage::StorageBackend;
use rust_alc_api::AppState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // JSON formatter: Cloud Run Logging が severity (info/warn/error) を解釈でき、
    // jsonPayload.<field> で構造化ログ (redact_pipeline_done の llm_ms 等) を
    // クエリできるようにする (Refs #333)。text formatter だと severity が常に
    // DEFAULT 扱いになり `severity>=ERROR` が 0 件になる。
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    dotenvy::dotenv().ok();

    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let port: u16 = std::env::var("PORT")
        .unwrap_or_else(|_| "8080".into())
        .parse()
        .expect("PORT must be a number");

    // NOTE: 旧ログイン用 Google verifier (GOOGLE_CLIENT_ID / GOOGLE_DEVICE_CLIENT_ID) は
    // 撤去 — ログインは auth-worker に完全移管済み (#479 PR-3) で、この verifier を
    // Extension で取り出す handler は存在しなかった (dead layer)。rust 側に残る
    // Google JWKS 検証は internal OIDC (aud=alc-api-internal、routes::router 内で構築)
    // のみ。

    let pool = PgPoolOptions::new()
        .max_connections(10)
        // RLS テナント GUC (`app.current_tenant_id`) は `set_current_tenant` が
        // session-scope (`set_config(..., false)`) で立てるため、コネクションを
        // プールへ返してもクリアされない。次に同じコネクションを borrow した
        // 別リクエスト (特に `self.pool` 直クエリ) へ tenant コンテキストが
        // リークするのを防ぐため、返却時に必ず RESET して未設定 (= RLS で
        // 行ゼロの fail-closed) に戻す。Refs #386。
        // dev端末の印 (`app.device_dev`) も同じ session-scope なので一緒に消す
        // (Refs ippoan/alc-app#387)。
        .after_release(|conn, _meta| {
            Box::pin(async move {
                alc_core::tenant::reset_tenant_context(conn).await?;
                Ok(true)
            })
        })
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

    let auth = Arc::new(PgAuthRepository::new(pool.clone()));
    let bot_admin = Arc::new(PgBotAdminRepository::new(pool.clone()));

    let state = AppState {
        pool: Some(pool.clone()),
        auth,
        bot_admin,
        storage,
    };

    // CORS: 任意オリジン全開放 (allow_origin(Any)) をやめ、ippoan 系フロントの
    // オリジンだけ許可する。Origin は `scheme://host[:port]` 形式 (path/userinfo
    // なし) なので host 抽出は単純分割で安全。許可 suffix は env
    // `CORS_ALLOWED_ORIGIN_SUFFIXES` (カンマ区切り) で上書き可。method は標準 verb に
    // 限定。header はカスタムヘッダ (X-Tenant-ID 等) が多いため Any を維持 (credentials
    // 不使用なので origin 制限が実質の境界)。Refs #389。
    let cors_suffixes: Vec<String> = std::env::var("CORS_ALLOWED_ORIGIN_SUFFIXES")
        .unwrap_or_else(|_| {
            "ippoan.org,mtamaramu.com,m-tama-ramu.workers.dev,localhost,127.0.0.1".to_string()
        })
        .split(',')
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| !s.is_empty())
        .collect();
    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::predicate(move |origin, _req| {
            origin
                .to_str()
                .ok()
                .and_then(|o| o.split("://").nth(1))
                .map(|hostport| {
                    let host = hostport
                        .split('/')
                        .next()
                        .unwrap_or("")
                        .split(':')
                        .next()
                        .unwrap_or("")
                        .to_ascii_lowercase();
                    cors_suffixes
                        .iter()
                        .any(|s| host == *s || host.ends_with(&format!(".{s}")))
                })
                .unwrap_or(false)
        }))
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::DELETE,
            Method::PATCH,
            Method::OPTIONS,
        ])
        .allow_headers(Any);

    let api_router = rust_alc_api::routes::router(rust_alc_api::routes::internal_oidc_trust());

    let app = Router::new()
        .nest("/api", api_router)
        .layer(axum::extract::DefaultBodyLimit::max(20 * 1024 * 1024)) // 20MB
        .layer(cors)
        // セキュリティレスポンスヘッダ (Refs #394 L-2)。JSON API のため XSS 面は
        // 小さいが、HSTS / nosniff / clickjacking 抑止は安価に効く。ハンドラが
        // 個別に設定した値は尊重する (if_not_present)。
        .layer(SetResponseHeaderLayer::if_not_present(
            STRICT_TRANSPORT_SECURITY,
            HeaderValue::from_static("max-age=31536000; includeSubDomains"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            X_FRAME_OPTIONS,
            HeaderValue::from_static("DENY"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            REFERRER_POLICY,
            HeaderValue::from_static("no-referrer"),
        ))
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port)).await?;
    tracing::info!("listening on 0.0.0.0:{port}");
    axum::serve(listener, app).await?;

    Ok(())
}
