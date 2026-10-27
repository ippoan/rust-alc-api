pub use crate::middleware::{AuthUser, TenantId};

use axum::{extract::Request, http::StatusCode, middleware::Next, response::Response, Extension};

use crate::auth_google::GoogleTokenVerifier;
use crate::device_dev::{self, DeviceDevSlot};

/// 注入された identity ヘッダーを信頼するミドルウェア。検証の実体は
/// `alc_core_wasm::require_tenant_header` (monolith と worker で 1 か所)。
///
/// monolith ではその外側に dev端末の印のスコープを張る (Refs ippoan/alc-app#387):
/// 入れ物を request extensions に入れ、実体が**認証を通した後に** `X-Device-Dev` を
/// 書き、以降の `set_current_tenant` がそれを DB の接続へ通す。認証に落ちた要求は
/// 入れ物が dev でないまま終わる。
pub async fn require_tenant_header(mut req: Request, next: Next) -> Result<Response, StatusCode> {
    let slot = DeviceDevSlot::default();
    req.extensions_mut().insert(slot.clone());
    device_dev::scope(slot, alc_core_wasm::require_tenant_header(req, next)).await
}

/// internal-auth の OIDC 検証設定。`require_internal_jwt` 配下に Extension で注入する。
/// `verifier` は `client_id=alc-api-internal` で構築した `GoogleTokenVerifier`。
/// app 配線 (`routes::router` の引数経由、prod は main.rs / テストは
/// `with_test_claims`) で注入するため、テストは決定的。
///
/// #479 で #434 Phase D の HS256 dual-accept (`enabled` flag + 共有 `JWT_SECRET`
/// 検証) を撤去し、Google OIDC (aud=alc-api-internal、JWKS RS256 署名検証) に
/// 一本化した。全呼び出し元 (auth-worker `internalAuthToken` / Cloud Scheduler)
/// は OIDC mint 済み。
#[derive(Clone)]
pub struct InternalOidcTrust {
    pub verifier: GoogleTokenVerifier,
}

/// 内部 API 用 OIDC 検証ミドルウェア
///
/// `Authorization: Bearer <token>` を要求し、Google OIDC (aud=alc-api-internal) を
/// **JWKS で RS256 署名検証**して受理する (`GoogleTokenVerifier::verify_internal_oidc`)。
/// auth-worker が internal 呼び出し (`/api/internal/*`) に使う。通常のユーザー JWT
/// (HS256 / aud 無し) は署名検証で弾かれる。
///
/// 旧 HS256 internal JWT (`verify_internal_token` + 共有 `JWT_SECRET`) の受理は
/// #479 で撤去した — 全呼び出し元が OIDC を mint する (auth-worker#340)。
pub async fn require_internal_jwt(
    Extension(oidc_trust): Extension<InternalOidcTrust>,
    req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let token = extract_bearer_token(&req).ok_or(StatusCode::UNAUTHORIZED)?;
    if oidc_trust
        .verifier
        .verify_internal_oidc(token)
        .await
        .is_ok()
    {
        return Ok(next.run(req).await);
    }
    tracing::warn!("internal OIDC verification failed");
    Err(StatusCode::UNAUTHORIZED)
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
    use uuid::Uuid;

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
        Router::new()
            .route("/i", get(echo_ok))
            .layer(axum_middleware::from_fn(require_internal_jwt))
            .layer(Extension(InternalOidcTrust {
                verifier: test_oidc_verifier(),
            }))
    }

    /// test_claims モードの verifier (`verify_internal_oidc("test-valid-token")` が Ok)。
    fn test_oidc_verifier() -> GoogleTokenVerifier {
        use crate::auth_google::GoogleClaims;
        GoogleTokenVerifier::with_test_claims(
            crate::auth_jwt::INTERNAL_AUD.to_string(),
            GoogleClaims {
                sub: "sa-123".to_string(),
                email: String::new(),
                name: String::new(),
                picture: None,
                email_verified: false,
                aud: crate::auth_jwt::INTERNAL_AUD.to_string(),
                iss: "https://accounts.google.com".to_string(),
                exp: (chrono::Utc::now().timestamp() + 3600) as u64,
            },
        )
    }

    async fn echo_ok() -> &'static str {
        "ok"
    }

    #[tokio::test]
    async fn internal_jwt_oidc_ok() {
        // OIDC verifier (test_claims モード) の署名検証 OK トークンは受理。
        let resp = send(
            app_internal_jwt(),
            req_with_headers("/i", &[("Authorization", "Bearer test-valid-token")]),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::OK);
    }

    /// 旧 HS256 JWT を模した token をテスト内で直接組み立てる (#479 PR-3 で
    /// rust 側の HS256 発行関数 `create_internal_token` / `create_access_token`
    /// は撤去済みのため、jsonwebtoken を直接使って craft する)。
    fn craft_hs256_jwt(claims: &serde_json::Value, secret: &str) -> String {
        use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
        encode(
            &Header::new(Algorithm::HS256),
            claims,
            &EncodingKey::from_secret(secret.as_bytes()),
        )
        .unwrap()
    }

    #[tokio::test]
    async fn internal_jwt_hs256_no_longer_accepted() {
        // #479 regression: 旧 HS256 internal JWT (共有 JWT_SECRET 署名、
        // aud=alc-api-internal で整形されたもの) は dual-accept 撤去後は
        // 拒否されること。
        let now = chrono::Utc::now().timestamp();
        let token = craft_hs256_jwt(
            &serde_json::json!({
                "iss": "auth-worker",
                "aud": crate::auth_jwt::INTERNAL_AUD,
                "iat": now,
                "exp": now + 60,
            }),
            "test-internal-secret-256-bits!!!",
        );
        let resp = send(
            app_internal_jwt(),
            req_with_headers("/i", &[("Authorization", &format!("Bearer {token}"))]),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn internal_jwt_missing_header() {
        let resp = send(app_internal_jwt(), req("/i")).await;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn internal_jwt_user_token_rejected() {
        // ユーザー JWT 相当 (HS256 / aud 無し、auth-worker が発行する access JWT
        // と同形の claims) は OIDC 署名検証で拒否されること
        let now = chrono::Utc::now().timestamp();
        let token = craft_hs256_jwt(
            &serde_json::json!({
                "sub": Uuid::new_v4(),
                "email": "u@e.com",
                "name": "u",
                "tenant_id": Uuid::new_v4(),
                "role": "admin",
                "iat": now,
                "exp": now + 3600,
            }),
            "test-internal-secret-256-bits!!!",
        );
        let resp = send(
            app_internal_jwt(),
            req_with_headers("/i", &[("Authorization", &format!("Bearer {token}"))]),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn internal_jwt_oidc_rejected_when_signature_invalid() {
        // 署名検証に失敗する token は拒否 (署名検証は無効化していない)。
        let resp = send(
            app_internal_jwt(),
            req_with_headers("/i", &[("Authorization", "Bearer not-a-valid-token")]),
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
    // dev端末の印 (X-Device-Dev) — 認証を通した要求のスコープにだけ立つ
    // -----------------------------------------------------------------

    async fn echo_dev() -> &'static str {
        if device_dev::is_device_dev() {
            "dev"
        } else {
            "prod"
        }
    }

    async fn body_of(resp: Response) -> String {
        let body = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
        String::from_utf8_lossy(&body).into_owned()
    }

    fn app_dev() -> Router {
        let tenant = Router::new()
            .route("/t", get(echo_dev))
            .layer(axum_middleware::from_fn(require_tenant_header));
        Router::new().route("/public", get(echo_dev)).merge(tenant)
    }

    #[tokio::test]
    async fn device_dev_tenant_header_route() {
        let tid = Uuid::new_v4().to_string();
        for (value, expected) in [
            (Some("1"), "dev"),
            (Some("true"), "prod"),
            (Some("0"), "prod"),
            (Some(""), "prod"),
            (None, "prod"),
        ] {
            let mut headers = vec![("X-Tenant-ID", tid.as_str())];
            if let Some(v) = value {
                headers.push(("X-Device-Dev", v));
            }
            let resp = send(app_dev(), req_with_headers("/t", &headers)).await;
            assert_eq!(resp.status(), StatusCode::OK);
            assert_eq!(body_of(resp).await, expected, "{value:?}");
        }
    }

    #[tokio::test]
    async fn device_dev_ignored_on_public_route() {
        let resp = send(
            app_dev(),
            req_with_headers("/public", &[("X-Device-Dev", "1")]),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(body_of(resp).await, "prod");
    }

    #[tokio::test]
    async fn device_dev_does_not_bypass_auth() {
        let resp = send(app_dev(), req_with_headers("/t", &[("X-Device-Dev", "1")])).await;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }
}
