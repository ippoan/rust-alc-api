use axum::{
    extract::State,
    http::StatusCode,
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
use crate::db::models::{Tenant, User};
use crate::db::DbPool;
use crate::middleware::auth::AuthUser;

/// 公開ルート (認証不要)
pub fn public_router() -> Router<DbPool> {
    Router::new()
        .route("/auth/google", post(google_login))
        .route("/auth/refresh", post(refresh_token))
        .route("/auth/tenants", post(create_tenant))
}

/// 保護ルート (JWT 必須、require_jwt ミドルウェアの後ろに配置)
pub fn protected_router() -> Router<DbPool> {
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
    State(pool): State<DbPool>,
    Extension(verifier): Extension<GoogleTokenVerifier>,
    Extension(jwt_secret): Extension<JwtSecret>,
    Json(body): Json<GoogleLoginRequest>,
) -> Result<Json<AuthResponse>, StatusCode> {
    // Google ID トークンを検証
    let google_claims = verifier
        .verify(&body.id_token)
        .await
        .map_err(|e| {
            tracing::warn!("Google token verification failed: {e}");
            StatusCode::UNAUTHORIZED
        })?;

    // ユーザーを google_sub で検索
    let existing_user = sqlx::query_as::<_, User>(
        "SELECT * FROM users WHERE google_sub = $1",
    )
    .bind(&google_claims.sub)
    .fetch_optional(&pool)
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
            .fetch_one(&pool)
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
            .fetch_one(&pool)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        }
    };

    // JWT + Refresh token 発行
    let access_token = create_access_token(&user, &jwt_secret)
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
    .execute(&pool)
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
    State(pool): State<DbPool>,
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
    .fetch_optional(&pool)
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
    State(pool): State<DbPool>,
    Extension(auth_user): Extension<AuthUser>,
) -> Result<StatusCode, StatusCode> {
    sqlx::query(
        "UPDATE users SET refresh_token_hash = NULL, refresh_token_expires_at = NULL WHERE id = $1",
    )
    .bind(auth_user.user_id)
    .execute(&pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(StatusCode::NO_CONTENT)
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
    State(pool): State<DbPool>,
    Json(body): Json<CreateTenant>,
) -> Result<(StatusCode, Json<TenantResponse>), StatusCode> {
    let tenant = sqlx::query_as::<_, Tenant>(
        "INSERT INTO tenants (name) VALUES ($1) RETURNING *",
    )
    .bind(&body.name)
    .fetch_one(&pool)
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
