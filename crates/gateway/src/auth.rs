use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// JWT クレーム (alc-core::auth_jwt::AppClaims と同一)
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AppClaims {
    pub sub: Uuid,
    pub email: String,
    pub name: String,
    pub tenant_id: Uuid,
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub org_slug: Option<String>,
    pub iat: i64,
    pub exp: i64,
}

/// JWT を検証してクレームを返す
pub fn verify_jwt(token: &str, secret: &str) -> Result<AppClaims, jsonwebtoken::errors::Error> {
    let mut validation = Validation::new(Algorithm::HS256);
    validation.validate_exp = true;

    let token_data = decode::<AppClaims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    )?;

    Ok(token_data.claims)
}

/// Authorization ヘッダーから Bearer トークンを抽出
pub fn extract_bearer_token(header_value: &str) -> Option<&str> {
    header_value.strip_prefix("Bearer ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};
    use jsonwebtoken::{encode, EncodingKey, Header};

    fn create_test_token(secret: &str, expired: bool) -> String {
        let now = Utc::now();
        let exp = if expired {
            (now - Duration::hours(1)).timestamp()
        } else {
            (now + Duration::hours(1)).timestamp()
        };
        let claims = AppClaims {
            sub: Uuid::new_v4(),
            email: "test@example.com".to_string(),
            name: "Test User".to_string(),
            tenant_id: Uuid::new_v4(),
            role: "admin".to_string(),
            org_slug: None,
            iat: now.timestamp(),
            exp,
        };
        encode(
            &Header::new(Algorithm::HS256),
            &claims,
            &EncodingKey::from_secret(secret.as_bytes()),
        )
        .unwrap()
    }

    #[test]
    fn test_verify_valid_token() {
        let secret = "test-secret-key-256-bits-long!!!";
        let token = create_test_token(secret, false);
        let claims = verify_jwt(&token, secret).unwrap();
        assert_eq!(claims.email, "test@example.com");
        assert_eq!(claims.role, "admin");
    }

    #[test]
    fn test_verify_expired_token() {
        let secret = "test-secret-key-256-bits-long!!!";
        let token = create_test_token(secret, true);
        assert!(verify_jwt(&token, secret).is_err());
    }

    #[test]
    fn test_verify_wrong_secret() {
        let token = create_test_token("correct-secret-key!!", false);
        assert!(verify_jwt(&token, "wrong-secret-key!!!!").is_err());
    }

    #[test]
    fn test_extract_bearer_token() {
        assert_eq!(extract_bearer_token("Bearer abc123"), Some("abc123"));
        assert_eq!(extract_bearer_token("Basic abc123"), None);
        assert_eq!(extract_bearer_token("abc123"), None);
    }
}
