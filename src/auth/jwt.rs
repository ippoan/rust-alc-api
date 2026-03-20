use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::db::models::User;

/// Access token の有効期限 (秒)
pub const ACCESS_TOKEN_EXPIRY_SECS: i64 = 3600; // 1時間
/// Refresh token の有効期限 (日)
pub const REFRESH_TOKEN_EXPIRY_DAYS: i64 = 30;

/// App JWT のクレーム
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AppClaims {
    pub sub: Uuid,
    pub email: String,
    pub name: String,
    pub tenant_id: Uuid,
    pub role: String,
    pub iat: i64,
    pub exp: i64,
}

/// JWT シークレットのラッパー
#[derive(Clone)]
pub struct JwtSecret(pub String);

/// Access token を発行
pub fn create_access_token(user: &User, secret: &JwtSecret) -> Result<String, jsonwebtoken::errors::Error> {
    let now = Utc::now();
    let claims = AppClaims {
        sub: user.id,
        email: user.email.clone(),
        name: user.name.clone(),
        tenant_id: user.tenant_id,
        role: user.role.clone(),
        iat: now.timestamp(),
        exp: (now + Duration::seconds(ACCESS_TOKEN_EXPIRY_SECS)).timestamp(),
    };

    encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(secret.0.as_bytes()),
    )
}

/// Access token を検証してクレームを返す
pub fn verify_access_token(token: &str, secret: &JwtSecret) -> Result<AppClaims, jsonwebtoken::errors::Error> {
    let mut validation = Validation::new(Algorithm::HS256);
    validation.validate_exp = true;

    let token_data = decode::<AppClaims>(
        token,
        &DecodingKey::from_secret(secret.0.as_bytes()),
        &validation,
    )?;

    Ok(token_data.claims)
}

/// Refresh token を生成し、(raw_token, hash) を返す
pub fn create_refresh_token() -> (String, String) {
    let raw = format!("rt_{}", Uuid::new_v4().simple());
    let hash = hash_refresh_token(&raw);
    (raw, hash)
}

/// Refresh token の有効期限を返す
pub fn refresh_token_expires_at() -> chrono::DateTime<Utc> {
    Utc::now() + Duration::days(REFRESH_TOKEN_EXPIRY_DAYS)
}

/// Refresh token を SHA-256 でハッシュ化
pub fn hash_refresh_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_user() -> User {
        User {
            id: Uuid::new_v4(),
            tenant_id: Uuid::new_v4(),
            google_sub: Some("google-sub-123".to_string()),
            lineworks_id: None,
            email: "test@example.com".to_string(),
            name: "Test User".to_string(),
            role: "admin".to_string(),
            refresh_token_hash: None,
            refresh_token_expires_at: None,
            created_at: Utc::now(),
        }
    }

    #[test]
    fn test_create_and_verify_access_token() {
        let user = test_user();
        let secret = JwtSecret("test-secret-key-256-bits-long!!!".to_string());

        let token = create_access_token(&user, &secret).unwrap();
        let claims = verify_access_token(&token, &secret).unwrap();

        assert_eq!(claims.sub, user.id);
        assert_eq!(claims.email, user.email);
        assert_eq!(claims.tenant_id, user.tenant_id);
        assert_eq!(claims.role, "admin");
    }

    #[test]
    fn test_verify_with_wrong_secret_fails() {
        let user = test_user();
        let secret = JwtSecret("correct-secret-key-256-bits!!!".to_string());
        let wrong_secret = JwtSecret("wrong-secret-key-256-bits!!!!!".to_string());

        let token = create_access_token(&user, &secret).unwrap();
        assert!(verify_access_token(&token, &wrong_secret).is_err());
    }

    #[test]
    fn test_refresh_token_generation() {
        let (raw, hash) = create_refresh_token();
        assert!(raw.starts_with("rt_"));
        assert_eq!(hash, hash_refresh_token(&raw));
    }

    #[test]
    fn test_refresh_token_hash_consistency() {
        let token = "rt_test123";
        let hash1 = hash_refresh_token(token);
        let hash2 = hash_refresh_token(token);
        assert_eq!(hash1, hash2);
    }
}
