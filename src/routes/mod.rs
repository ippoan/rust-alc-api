pub mod auth;
pub mod employees;
pub mod measurements;
pub mod upload;

use axum::{middleware as axum_middleware, Router};

use crate::db::DbPool;
use crate::middleware::auth::{require_jwt, require_tenant};

pub fn router() -> Router<DbPool> {
    // JWT 必須ルート (管理者のみ)
    let jwt_protected = Router::new()
        .merge(auth::protected_router())
        .merge(employees::router())
        .merge(upload::router())
        .layer(axum_middleware::from_fn(require_jwt));

    // キオスク対応ルート (JWT or X-Tenant-ID)
    let tenant_protected = Router::new()
        .merge(measurements::router())
        .layer(axum_middleware::from_fn(require_tenant));

    // 公開ルート (認証不要)
    let public_routes = auth::public_router();

    Router::new()
        .merge(public_routes)
        .merge(jwt_protected)
        .merge(tenant_protected)
}
