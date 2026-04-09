pub use crate::middleware::{AuthUser, TenantId};

use axum::{extract::Request, http::StatusCode, middleware::Next, response::Response, Extension};
use uuid::Uuid;

use crate::auth_jwt::{verify_access_token, JwtSecret};

/// JWT 必須ミドルウェア — 管理ページ用
///
/// Authorization: Bearer <jwt> ヘッダーから JWT を検証し、
/// AuthUser と TenantId を Extension に挿入する。
pub async fn require_jwt(
    Extension(jwt_secret): Extension<JwtSecret>,
    mut req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let token = extract_bearer_token(&req).ok_or(StatusCode::UNAUTHORIZED)?;

    let claims = verify_access_token(token, &jwt_secret).map_err(|e| {
        tracing::warn!("JWT verification failed: {e}");
        StatusCode::UNAUTHORIZED
    })?;

    let auth_user = AuthUser {
        user_id: claims.sub,
        email: claims.email,
        name: claims.name.clone(),
        tenant_id: claims.tenant_id,
        role: claims.role,
    };

    req.extensions_mut().insert(TenantId(claims.tenant_id));
    req.extensions_mut().insert(auth_user);
    Ok(next.run(req).await)
}

/// テナント認証ミドルウェア — キオスクモード対応
///
/// 1. Authorization: Bearer <jwt> があれば JWT を検証 (管理者モード)
/// 2. なければ X-Tenant-ID ヘッダーにフォールバック (キオスクモード)
pub async fn require_tenant(
    jwt_secret: Option<Extension<JwtSecret>>,
    mut req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    // まず JWT を試行 (フラット化: 閉じ括弧の llvm-cov 問題回避)
    if let Some(Ok(claims)) = extract_bearer_token(&req)
        .zip(jwt_secret.as_ref())
        .map(|(token, Extension(secret))| verify_access_token(token, secret))
    {
        let auth_user = AuthUser {
            user_id: claims.sub,
            email: claims.email,
            name: claims.name.clone(),
            tenant_id: claims.tenant_id,
            role: claims.role,
        };
        req.extensions_mut().insert(TenantId(claims.tenant_id));
        req.extensions_mut().insert(auth_user);
        return Ok(next.run(req).await);
    }

    // フォールバック: X-Tenant-ID ヘッダー (キオスクモード)
    let tenant_id = req
        .headers()
        .get("X-Tenant-ID")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| Uuid::parse_str(v).ok())
        .ok_or(StatusCode::UNAUTHORIZED)?;

    req.extensions_mut().insert(TenantId(tenant_id));
    Ok(next.run(req).await)
}

/// X-Tenant-ID ヘッダーのみで認証するミドルウェア (gateway 配下の内部サービス用)
///
/// Gateway が JWT を検証済みで X-Tenant-ID ヘッダーを注入している前提。
/// AuthUser も X-User-ID / X-User-Email / X-User-Role ヘッダーから復元する。
pub async fn require_tenant_header(mut req: Request, next: Next) -> Result<Response, StatusCode> {
    let tenant_id = req
        .headers()
        .get("X-Tenant-ID")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| Uuid::parse_str(v).ok())
        .ok_or(StatusCode::UNAUTHORIZED)?;

    req.extensions_mut().insert(TenantId(tenant_id));

    // Gateway が注入した認証ヘッダーから AuthUser を復元
    let user_id = req
        .headers()
        .get("X-User-ID")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| Uuid::parse_str(v).ok());
    let email = req
        .headers()
        .get("X-User-Email")
        .and_then(|v| v.to_str().ok())
        .map(String::from);
    let role = req
        .headers()
        .get("X-User-Role")
        .and_then(|v| v.to_str().ok())
        .map(String::from);

    if let (Some(user_id), Some(email), Some(role)) = (user_id, email, role) {
        let auth_user = AuthUser {
            user_id,
            email,
            name: String::new(),
            tenant_id,
            role,
        };
        req.extensions_mut().insert(auth_user);
    }

    Ok(next.run(req).await)
}

/// Authorization ヘッダーから Bearer トークンを抽出
fn extract_bearer_token(req: &Request) -> Option<&str> {
    req.headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, middleware as axum_middleware, routing::get, Router};

    async fn echo_tenant(Extension(tid): Extension<TenantId>) -> String {
        tid.0.to_string()
    }

    async fn echo_auth_user(Extension(user): Extension<AuthUser>) -> String {
        format!("{}:{}", user.email, user.role)
    }

    fn app_tenant_header() -> Router {
        Router::new()
            .route("/t", get(echo_tenant))
            .route("/u", get(echo_auth_user))
            .layer(axum_middleware::from_fn(require_tenant_header))
    }

    async fn send(app: Router, r: Request<Body>) -> Response {
        use tower::ServiceExt;
        app.into_service().oneshot(r).await.unwrap()
    }

    fn req(uri: &str) -> Request<Body> {
        Request::builder().uri(uri).body(Body::empty()).unwrap()
    }

    fn req_with_headers(uri: &str, headers: &[(&str, &str)]) -> Request<Body> {
        let mut b = Request::builder().uri(uri);
        for (k, v) in headers {
            b = b.header(*k, *v);
        }
        b.body(Body::empty()).unwrap()
    }

    #[tokio::test]
    async fn tenant_header_ok() {
        let tid = Uuid::new_v4();
        let resp = send(
            app_tenant_header(),
            req_with_headers("/t", &[("X-Tenant-ID", &tid.to_string())]),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
        assert_eq!(String::from_utf8_lossy(&body), tid.to_string());
    }

    #[tokio::test]
    async fn tenant_header_missing() {
        let resp = send(app_tenant_header(), req("/t")).await;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn tenant_header_invalid_uuid() {
        let resp = send(
            app_tenant_header(),
            req_with_headers("/t", &[("X-Tenant-ID", "not-a-uuid")]),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn tenant_header_with_auth_user() {
        let tid = Uuid::new_v4();
        let uid = Uuid::new_v4();
        let resp = send(
            app_tenant_header(),
            req_with_headers(
                "/u",
                &[
                    ("X-Tenant-ID", &tid.to_string()),
                    ("X-User-ID", &uid.to_string()),
                    ("X-User-Email", "test@example.com"),
                    ("X-User-Role", "admin"),
                ],
            ),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
        assert_eq!(String::from_utf8_lossy(&body), "test@example.com:admin");
    }

    #[tokio::test]
    async fn tenant_header_partial_auth_headers() {
        let tid = Uuid::new_v4();
        let resp = send(
            app_tenant_header(),
            req_with_headers(
                "/t",
                &[
                    ("X-Tenant-ID", &tid.to_string()),
                    ("X-User-ID", &Uuid::new_v4().to_string()),
                ],
            ),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::OK);
    }
}
