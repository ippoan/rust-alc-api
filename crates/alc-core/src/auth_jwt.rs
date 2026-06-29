//! auth_jwt のロジックは leaf crate `alc-auth-jwt` に切り出した (Refs #410)。
//!
//! 本 module は後方互換のための薄い shim:
//! - `pub use alc_auth_jwt::*;` で `JwtSecret` / `AppClaims` / `InternalClaims` /
//!   `current_env_label` / `verify_access_token` / `create_internal_token` /
//!   refresh token hash 関数群などを再 export する (= 既存 `alc_core::auth_jwt::...`
//!   import が無変更で通る)。
//! - `create_access_token` だけは leaf crate 側が `&AccessTokenInput` を取るため、
//!   `&User` (alc-core models) を受ける従来シグネチャの wrapper をここに残し、
//!   呼び出し側 (`crates/alc-auth` 等) の変更を不要にする。

pub use alc_auth_jwt::{
    create_internal_token, create_refresh_token, current_env_label, hash_refresh_token,
    refresh_token_expires_at, verify_access_token, verify_internal_oidc_aud, verify_internal_token,
    AccessTokenInput, AppClaims, InternalClaims, JwtSecret, ACCESS_TOKEN_EXPIRY_SECS, INTERNAL_AUD,
    REFRESH_TOKEN_EXPIRY_DAYS,
};

use crate::models::User;

/// Access token を発行 (`&User` を取る後方互換 wrapper)。
///
/// leaf crate `alc-auth-jwt` は `alc-core::models::User` に依存しないため、
/// `create_access_token` は `&AccessTokenInput` を取る。ここで `&User` →
/// `AccessTokenInput` 変換を噛ませて従来の呼び出し側 (`crates/alc-auth`) を
/// 無変更に保つ。
pub fn create_access_token(
    user: &User,
    secret: &JwtSecret,
    org_slug: Option<String>,
) -> Result<String, jsonwebtoken::errors::Error> {
    let input = AccessTokenInput {
        sub: user.id,
        email: user.email.clone(),
        name: user.name.clone(),
        tenant_id: user.tenant_id,
        role: user.role.clone(),
    };
    alc_auth_jwt::create_access_token(&input, secret, org_slug)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use uuid::Uuid;

    fn test_user() -> User {
        User {
            id: Uuid::new_v4(),
            tenant_id: Uuid::new_v4(),
            google_sub: Some("google-sub-123".to_string()),
            lineworks_id: None,
            line_user_id: None,
            email: "test@example.com".to_string(),
            name: "Test User".to_string(),
            role: "admin".to_string(),
            username: None,
            password_hash: None,
            refresh_token_hash: None,
            refresh_token_expires_at: None,
            created_at: Utc::now(),
        }
    }

    #[test]
    fn test_create_access_token_from_user_wrapper() {
        // `&User` wrapper が leaf crate の create_access_token に委譲し、
        // User のフィールドが claims に正しく写ることを確認する。
        let user = test_user();
        let secret = JwtSecret("wrapper-test-secret-256-bits!!!!".to_string());
        let token = create_access_token(&user, &secret, Some("slug".to_string())).unwrap();
        let claims = verify_access_token(&token, &secret).unwrap();
        assert_eq!(claims.sub, user.id);
        assert_eq!(claims.email, user.email);
        assert_eq!(claims.name, user.name);
        assert_eq!(claims.tenant_id, user.tenant_id);
        assert_eq!(claims.role, user.role);
        assert_eq!(claims.org_slug.as_deref(), Some("slug"));
    }
}
