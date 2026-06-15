pub use crate::middleware::{AuthUser, TenantId};

use axum::{extract::Request, http::StatusCode, middleware::Next, response::Response, Extension};
use uuid::Uuid;

use crate::auth_jwt::{verify_access_token, verify_internal_token, JwtSecret};

/// `require_tenant` が tenant の実在確認に使う DB pool を Extension で受け渡すための newtype。
///
/// `AppState.pool` と同じ `Option<PgPool>` を保持する。`None` の場合 (mock テスト等で
/// DB を持たないケース) は実在確認をスキップする (= fail-open)。
/// main / テストハーネスの router 構築時に `Extension(TenantValidationPool(state.pool.clone()))`
/// を layer する。
#[derive(Clone)]
pub struct TenantValidationPool(pub Option<sqlx::PgPool>);

/// resolve 済みの tenant_id が `tenants` テーブルに実在するか確認する。
///
/// 揮発性 staging DB で tenant が消えると、ブラウザの JWT は古い tenant_id を持つが
/// `tenants` に無く、`*_tenant_id_fkey` FK 違反で INSERT が 500 にラップされる。
/// ここで先に 401 を返すことで、フロント (nuxt-trouble) の `onUnauthorized` →
/// `clearAuth()` → `/login` 自動再ログインフローに乗せ、新 tenant 作成で回復させる。
///
/// pool が `None` (DB 無し) の場合は確認できないので `true` を返してスキップする。
async fn tenant_exists(pool: Option<&sqlx::PgPool>, tenant_id: Uuid) -> bool {
    let Some(pool) = pool else {
        return true;
    };
    match sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM tenants WHERE id = $1)")
        .bind(tenant_id)
        .fetch_one(pool)
        .await
    {
        Ok(exists) => exists,
        // DB エラー時は実在確認自体が失敗しているだけなので fail-open。
        // (FK 違反由来の 500 はこの後の handler 側で従来どおり起きる)
        Err(e) => {
            tracing::warn!("tenant existence check query failed: {e}");
            true
        }
    }
}

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
        tenant_slug: claims.org_slug,
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
    validation_pool: Option<Extension<TenantValidationPool>>,
    mut req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let pool = validation_pool
        .as_ref()
        .and_then(|Extension(p)| p.0.as_ref());

    // まず JWT を試行 (フラット化: 閉じ括弧の llvm-cov 問題回避)
    if let Some(Ok(claims)) = extract_bearer_token(&req)
        .zip(jwt_secret.as_ref())
        .map(|(token, Extension(secret))| verify_access_token(token, secret))
    {
        // tenant が DB に実在しないなら 401 を返す (揮発性 staging で tenant が消えた
        // ケース。フロントの自動 logout → 再ログインで回復させる)。
        if !tenant_exists(pool, claims.tenant_id).await {
            // 一行 warn! に収めるため tenant_id を一旦束縛 (複数行 `tracing::warn!` は
            // llvm-cov が format 引数行を別 region 0 カウントし coverage 100% を割る、PR #364)。
            let tid = claims.tenant_id;
            tracing::warn!("tenant {tid} not in tenants table (JWT); returning 401");
            return Err(StatusCode::UNAUTHORIZED);
        }
        let auth_user = AuthUser {
            user_id: claims.sub,
            email: claims.email,
            name: claims.name.clone(),
            tenant_id: claims.tenant_id,
            tenant_slug: claims.org_slug,
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

    if !tenant_exists(pool, tenant_id).await {
        // 一行に収める (上記 JWT 経路と同理由: 複数行 warn! は llvm-cov で uncovered 計上)。
        tracing::warn!("tenant {tenant_id} not in tenants table (X-Tenant-ID); returning 401");
        return Err(StatusCode::UNAUTHORIZED);
    }

    req.extensions_mut().insert(TenantId(tenant_id));
    Ok(next.run(req).await)
}

/// 内部 API 用 JWT 検証ミドルウェア
///
/// `Authorization: Bearer <jwt>` を要求し、`aud == "alc-api-internal"` を強制する。
/// auth-worker が LINE WORKS webhook を受け取って rust-alc-api に転送する際の
/// `/api/internal/*` ルート保護に使う。通常のユーザー JWT (`AppClaims`) は
/// `aud` を持たないため弾かれる。
pub async fn require_internal_jwt(
    Extension(jwt_secret): Extension<JwtSecret>,
    req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let token = extract_bearer_token(&req).ok_or(StatusCode::UNAUTHORIZED)?;
    verify_internal_token(token, &jwt_secret).map_err(|e| {
        tracing::warn!("internal JWT verification failed: {e}");
        StatusCode::UNAUTHORIZED
    })?;
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
/// (`require_tenant` のフォールバックパターンと同じ規約)。X-Tenant-ID 欠落時は 401。
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
        let resp = send(
            app_internal_jwt(),
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
}
