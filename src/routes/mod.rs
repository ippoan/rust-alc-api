pub mod auth;
pub mod employees;
pub mod equipment_failures;
pub mod health_baselines;
pub mod measurements;
pub mod tenko_records;
pub mod tenko_schedules;
pub mod tenko_sessions;
pub mod tenko_webhooks;
pub mod upload;

use axum::{middleware as axum_middleware, Router};

use crate::AppState;
use crate::middleware::auth::{require_jwt, require_tenant};

pub fn router() -> Router<AppState> {
    // JWT 必須ルート (管理者のみ)
    let jwt_protected = Router::new()
        .merge(auth::protected_router())
        .merge(employees::jwt_router())
        .merge(upload::router())
        .merge(measurements::jwt_router())
        .merge(tenko_webhooks::jwt_router())
        .layer(axum_middleware::from_fn(require_jwt));

    // テナント対応ルート (JWT or X-Tenant-ID)
    let tenant_protected = Router::new()
        .merge(measurements::router())
        .merge(employees::tenant_router())
        .merge(tenko_schedules::tenant_router())
        .merge(tenko_sessions::tenant_router())
        .merge(tenko_records::tenant_router())
        .merge(health_baselines::tenant_router())
        .merge(equipment_failures::tenant_router())
        .layer(axum_middleware::from_fn(require_tenant));

    // 公開ルート (認証不要)
    let public_routes = auth::public_router();

    Router::new()
        .merge(public_routes)
        .merge(jwt_protected)
        .merge(tenant_protected)
}
