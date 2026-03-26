use axum::{extract::Request, http::StatusCode, middleware::Next, response::Response, Extension};
use uuid::Uuid;

use crate::auth::jwt::{verify_access_token, JwtSecret};

/// 認証済みユーザー情報 (JWT から抽出)
#[derive(Debug, Clone)]
pub struct AuthUser {
    pub user_id: Uuid,
    pub email: String,
    pub name: String,
    pub tenant_id: Uuid,
    pub role: String,
}

/// テナント ID (後方互換)
#[derive(Debug, Clone, Copy)]
pub struct TenantId(pub Uuid);

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
    // まず JWT を試行
    if let Some(token) = extract_bearer_token(&req) {
        if let Some(Extension(ref secret)) = jwt_secret {
            if let Ok(claims) = verify_access_token(token, secret) {
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
        }
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

/// Authorization ヘッダーから Bearer トークンを抽出
fn extract_bearer_token(req: &Request) -> Option<&str> {
    req.headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
}
