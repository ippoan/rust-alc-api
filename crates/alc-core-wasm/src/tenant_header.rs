use axum::{extract::Request, http::StatusCode, middleware::Next, response::Response};
use uuid::Uuid;

use crate::types::{AuthUser, TenantId};

/// 注入された identity ヘッダーを信頼するミドルウェア (Refs #434)
///
/// **前段の trusted proxy (CF Worker = alc-app / carins / nuxt-items、または
/// per-domain API gateway) が auth-worker `/auth/introspect` で user/device JWT を
/// 検証し、検証済み identity を `X-Tenant-ID` / `X-User-ID` / `X-User-Email` /
/// `X-User-Role` ヘッダーとして注入している前提**。rust-alc-api 自身は JWT 検証を
/// 行わず、注入された identity を信頼するだけの dumb backend に徹する。
///
/// #434 で monolith の `require_jwt` / `require_tenant` (= ローカル JWT 検証 +
/// bare X-Tenant-ID フォールバック) を撤去し、tenant/admin 経路をこのミドルウェアに
/// 一本化した。外部からの直叩き防止は **Cloud Run IAM による網層ロックダウン**
/// (proxy の OIDC ID token のみ到達可) が担う (= 確定アーキ #4807535677、step 3)。
///
/// - `X-Tenant-ID` 欠落 → 401
/// - `X-User-ID` / `X-User-Email` / `X-User-Role` が揃えば AuthUser も復元する
///   (admin 経路の role 判定はハンドラ側が AuthUser から行う)
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

    let tenant_slug = req
        .headers()
        .get("X-Tenant-Slug")
        .and_then(|v| v.to_str().ok())
        .map(String::from);

    if let (Some(user_id), Some(email), Some(role)) = (user_id, email, role) {
        let auth_user = AuthUser {
            user_id,
            email,
            name: String::new(),
            tenant_id,
            tenant_slug,
            role,
        };
        req.extensions_mut().insert(auth_user);
    }

    Ok(next.run(req).await)
}
