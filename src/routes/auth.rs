use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Redirect},
    routing::{get, post},
    Extension, Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::google::GoogleTokenVerifier;
use crate::auth::jwt::{
    self, create_access_token, create_refresh_token, hash_refresh_token, refresh_token_expires_at,
    JwtSecret,
};
use crate::auth::lineworks;
use crate::db::models::{Tenant, User};
use crate::AppState;
use crate::middleware::auth::AuthUser;

/// 公開ルート (認証不要)
pub fn public_router() -> Router<AppState> {
    Router::new()
        .route("/auth/google", post(google_login))
        .route("/auth/google/code", post(google_code_login))
        .route("/auth/refresh", post(refresh_token))
        .route("/auth/tenants", post(create_tenant))
        .route("/auth/lineworks/redirect", get(lineworks_redirect))
        .route("/auth/lineworks/callback", get(lineworks_callback))
}

/// 保護ルート (JWT 必須、require_jwt ミドルウェアの後ろに配置)
pub fn protected_router() -> Router<AppState> {
    Router::new()
        .route("/auth/me", get(me))
        .route("/auth/logout", post(logout))
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
    let google_claims = verifier
        .verify(&body.id_token)
        .await
        .map_err(|e| {
            tracing::warn!("Google token verification failed: {e}");
            StatusCode::UNAUTHORIZED
        })?;

    issue_tokens_for_google_claims(&state, &jwt_secret, google_claims).await
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

    issue_tokens_for_google_claims(&state, &jwt_secret, google_claims).await
}

/// Google claims からユーザーを検索/作成し、JWT + Refresh token を発行する共通ロジック
async fn issue_tokens_for_google_claims(
    state: &AppState,
    jwt_secret: &JwtSecret,
    google_claims: crate::auth::google::GoogleClaims,
) -> Result<Json<AuthResponse>, StatusCode> {
    // ユーザーを google_sub で検索
    let existing_user = sqlx::query_as::<_, User>(
        "SELECT * FROM users WHERE google_sub = $1",
    )
    .bind(&google_claims.sub)
    .fetch_optional(&state.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let user = match existing_user {
        Some(user) => user,
        None => {
            // 初回ログイン: テナント自動作成 + ユーザー作成
            let tenant_name = google_claims
                .email
                .split('@')
                .nth(1)
                .unwrap_or("default")
                .to_string();

            let tenant = sqlx::query_as::<_, Tenant>(
                "INSERT INTO tenants (name) VALUES ($1) RETURNING *",
            )
            .bind(&tenant_name)
            .fetch_one(&state.pool)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

            sqlx::query_as::<_, User>(
                r#"
                INSERT INTO users (tenant_id, google_sub, email, name, role)
                VALUES ($1, $2, $3, $4, 'admin')
                RETURNING *
                "#,
            )
            .bind(tenant.id)
            .bind(&google_claims.sub)
            .bind(&google_claims.email)
            .bind(&google_claims.name)
            .fetch_one(&state.pool)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        }
    };

    // JWT + Refresh token 発行
    let access_token = create_access_token(&user, jwt_secret)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let (raw_refresh, refresh_hash) = create_refresh_token();
    let expires_at = refresh_token_expires_at();

    // Refresh token をDBに保存
    sqlx::query(
        "UPDATE users SET refresh_token_hash = $1, refresh_token_expires_at = $2 WHERE id = $3",
    )
    .bind(&refresh_hash)
    .bind(expires_at)
    .bind(user.id)
    .execute(&state.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(AuthResponse {
        access_token,
        refresh_token: raw_refresh,
        expires_in: jwt::ACCESS_TOKEN_EXPIRY_SECS,
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
    let user = sqlx::query_as::<_, User>(
        r#"
        SELECT * FROM users
        WHERE refresh_token_hash = $1
          AND refresh_token_expires_at > NOW()
        "#,
    )
    .bind(&token_hash)
    .fetch_optional(&state.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::UNAUTHORIZED)?;

    let access_token = create_access_token(&user, &jwt_secret)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(RefreshResponse {
        access_token,
        expires_in: jwt::ACCESS_TOKEN_EXPIRY_SECS,
    }))
}

// --- Me ---

async fn me(
    Extension(auth_user): Extension<AuthUser>,
) -> Json<UserResponse> {
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
    sqlx::query(
        "UPDATE users SET refresh_token_hash = NULL, refresh_token_expires_at = NULL WHERE id = $1",
    )
    .bind(auth_user.user_id)
    .execute(&state.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(StatusCode::NO_CONTENT)
}

// --- LINE WORKS OAuth ---

#[derive(Debug, Deserialize)]
struct LineworksRedirectParams {
    domain: String,
    redirect_uri: String,
}

/// LINE WORKS OAuth 開始: SSO config を DB から取得 → LINE WORKS authorize URL にリダイレクト
async fn lineworks_redirect(
    State(state): State<AppState>,
    Query(params): Query<LineworksRedirectParams>,
) -> Result<impl IntoResponse, StatusCode> {
    let oauth_state_secret = std::env::var("OAUTH_STATE_SECRET")
        .map_err(|_| {
            tracing::error!("OAUTH_STATE_SECRET not set");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    // DB から SSO config を検索（SECURITY DEFINER 関数でRLSバイパス）
    let config = sqlx::query_as::<_, SsoConfigRow>(
        "SELECT * FROM resolve_sso_config('lineworks', $1)",
    )
    .bind(&params.domain)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("SSO config query failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?
    .ok_or_else(|| {
        tracing::warn!("No SSO config found for domain: {}", params.domain);
        StatusCode::NOT_FOUND
    })?;

    // HMAC-signed state 生成
    let state_payload = lineworks::state::StatePayload {
        redirect_uri: params.redirect_uri,
        nonce: Uuid::new_v4().to_string(),
        provider: "lineworks".to_string(),
        external_org_id: config.external_org_id.clone(),
    };
    let signed_state = lineworks::state::sign(&state_payload, &oauth_state_secret);

    // callback URL
    let api_origin = std::env::var("API_ORIGIN")
        .unwrap_or_else(|_| "https://alc-api.mtamaramu.com".to_string());
    let callback_uri = format!("{api_origin}/api/auth/lineworks/callback");
    let encoded_callback = urlencoding::encode(&callback_uri);

    let authorize_url = lineworks::authorize_url(
        &config.client_id,
        &encoded_callback,
        &urlencoding::encode(&signed_state),
    );

    Ok(Redirect::temporary(&authorize_url))
}

#[derive(Debug, sqlx::FromRow)]
struct SsoConfigRow {
    tenant_id: Uuid,
    client_id: String,
    client_secret_encrypted: String,
    external_org_id: String,
    woff_id: Option<String>,
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
    let oauth_state_secret = std::env::var("OAUTH_STATE_SECRET")
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // State 検証
    let state_payload = lineworks::state::verify(&params.state, &oauth_state_secret)
        .map_err(|e| {
            tracing::warn!("State verification failed: {e}");
            StatusCode::BAD_REQUEST
        })?;

    // SSO config を DB から取得（SECURITY DEFINER 関数）
    let config = sqlx::query_as::<_, SsoConfigRow>(
        "SELECT * FROM resolve_sso_config('lineworks', $1)",
    )
    .bind(&state_payload.external_org_id)
    .fetch_one(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("SSO config lookup failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // callback URL 再構築（token exchange で必要）
    let api_origin = std::env::var("API_ORIGIN")
        .unwrap_or_else(|_| "https://alc-api.mtamaramu.com".to_string());
    let callback_uri = format!("{api_origin}/api/auth/lineworks/callback");

    // Code → Token 交換
    let http_client = reqwest::Client::new();
    let token_resp = lineworks::exchange_code(
        &http_client,
        &config.client_id,
        &config.client_secret_encrypted,
        &params.code,
        &callback_uri,
    )
    .await
    .map_err(|e| {
        tracing::error!("LINE WORKS token exchange failed: {e}");
        StatusCode::BAD_GATEWAY
    })?;

    // User profile 取得
    let profile = lineworks::fetch_user_profile(&http_client, &token_resp.access_token)
        .await
        .map_err(|e| {
            tracing::error!("LINE WORKS user profile failed: {e}");
            StatusCode::BAD_GATEWAY
        })?;

    let lineworks_id = profile.user_id.clone();
    let email = profile.email_or_id();
    let name = profile.display_name();

    // User upsert (lineworks_id で検索、なければ作成)
    let user = sqlx::query_as::<_, User>(
        "SELECT * FROM users WHERE lineworks_id = $1",
    )
    .bind(&lineworks_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let user = match user {
        Some(u) => u,
        None => {
            sqlx::query_as::<_, User>(
                r#"
                INSERT INTO users (tenant_id, lineworks_id, email, name, role)
                VALUES ($1, $2, $3, $4, 'admin')
                RETURNING *
                "#,
            )
            .bind(config.tenant_id)
            .bind(&lineworks_id)
            .bind(&email)
            .bind(&name)
            .fetch_one(&state.pool)
            .await
            .map_err(|e| {
                tracing::error!("User creation failed: {e}");
                StatusCode::INTERNAL_SERVER_ERROR
            })?
        }
    };

    // JWT 発行
    let access_token = create_access_token(&user, &jwt_secret)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let (raw_refresh, refresh_hash) = create_refresh_token();
    let expires_at = refresh_token_expires_at();

    // Refresh token 保存
    sqlx::query(
        "UPDATE users SET refresh_token_hash = $1, refresh_token_expires_at = $2 WHERE id = $3",
    )
    .bind(&refresh_hash)
    .bind(expires_at)
    .bind(user.id)
    .execute(&state.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // リダイレクト（JWT を fragment で渡す）
    let redirect_url = format!(
        "{}#token={}&refresh_token={}&expires_in={}&lw_callback=1",
        state_payload.redirect_uri,
        urlencoding::encode(&access_token),
        urlencoding::encode(&raw_refresh),
        jwt::ACCESS_TOKEN_EXPIRY_SECS,
    );

    Ok(Redirect::temporary(&redirect_url))
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
    let tenant = sqlx::query_as::<_, Tenant>(
        "INSERT INTO tenants (name) VALUES ($1) RETURNING *",
    )
    .bind(&body.name)
    .fetch_one(&state.pool)
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
