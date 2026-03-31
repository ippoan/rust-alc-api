use axum::{
    extract::{Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Redirect},
    routing::{get, post},
    Extension, Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use alc_core::auth_google::GoogleTokenVerifier;
use alc_core::auth_jwt::{
    self, create_access_token, create_refresh_token, hash_refresh_token, refresh_token_expires_at,
    JwtSecret,
};
use alc_core::auth_lineworks;
use alc_core::auth_middleware::AuthUser;
use alc_core::repository::auth::AuthRepository;
use alc_core::AppState;

/// 公開ルート (認証不要)
pub fn public_router() -> Router<AppState> {
    Router::new()
        .route("/auth/google", post(google_login))
        .route("/auth/google/code", post(google_code_login))
        .route("/auth/refresh", post(refresh_token))
        .route("/auth/tenants", post(create_tenant))
        .route("/auth/lineworks/redirect", get(lineworks_redirect))
        .route("/auth/lineworks/callback", get(lineworks_callback))
        .route("/auth/google/redirect", get(google_redirect))
        .route("/auth/google/callback", get(google_callback))
        .route("/auth/woff-config", get(woff_config))
        .route("/auth/woff", post(woff_auth))
}

/// 保護ルート (JWT 必須、require_jwt ミドルウェアの後ろに配置)
pub fn protected_router() -> Router<AppState> {
    Router::new()
        .route("/auth/me", get(me))
        .route("/auth/logout", post(logout))
        .route("/my-orgs", post(my_orgs))
}

// --- Google ログイン ---

#[derive(Debug, Deserialize)]
pub struct GoogleLoginRequest {
    pub id_token: String,
}

#[derive(Debug, Serialize)]
pub struct AuthResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: i64,
    pub user: UserResponse,
}

#[derive(Debug, Serialize)]
pub struct UserResponse {
    pub id: Uuid,
    pub email: String,
    pub name: String,
    pub tenant_id: Uuid,
    pub role: String,
}

async fn google_login(
    State(state): State<AppState>,
    Extension(verifier): Extension<GoogleTokenVerifier>,
    Extension(jwt_secret): Extension<JwtSecret>,
    Json(body): Json<GoogleLoginRequest>,
) -> Result<Json<AuthResponse>, StatusCode> {
    let google_claims = verifier.verify(&body.id_token).await.map_err(|e| {
        tracing::warn!("Google token verification failed: {e}");
        StatusCode::UNAUTHORIZED
    })?;

    issue_tokens_for_google_claims(&*state.auth, &jwt_secret, google_claims).await
}

// --- Google Authorization Code ログイン ---

#[derive(Debug, Deserialize)]
pub struct GoogleCodeRequest {
    pub code: String,
    pub redirect_uri: String,
}

async fn google_code_login(
    State(state): State<AppState>,
    Extension(verifier): Extension<GoogleTokenVerifier>,
    Extension(jwt_secret): Extension<JwtSecret>,
    Json(body): Json<GoogleCodeRequest>,
) -> Result<Json<AuthResponse>, StatusCode> {
    let google_claims = verifier
        .exchange_code(&body.code, &body.redirect_uri)
        .await
        .map_err(|e| {
            tracing::warn!("Google code exchange failed: {e}");
            StatusCode::UNAUTHORIZED
        })?;

    issue_tokens_for_google_claims(&*state.auth, &jwt_secret, google_claims).await
}

/// Google claims からユーザーを検索/作成し、JWT + Refresh token を発行する共通ロジック
async fn issue_tokens_for_google_claims(
    repo: &dyn AuthRepository,
    jwt_secret: &JwtSecret,
    google_claims: alc_core::auth_google::GoogleClaims,
) -> Result<Json<AuthResponse>, StatusCode> {
    // ユーザーを google_sub で検索
    let existing_user = repo
        .find_user_by_google_sub(&google_claims.sub)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let user = match existing_user {
        Some(user) => user,
        None => {
            // 初回ログイン: 招待 → ドメイン → 新テナント の順で検索
            let email_domain = google_claims
                .email
                .split('@')
                .nth(1)
                .unwrap_or("default")
                .to_string();

            // 1. tenant_allowed_emails でメール完全一致検索
            let invitation = repo
                .find_invitation_by_email(&google_claims.email)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

            let (tenant_id, role) = if let Some(inv) = &invitation {
                (inv.tenant_id, inv.role.clone())
            } else {
                // 2. tenants.email_domain でドメイン一致検索
                let domain_tenant = repo
                    .find_tenant_by_email_domain(&email_domain)
                    .await
                    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

                if let Some(t) = domain_tenant {
                    (t.id, "admin".to_string())
                } else {
                    // 3. 新テナント作成 (従来の動作)
                    let new_tenant = repo
                        .create_tenant_with_domain(&email_domain)
                        .await
                        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
                    (new_tenant.id, "admin".to_string())
                }
            };

            let user = repo
                .create_user_google(
                    tenant_id,
                    &google_claims.sub,
                    &google_claims.email,
                    &google_claims.name,
                    &role,
                )
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

            // 招待レコードを消費 (使い済み)
            if let Some(inv) = &invitation {
                let _ = repo.delete_invitation(inv.id).await;
            }

            user
        }
    };

    // JWT + Refresh token 発行
    let slug = repo
        .get_tenant_slug(user.tenant_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let access_token = create_access_token(&user, jwt_secret, slug)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let (raw_refresh, refresh_hash) = create_refresh_token();
    let expires_at = refresh_token_expires_at();

    // Refresh token をDBに保存
    repo.save_refresh_token(user.id, &refresh_hash, expires_at)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(AuthResponse {
        access_token,
        refresh_token: raw_refresh,
        expires_in: auth_jwt::ACCESS_TOKEN_EXPIRY_SECS,
        user: UserResponse {
            id: user.id,
            email: user.email,
            name: user.name,
            tenant_id: user.tenant_id,
            role: user.role,
        },
    }))
}

// --- Refresh token ---

#[derive(Debug, Deserialize)]
pub struct RefreshRequest {
    pub refresh_token: String,
}

#[derive(Debug, Serialize)]
pub struct RefreshResponse {
    pub access_token: String,
    pub expires_in: i64,
}

async fn refresh_token(
    State(state): State<AppState>,
    Extension(jwt_secret): Extension<JwtSecret>,
    Json(body): Json<RefreshRequest>,
) -> Result<Json<RefreshResponse>, StatusCode> {
    let token_hash = hash_refresh_token(&body.refresh_token);

    // ハッシュが一致し、期限内のユーザーを検索
    let user = state
        .auth
        .find_user_by_refresh_token_hash(&token_hash)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let slug = state
        .auth
        .get_tenant_slug(user.tenant_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let access_token = create_access_token(&user, &jwt_secret, slug)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(RefreshResponse {
        access_token,
        expires_in: auth_jwt::ACCESS_TOKEN_EXPIRY_SECS,
    }))
}

// --- Me ---

async fn me(Extension(auth_user): Extension<AuthUser>) -> Json<UserResponse> {
    Json(UserResponse {
        id: auth_user.user_id,
        email: auth_user.email,
        name: auth_user.name,
        tenant_id: auth_user.tenant_id,
        role: auth_user.role,
    })
}

// --- Logout ---

async fn logout(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthUser>,
) -> Result<StatusCode, StatusCode> {
    state
        .auth
        .clear_refresh_token(auth_user.user_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(StatusCode::NO_CONTENT)
}

// --- My Organizations ---

#[derive(Debug, Serialize)]
struct MyOrgsResponse {
    organizations: Vec<OrgItem>,
}

#[derive(Debug, Serialize)]
struct OrgItem {
    id: Uuid,
    name: String,
    slug: String,
    role: String,
}

/// ユーザーが所属するテナント一覧を返す
async fn my_orgs(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthUser>,
) -> Result<Json<MyOrgsResponse>, StatusCode> {
    let tenant = state
        .auth
        .get_tenant_by_id(auth_user.tenant_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let orgs = match tenant {
        Some(t) => vec![OrgItem {
            id: t.id,
            name: t.name,
            slug: t.slug.unwrap_or_default(),
            role: auth_user.role,
        }],
        None => vec![],
    };

    Ok(Json(MyOrgsResponse {
        organizations: orgs,
    }))
}

// --- Google OAuth Redirect Flow ---

#[derive(Debug, Deserialize)]
struct GoogleRedirectParams {
    redirect_uri: String,
}

/// Google OAuth 開始: HMAC-signed state 生成 → Google authorize URL にリダイレクト
async fn google_redirect(
    Query(params): Query<GoogleRedirectParams>,
    Extension(verifier): Extension<GoogleTokenVerifier>,
) -> Result<impl IntoResponse, StatusCode> {
    let oauth_state_secret =
        std::env::var("OAUTH_STATE_SECRET").map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let state_payload = auth_lineworks::state::StatePayload {
        redirect_uri: params.redirect_uri,
        nonce: Uuid::new_v4().to_string(),
        provider: "google".to_string(),
        external_org_id: String::new(),
    };
    let signed_state = auth_lineworks::state::sign(&state_payload, &oauth_state_secret);

    let api_origin =
        std::env::var("API_ORIGIN").unwrap_or_else(|_| "https://alc-api.ippoan.org".to_string());
    let callback_uri = format!("{api_origin}/api/auth/google/callback");

    let google_auth_url = format!(
        "https://accounts.google.com/o/oauth2/v2/auth?\
         client_id={}\
         &redirect_uri={}\
         &response_type=code\
         &scope=openid%20email%20profile\
         &state={}\
         &access_type=online\
         &prompt=select_account",
        urlencoding::encode(verifier.client_id()),
        urlencoding::encode(&callback_uri),
        urlencoding::encode(&signed_state),
    );

    Ok(Redirect::temporary(&google_auth_url))
}

#[derive(Debug, Deserialize)]
struct GoogleCallbackParams {
    code: String,
    state: String,
}

/// Google OAuth コールバック: code → id_token → JWT 発行 → リダイレクト
async fn google_callback(
    State(state): State<AppState>,
    Extension(verifier): Extension<GoogleTokenVerifier>,
    Extension(jwt_secret): Extension<JwtSecret>,
    Query(params): Query<GoogleCallbackParams>,
) -> Result<impl IntoResponse, StatusCode> {
    let oauth_state_secret =
        std::env::var("OAUTH_STATE_SECRET").map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let state_payload =
        auth_lineworks::state::verify(&params.state, &oauth_state_secret).map_err(|e| {
            tracing::warn!("Google state verification failed: {e}");
            StatusCode::BAD_REQUEST
        })?;

    let api_origin =
        std::env::var("API_ORIGIN").unwrap_or_else(|_| "https://alc-api.ippoan.org".to_string());
    let callback_uri = format!("{api_origin}/api/auth/google/callback");

    let google_claims = verifier
        .exchange_code(&params.code, &callback_uri)
        .await
        .map_err(|e| {
            tracing::error!("Google code exchange failed: {e:?}");
            StatusCode::BAD_GATEWAY
        })?;

    let auth_response =
        issue_tokens_for_google_claims(&*state.auth, &jwt_secret, google_claims).await?;

    let redirect_url = format!(
        "{}#token={}&refresh_token={}&expires_in={}&lw_callback=1",
        state_payload.redirect_uri,
        urlencoding::encode(&auth_response.access_token),
        urlencoding::encode(&auth_response.refresh_token),
        auth_response.expires_in,
    );

    let parent_domain = extract_parent_domain(&state_payload.redirect_uri);
    let cookie = format!(
        "logi_auth_token={}; Domain=.{}; Path=/; Max-Age=86400; Secure; SameSite=Lax",
        auth_response.access_token, parent_domain
    );

    Ok((
        StatusCode::TEMPORARY_REDIRECT,
        [
            (header::LOCATION, redirect_url),
            (header::SET_COOKIE, cookie),
        ],
    ))
}

// --- LINE WORKS OAuth ---

#[derive(Debug, Deserialize)]
struct LineworksRedirectParams {
    domain: Option<String>,
    address: Option<String>,
    redirect_uri: String,
}

/// LINE WORKS OAuth 開始: SSO config を DB から取得 → LINE WORKS authorize URL にリダイレクト
async fn lineworks_redirect(
    State(state): State<AppState>,
    Query(params): Query<LineworksRedirectParams>,
) -> Result<impl IntoResponse, StatusCode> {
    let oauth_state_secret = std::env::var("OAUTH_STATE_SECRET").map_err(|_| {
        tracing::error!("OAUTH_STATE_SECRET not set");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // address パラメータから domain を抽出（user@domain → domain）
    let domain = params
        .domain
        .or_else(|| {
            params
                .address
                .as_ref()
                .map(|a| a.split('@').next_back().unwrap_or(a).to_string())
        })
        .ok_or_else(|| {
            tracing::warn!("Missing domain or address parameter");
            StatusCode::BAD_REQUEST
        })?;

    // DB から SSO config を検索（SECURITY DEFINER 関数でRLSバイパス）
    let config = state
        .auth
        .resolve_sso_config("lineworks", &domain)
        .await
        .map_err(|e| {
            tracing::error!("SSO config query failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or_else(|| {
            tracing::warn!("No SSO config found for domain: {}", domain);
            StatusCode::NOT_FOUND
        })?;

    // HMAC-signed state 生成
    let state_payload = auth_lineworks::state::StatePayload {
        redirect_uri: params.redirect_uri,
        nonce: Uuid::new_v4().to_string(),
        provider: "lineworks".to_string(),
        external_org_id: config.external_org_id.clone(),
    };
    let signed_state = auth_lineworks::state::sign(&state_payload, &oauth_state_secret);

    // callback URL
    let api_origin =
        std::env::var("API_ORIGIN").unwrap_or_else(|_| "https://alc-api.mtamaramu.com".to_string());
    let callback_uri = format!("{api_origin}/api/auth/lineworks/callback");
    let encoded_callback = urlencoding::encode(&callback_uri);

    let authorize_url = auth_lineworks::authorize_url(
        &config.client_id,
        &encoded_callback,
        &urlencoding::encode(&signed_state),
    );

    Ok(Redirect::temporary(&authorize_url))
}

#[derive(Debug, Deserialize)]
struct LineworksCallbackParams {
    code: String,
    state: String,
}

/// LINE WORKS OAuth コールバック: code → token → user info → JWT 発行 → リダイレクト
async fn lineworks_callback(
    State(state): State<AppState>,
    Extension(jwt_secret): Extension<JwtSecret>,
    Query(params): Query<LineworksCallbackParams>,
) -> Result<impl IntoResponse, StatusCode> {
    let oauth_state_secret =
        std::env::var("OAUTH_STATE_SECRET").map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // State 検証
    let state_payload =
        auth_lineworks::state::verify(&params.state, &oauth_state_secret).map_err(|e| {
            tracing::warn!("State verification failed: {e}");
            StatusCode::BAD_REQUEST
        })?;

    // SSO config を DB から取得（SECURITY DEFINER 関数）
    let config = state
        .auth
        .resolve_sso_config_required("lineworks", &state_payload.external_org_id)
        .await
        .map_err(|e| {
            tracing::error!("SSO config lookup failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    // callback URL 再構築（token exchange で必要）
    let api_origin =
        std::env::var("API_ORIGIN").unwrap_or_else(|_| "https://alc-api.mtamaramu.com".to_string());
    let callback_uri = format!("{api_origin}/api/auth/lineworks/callback");

    // client_secret を復号（AES-256-GCM, SSO_ENCRYPTION_KEY で暗号化）
    let encryption_key =
        std::env::var("SSO_ENCRYPTION_KEY").unwrap_or_else(|_| jwt_secret.0.clone());
    let client_secret =
        auth_lineworks::decrypt_secret(&config.client_secret_encrypted, &encryption_key).map_err(
            |e| {
                tracing::error!("client_secret decryption failed: {e}");
                StatusCode::INTERNAL_SERVER_ERROR
            },
        )?;

    // Code → Token 交換
    let http_client = reqwest::Client::new();
    let token_resp = auth_lineworks::exchange_code(
        &http_client,
        &config.client_id,
        &client_secret,
        &params.code,
        &callback_uri,
    )
    .await
    .map_err(|e| {
        tracing::error!("LINE WORKS token exchange failed: {e}");
        StatusCode::BAD_GATEWAY
    })?;

    // User profile 取得
    let profile = auth_lineworks::fetch_user_profile(&http_client, &token_resp.access_token)
        .await
        .map_err(|e| {
            tracing::error!("LINE WORKS user profile failed: {e}");
            StatusCode::BAD_GATEWAY
        })?;

    let user = upsert_lineworks_user(&*state.auth, config.tenant_id, &profile).await?;

    let slug = state
        .auth
        .get_tenant_slug(config.tenant_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // JWT + Refresh token 発行・保存
    let access_token = create_access_token(&user, &jwt_secret, slug)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let (raw_refresh, refresh_hash) = create_refresh_token();
    let expires_at = refresh_token_expires_at();

    state
        .auth
        .save_refresh_token(user.id, &refresh_hash, expires_at)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // リダイレクト（JWT を fragment で渡す + cross-subdomain cookie 設定）
    let redirect_url = format!(
        "{}#token={}&refresh_token={}&expires_in={}&lw_callback=1",
        state_payload.redirect_uri,
        urlencoding::encode(&access_token),
        urlencoding::encode(&raw_refresh),
        auth_jwt::ACCESS_TOKEN_EXPIRY_SECS,
    );

    let parent_domain = extract_parent_domain(&state_payload.redirect_uri);
    let cookie = format!(
        "logi_auth_token={}; Domain=.{}; Path=/; Max-Age=86400; Secure; SameSite=Lax",
        access_token, parent_domain
    );

    Ok((
        StatusCode::TEMPORARY_REDIRECT,
        [
            (header::LOCATION, redirect_url),
            (header::SET_COOKIE, cookie),
        ],
    ))
}

// --- WOFF SDK ---

#[derive(Debug, Deserialize)]
struct WoffConfigParams {
    domain: String,
}

#[derive(Debug, Serialize)]
struct WoffConfigResponse {
    #[serde(rename = "woffId")]
    woff_id: String,
}

/// WOFF SDK 設定取得: domain → woffId
async fn woff_config(
    State(state): State<AppState>,
    Query(params): Query<WoffConfigParams>,
) -> Result<Json<WoffConfigResponse>, StatusCode> {
    let config = state
        .auth
        .resolve_sso_config("lineworks", &params.domain)
        .await
        .map_err(|e| {
            tracing::error!("SSO config query failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    let woff_id = config.woff_id.ok_or_else(|| {
        tracing::warn!("WOFF not configured for domain: {}", params.domain);
        StatusCode::NOT_FOUND
    })?;

    Ok(Json(WoffConfigResponse { woff_id }))
}

#[derive(Debug, Deserialize)]
struct WoffAuthRequest {
    access_token: String,
    domain_id: String,
}

#[derive(Debug, Serialize)]
struct WoffAuthResponse {
    token: String,
    #[serde(rename = "expiresAt")]
    expires_at: String,
    #[serde(rename = "tenantId")]
    tenant_id: Uuid,
}

/// WOFF SDK 認証: access_token で直接ユーザー認証 → JWT 発行
async fn woff_auth(
    State(state): State<AppState>,
    Extension(jwt_secret): Extension<JwtSecret>,
    Json(body): Json<WoffAuthRequest>,
) -> Result<Json<WoffAuthResponse>, StatusCode> {
    let config = state
        .auth
        .resolve_sso_config("lineworks", &body.domain_id)
        .await
        .map_err(|e| {
            tracing::error!("SSO config query failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    // WOFF SDK は access_token を直接提供するので code exchange 不要
    let http_client = reqwest::Client::new();
    let profile = auth_lineworks::fetch_user_profile(&http_client, &body.access_token)
        .await
        .map_err(|e| {
            tracing::warn!("WOFF user profile fetch failed: {e}");
            StatusCode::UNAUTHORIZED
        })?;

    let user = upsert_lineworks_user(&*state.auth, config.tenant_id, &profile).await?;

    let slug = state
        .auth
        .get_tenant_slug(config.tenant_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let access_token = create_access_token(&user, &jwt_secret, slug)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let (_raw_refresh, refresh_hash) = create_refresh_token();
    let expires_at = refresh_token_expires_at();

    state
        .auth
        .save_refresh_token(user.id, &refresh_hash, expires_at)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let token_expires_at = (chrono::Utc::now()
        + chrono::Duration::seconds(auth_jwt::ACCESS_TOKEN_EXPIRY_SECS))
    .to_rfc3339();

    Ok(Json(WoffAuthResponse {
        token: access_token,
        expires_at: token_expires_at,
        tenant_id: user.tenant_id,
    }))
}

// --- LINE WORKS ユーザー共通 ---

/// LINE WORKS ユーザー upsert（lineworks_id で検索、なければ作成）
async fn upsert_lineworks_user(
    repo: &dyn AuthRepository,
    tenant_id: Uuid,
    profile: &auth_lineworks::UserProfile,
) -> Result<alc_core::models::User, StatusCode> {
    let lineworks_id = &profile.user_id;
    let email = profile.email_or_id();
    let name = profile.display_name();

    let existing = repo
        .find_user_by_lineworks_id(lineworks_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    match existing {
        Some(u) => Ok(u),
        None => repo
            .create_user_lineworks(tenant_id, lineworks_id, &email, &name)
            .await
            .map_err(|e| {
                tracing::error!("User creation failed: {e}");
                StatusCode::INTERNAL_SERVER_ERROR
            }),
    }
}

/// redirect_uri からパレントドメインを抽出
/// 例: "https://items.mtamaramu.com/foo" → "mtamaramu.com"
fn extract_parent_domain(url_str: &str) -> String {
    // "https://items.mtamaramu.com/foo" → "items.mtamaramu.com"
    let host = url_str
        .strip_prefix("https://")
        .or_else(|| url_str.strip_prefix("http://"))
        .unwrap_or(url_str)
        .split('/')
        .next()
        .unwrap_or("mtamaramu.com")
        .split(':')
        .next()
        .unwrap_or("mtamaramu.com");

    let parts: Vec<&str> = host.split('.').collect();
    if parts.len() > 2 {
        parts[parts.len() - 2..].join(".")
    } else {
        host.to_string()
    }
}

// --- テナント作成 (後方互換) ---

#[derive(Debug, Deserialize)]
pub struct CreateTenant {
    pub name: String,
}

#[derive(Debug, Serialize)]
pub struct TenantResponse {
    pub id: Uuid,
    pub name: String,
}

async fn create_tenant(
    State(state): State<AppState>,
    Json(body): Json<CreateTenant>,
) -> Result<(StatusCode, Json<TenantResponse>), StatusCode> {
    let tenant = state
        .auth
        .create_tenant_by_name(&body.name)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok((
        StatusCode::CREATED,
        Json(TenantResponse {
            id: tenant.id,
            name: tenant.name,
        }),
    ))
}
