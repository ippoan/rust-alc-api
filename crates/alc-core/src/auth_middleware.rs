pub use crate::middleware::{AuthUser, TenantId};

use axum::{extract::Request, http::StatusCode, middleware::Next, response::Response, Extension};
use uuid::Uuid;

use crate::auth_jwt::{verify_internal_oidc_aud, verify_internal_token, JwtSecret};

/// lockdown (`allUsers` 削除) 後に internal-auth の OIDC 経路を有効化するフラグ
/// (Refs #434 Phase D)。`require_internal_jwt` 配下に Extension で注入する。
/// `true` の時だけ Google OIDC (aud=alc-api-internal) を受理 (それまでは HS256 のみ = 非破壊)。
/// app 配線側で `INTERNAL_AUTH_TRUST_OIDC` env から解決する (Extension 注入なのでテストは決定的)。
#[derive(Clone, Copy, Debug)]
pub struct InternalOidcTrust(pub bool);

/// 内部 API 用 JWT 検証ミドルウェア
///
/// `Authorization: Bearer <jwt>` を要求し、`aud == "alc-api-internal"` を強制する。
/// auth-worker が LINE WORKS webhook を受け取って rust-alc-api に転送する際の
/// `/api/internal/*` ルート保護に使う。通常のユーザー JWT (`AppClaims`) は
/// `aud` を持たないため弾かれる。
///
/// #434 Phase D の lockdown 移行用に **dual-accept**:
/// 1. 従来の HS256 internal JWT (`verify_internal_token`) — 移行前/現行。
/// 2. `INTERNAL_AUTH_TRUST_OIDC=1` の時のみ、Google OIDC (aud=alc-api-internal) を受理
///    (`verify_internal_oidc_aud`)。署名は Cloud Run IAM が検証済みの前提で aud のみ確認。
///    flag off の間は完全に dormant (非破壊)。
pub async fn require_internal_jwt(
    Extension(jwt_secret): Extension<JwtSecret>,
    Extension(oidc_trust): Extension<InternalOidcTrust>,
    req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let token = extract_bearer_token(&req).ok_or(StatusCode::UNAUTHORIZED)?;
    // 1) HS256 internal JWT (移行前/現行)。
    if verify_internal_token(token, &jwt_secret).is_ok() {
        return Ok(next.run(req).await);
    }
    // 2) lockdown 後の OIDC 経路 (flag gated)。
    if oidc_trust.0 && verify_internal_oidc_aud(token).is_ok() {
        return Ok(next.run(req).await);
    }
    tracing::warn!("internal JWT verification failed");
    Err(StatusCode::UNAUTHORIZED)
}

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

/// `INTERNAL_SHARED_SECRET` env で配布される shared secret を `X-Internal-Shared-Secret`
/// ヘッダーで検証する middleware。timing-safe 比較。
///
/// 認証ヘッダーが揃えば追加で `X-Tenant-ID` から `TenantId` extension を挿入する
/// (`require_tenant_header` と同じ規約)。X-Tenant-ID 欠落時は 401。
///
/// 使用箇所: email-receiver Worker → `POST /api/dtako/tickets` 等の internal ingest
/// 経路。本 middleware を `from_fn_with_state` ではなく `Extension(InternalSharedSecret)`
/// 経由で読むことで、binding ごとに secret を差し替え可能 (テスト時 mock しやすい)。
///
/// 注意: TLS 終端を越えてくる shared secret なので、constant-time 比較で timing
/// attack を防ぐ。長さ違いも constant-time で扱う。
#[derive(Clone)]
pub struct InternalSharedSecret(pub String);

pub async fn require_internal_shared_secret(
    Extension(InternalSharedSecret(expected)): Extension<InternalSharedSecret>,
    mut req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let provided = req
        .headers()
        .get("X-Internal-Shared-Secret")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if !constant_time_eq(expected.as_bytes(), provided.as_bytes()) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let tenant_id = req
        .headers()
        .get("X-Tenant-ID")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| Uuid::parse_str(v).ok())
        .ok_or(StatusCode::UNAUTHORIZED)?;

    req.extensions_mut().insert(TenantId(tenant_id));
    Ok(next.run(req).await)
}

/// timing-safe な byte 列等値比較。長さが異なれば短い方を 0 と比較し続けて
/// 早期 return しない (= short-circuit を避ける)。
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    let max = a.len().max(b.len());
    let mut diff: u8 = (a.len() ^ b.len()) as u8;
    for i in 0..max {
        let ai = *a.get(i).unwrap_or(&0);
        let bi = *b.get(i).unwrap_or(&0);
        diff |= ai ^ bi;
    }
    diff == 0
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

    fn app_internal_jwt() -> Router {
        app_internal_jwt_with(false)
    }

    fn app_internal_jwt_with(oidc: bool) -> Router {
        Router::new()
            .route("/i", get(echo_ok))
            .layer(axum_middleware::from_fn(require_internal_jwt))
            .layer(Extension(InternalOidcTrust(oidc)))
            .layer(Extension(JwtSecret(
                "test-internal-secret-256-bits!!!".to_string(),
            )))
    }

    async fn echo_ok() -> &'static str {
        "ok"
    }

    #[tokio::test]
    async fn internal_jwt_ok() {
        use crate::auth_jwt::create_internal_token;
        let secret = JwtSecret("test-internal-secret-256-bits!!!".to_string());
        let token = create_internal_token(&secret, "auth-worker", 60).unwrap();
        let resp = send(
            app_internal_jwt(),
            req_with_headers("/i", &[("Authorization", &format!("Bearer {token}"))]),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn internal_jwt_missing_header() {
        let resp = send(app_internal_jwt(), req("/i")).await;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn internal_jwt_user_token_rejected() {
        // ユーザー JWT は aud を持たないので拒否されること
        use crate::auth_jwt::create_access_token;
        use crate::models::User;
        let secret = JwtSecret("test-internal-secret-256-bits!!!".to_string());
        let user = User {
            id: Uuid::new_v4(),
            tenant_id: Uuid::new_v4(),
            google_sub: Some("g".to_string()),
            lineworks_id: None,
            line_user_id: None,
            email: "u@e.com".to_string(),
            name: "u".to_string(),
            role: "admin".to_string(),
            username: None,
            password_hash: None,
            refresh_token_hash: None,
            refresh_token_expires_at: None,
            created_at: chrono::Utc::now(),
        };
        let token = create_access_token(&user, &secret, None).unwrap();
        let resp = send(
            app_internal_jwt(),
            req_with_headers("/i", &[("Authorization", &format!("Bearer {token}"))]),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn internal_jwt_wrong_secret_rejected() {
        use crate::auth_jwt::create_internal_token;
        let other = JwtSecret("different-secret-key-256-bits!!".to_string());
        let token = create_internal_token(&other, "auth-worker", 60).unwrap();
        // OIDC trust off (既定) なので、HS256 不一致 = 401 (dual-accept の OIDC 経路に入らない)。
        let resp = send(
            app_internal_jwt(),
            req_with_headers("/i", &[("Authorization", &format!("Bearer {token}"))]),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn internal_jwt_oidc_aud_accepted_when_trust_enabled() {
        // OIDC trust on + aud=alc-api-internal の OIDC token (別 secret 署名 = HS256 不一致) は
        // 受理される (Cloud Run IAM が署名検証済みの前提、Refs #434 Phase D)。
        use crate::auth_jwt::create_internal_token;
        let other = JwtSecret("different-secret-key-256-bits!!".to_string());
        let token = create_internal_token(&other, "auth-worker", 60).unwrap(); // aud=alc-api-internal
        let resp = send(
            app_internal_jwt_with(true),
            req_with_headers("/i", &[("Authorization", &format!("Bearer {token}"))]),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn internal_jwt_oidc_wrong_aud_rejected_when_trust_enabled() {
        // OIDC trust on でも aud が alc-api-internal でなければ拒否 (confused-deputy 防止)。
        use crate::auth_jwt::create_access_token;
        use crate::models::User;
        let secret = JwtSecret("test-internal-secret-256-bits!!!".to_string());
        let user = User {
            id: Uuid::new_v4(),
            tenant_id: Uuid::new_v4(),
            google_sub: Some("g".to_string()),
            lineworks_id: None,
            line_user_id: None,
            email: "u@e.com".to_string(),
            name: "u".to_string(),
            role: "admin".to_string(),
            username: None,
            password_hash: None,
            refresh_token_hash: None,
            refresh_token_expires_at: None,
            created_at: chrono::Utc::now(),
        };
        // ユーザー JWT は aud を持たない → HS256 でも OIDC でも弾かれる。
        let token = create_access_token(&user, &secret, None).unwrap();
        let resp = send(
            app_internal_jwt_with(true),
            req_with_headers("/i", &[("Authorization", &format!("Bearer {token}"))]),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
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

    // -----------------------------------------------------------------
    // require_internal_shared_secret + constant_time_eq
    // -----------------------------------------------------------------

    fn app_internal_secret(secret: &str) -> Router {
        Router::new()
            .route("/i", get(echo_tenant))
            .layer(axum_middleware::from_fn(require_internal_shared_secret))
            .layer(Extension(InternalSharedSecret(secret.to_string())))
    }

    #[tokio::test]
    async fn internal_secret_ok_inserts_tenant_id() {
        let tid = Uuid::new_v4();
        let resp = send(
            app_internal_secret("secret-value"),
            req_with_headers(
                "/i",
                &[
                    ("X-Internal-Shared-Secret", "secret-value"),
                    ("X-Tenant-ID", &tid.to_string()),
                ],
            ),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
        assert_eq!(String::from_utf8_lossy(&body), tid.to_string());
    }

    #[tokio::test]
    async fn internal_secret_missing_header_unauthorized() {
        // 「ヘッダー無し」と「secret 値が空文字列」の両方で 401。
        let resp = send(
            app_internal_secret("secret-value"),
            req_with_headers("/i", &[("X-Tenant-ID", &Uuid::new_v4().to_string())]),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn internal_secret_mismatch_unauthorized() {
        let resp = send(
            app_internal_secret("secret-value"),
            req_with_headers(
                "/i",
                &[
                    ("X-Internal-Shared-Secret", "wrong-secret"),
                    ("X-Tenant-ID", &Uuid::new_v4().to_string()),
                ],
            ),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn internal_secret_ok_but_tenant_missing_unauthorized() {
        let resp = send(
            app_internal_secret("secret-value"),
            req_with_headers("/i", &[("X-Internal-Shared-Secret", "secret-value")]),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn internal_secret_ok_but_tenant_invalid_uuid_unauthorized() {
        let resp = send(
            app_internal_secret("secret-value"),
            req_with_headers(
                "/i",
                &[
                    ("X-Internal-Shared-Secret", "secret-value"),
                    ("X-Tenant-ID", "not-a-uuid"),
                ],
            ),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn constant_time_eq_matches_equal_strings() {
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(constant_time_eq(b"", b""));
    }

    #[test]
    fn constant_time_eq_rejects_different_strings() {
        assert!(!constant_time_eq(b"abc", b"abd"));
        // 長さ違い (両方向)。
        assert!(!constant_time_eq(b"abc", b"abcd"));
        assert!(!constant_time_eq(b"abcd", b"abc"));
        // 長さ違い + 空。
        assert!(!constant_time_eq(b"", b"x"));
        assert!(!constant_time_eq(b"x", b""));
    }
}
