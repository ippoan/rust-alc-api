pub use crate::middleware::{AuthUser, TenantId};

use axum::{extract::Request, http::StatusCode, middleware::Next, response::Response, Extension};
use uuid::Uuid;

use crate::auth_google::GoogleTokenVerifier;

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
