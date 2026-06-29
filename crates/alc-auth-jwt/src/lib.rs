//! 薄い leaf crate: app / internal JWT の発行・検証と refresh token hash。
//!
//! `alc-core` から auth_jwt ロジックだけを切り出したもの (Refs #410)。
//! 依存は `jsonwebtoken / serde / uuid / chrono / sha2 / tracing` のみで、
//! sqlx / reqwest / ring / ts-rs / axum を一切持ち込まない (= ビルドが軽く
//! 横断利用しやすい)。
//!
//! `alc-core::auth_jwt` がこの crate を glob re-export しているため、
//! repo 内の既存 import (`alc_core::auth_jwt::...`) は無変更で通る。

#[cfg(test)]
#[macro_use]
mod test_macros;

use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub org_slug: Option<String>,
    pub iat: i64,
    pub exp: i64,
    /// 発行環境 (`"staging"` / `"prod"`)。Refs #218 — auth-worker と JWT_SECRET を
    /// 共有しているため、staging で発行された token が prod の verifier を素通り
    /// しないよう、token に発行環境を載せて verify 側で `current_env_label()` と
    /// 一致を強制する。Option なのは旧 token 互換性のため (未設定なら一致チェック
    /// を skip。deploy 後 1h で旧 token expire するので実質必須化と等価)。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub env: Option<String>,
}

/// `create_access_token` の入力。`alc-core::models::User` への依存を持ち込まない
/// ため、必要なフィールドだけを受け取る (Refs #410)。alc-core 側に `&User` を
/// 受ける互換 wrapper を残しているので、呼び出し側の変更は不要。
pub struct AccessTokenInput {
    pub sub: Uuid,
    pub email: String,
    pub name: String,
    pub tenant_id: Uuid,
    pub role: String,
}

/// JWT シークレットのラッパー
#[derive(Clone)]
pub struct JwtSecret(pub String);

/// 現在の Cloud Run 環境ラベルを `"staging"` / `"prod"` で返す。Refs #218。
///
/// 判定は `cloudrun/render.sh` で注入される `STAGING_MODE` env var ベース:
///   - `STAGING_MODE=true` → `"staging"`
///   - それ以外 (未設定 / `"false"`) → `"prod"`
///
/// JWT の `env` claim に書き込み、verify 時にも同関数の戻り値と比較する。
pub fn current_env_label() -> &'static str {
    match std::env::var("STAGING_MODE").as_deref() {
        Ok("true") => "staging",
        _ => "prod",
    }
}

/// Access token を発行
pub fn create_access_token(
    input: &AccessTokenInput,
    secret: &JwtSecret,
    org_slug: Option<String>,
) -> Result<String, jsonwebtoken::errors::Error> {
    let now = Utc::now();
    let claims = AppClaims {
        sub: input.sub,
        email: input.email.clone(),
        name: input.name.clone(),
        tenant_id: input.tenant_id,
        role: input.role.clone(),
        org_slug,
        iat: now.timestamp(),
        exp: (now + Duration::seconds(ACCESS_TOKEN_EXPIRY_SECS)).timestamp(),
        env: Some(current_env_label().to_string()),
    };

    encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(secret.0.as_bytes()),
    )
}

/// Access token を検証してクレームを返す。Refs #218 — token の `env` claim と
/// `current_env_label()` の一致を強制する (旧 token 互換のため env 未設定は通す)。
pub fn verify_access_token(
    token: &str,
    secret: &JwtSecret,
) -> Result<AppClaims, jsonwebtoken::errors::Error> {
    let mut validation = Validation::new(Algorithm::HS256);
    validation.validate_exp = true;

    let token_data = decode::<AppClaims>(
        token,
        &DecodingKey::from_secret(secret.0.as_bytes()),
        &validation,
    )?;

    if let Some(token_env) = token_data.claims.env.as_deref() {
        let expected = current_env_label();
        if token_env != expected {
            return Err(jsonwebtoken::errors::Error::from(
                jsonwebtoken::errors::ErrorKind::InvalidIssuer,
            ));
        }
    }

    Ok(token_data.claims)
}

/// 内部 API 用 JWT のクレーム (auth-worker → rust-alc-api 間の callback で使用)
///
/// 通常のユーザー JWT (`AppClaims`) と区別するため `aud = "alc-api-internal"` を強制する。
/// `JWT_SECRET` (HS256) は両者で共有しているため、aud で用途分離しないと
/// auth-worker が発行した内部 JWT がうっかり require_jwt 経路で受け入れられかねない。
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct InternalClaims {
    pub iss: String,
    pub aud: String,
    pub iat: i64,
    pub exp: i64,
    /// 発行環境 (`"staging"` / `"prod"`)。Refs #218 — auth-worker と JWT_SECRET を
    /// 共有しているため、staging で発行された internal token が prod で素通り
    /// しないよう、token に発行環境を載せて verify 側で `current_env_label()` と
    /// 一致を強制する。Option なのは旧 token 互換性のため (実 lifetime は 60s
    /// なので 1 分で旧 token は expire し実質必須化)。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub env: Option<String>,
}

/// 内部 API 用 JWT を発行 (主に rust-alc-api 内テストや CLI から auth-worker 用 JWT を生成する用途)
pub fn create_internal_token(
    secret: &JwtSecret,
    iss: &str,
    ttl_seconds: i64,
) -> Result<String, jsonwebtoken::errors::Error> {
    let now = Utc::now();
    let claims = InternalClaims {
        iss: iss.to_string(),
        aud: INTERNAL_AUD.to_string(),
        iat: now.timestamp(),
        exp: (now + Duration::seconds(ttl_seconds)).timestamp(),
        env: Some(current_env_label().to_string()),
    };
    encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(secret.0.as_bytes()),
    )
}

/// 内部 API 用 JWT を検証。Refs #218 — token の `env` claim と `current_env_label()`
/// の一致を強制する (旧 token 互換のため env 未設定は通す)。
pub fn verify_internal_token(
    token: &str,
    secret: &JwtSecret,
) -> Result<InternalClaims, jsonwebtoken::errors::Error> {
    let mut validation = Validation::new(Algorithm::HS256);
    validation.validate_exp = true;
    validation.set_audience(&[INTERNAL_AUD]);

    let token_data = decode::<InternalClaims>(
        token,
        &DecodingKey::from_secret(secret.0.as_bytes()),
        &validation,
    )?;

    if let Some(token_env) = token_data.claims.env.as_deref() {
        let expected = current_env_label();
        if token_env != expected {
            return Err(jsonwebtoken::errors::Error::from(
                jsonwebtoken::errors::ErrorKind::InvalidIssuer,
            ));
        }
    }

    Ok(token_data.claims)
}

pub const INTERNAL_AUD: &str = "alc-api-internal";

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
    use std::sync::Mutex;

    /// JWT 関連テストで `STAGING_MODE` env var を触る (current_env_label() 経由で
    /// 暗黙参照される)。並列実行で値が leak しないよう本ファイル内テストを直列化する。
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn test_input() -> AccessTokenInput {
        AccessTokenInput {
            sub: Uuid::new_v4(),
            tenant_id: Uuid::new_v4(),
            email: "test@example.com".to_string(),
            name: "Test User".to_string(),
            role: "admin".to_string(),
        }
    }

    #[test]
    fn test_create_and_verify_access_token() {
        // STAGING_MODE が並列テストで変わると create と verify で env 値が
        // ずれて InvalidIssuer になるので serialize (Refs #218)。
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        test_group!("JWTトークン");
        test_case!("アクセストークンの生成と検証", {
            let input = test_input();
            let secret = JwtSecret("test-secret-key-256-bits-long!!!".to_string());

            let token =
                create_access_token(&input, &secret, Some("test-slug".to_string())).unwrap();
            let claims = verify_access_token(&token, &secret).unwrap();

            assert_eq!(claims.sub, input.sub);
            assert_eq!(claims.email, input.email);
            assert_eq!(claims.tenant_id, input.tenant_id);
            assert_eq!(claims.role, "admin");
        });
    }

    #[test]
    fn test_verify_with_wrong_secret_fails() {
        test_group!("JWTトークン");
        test_case!("不正なシークレットで検証失敗", {
            let input = test_input();
            let secret = JwtSecret("correct-secret-key-256-bits!!!".to_string());
            let wrong_secret = JwtSecret("wrong-secret-key-256-bits!!!!!".to_string());

            let token =
                create_access_token(&input, &secret, Some("test-slug".to_string())).unwrap();
            assert!(verify_access_token(&token, &wrong_secret).is_err());
        });
    }

    #[test]
    fn test_create_and_verify_internal_token() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        test_group!("内部JWT");
        test_case!(
            "内部トークンの生成と検証 (aud=alc-api-internal)",
            {
                let secret = JwtSecret("test-internal-secret-256-bits!!!".to_string());
                let token = create_internal_token(&secret, "auth-worker", 60).unwrap();
                let claims = verify_internal_token(&token, &secret).unwrap();
                assert_eq!(claims.iss, "auth-worker");
                assert_eq!(claims.aud, INTERNAL_AUD);
            }
        );
    }

    #[test]
    fn test_internal_token_rejects_user_token() {
        test_group!("内部JWT");
        test_case!(
            "ユーザートークンは内部検証で拒否される (aud 不一致)",
            {
                let input = test_input();
                let secret = JwtSecret("shared-secret-key-256-bits-long!".to_string());
                let user_token = create_access_token(&input, &secret, None).unwrap();
                assert!(verify_internal_token(&user_token, &secret).is_err());
            }
        );
    }

    #[test]
    fn test_user_token_rejects_internal_token() {
        test_group!("内部JWT");
        test_case!(
            "内部トークンはユーザー検証で拒否される (Claims 不一致)",
            {
                let secret = JwtSecret("shared-secret-key-256-bits-long!".to_string());
                let internal = create_internal_token(&secret, "auth-worker", 60).unwrap();
                // AppClaims に sub/email/tenant_id 等が無いので decode 失敗
                assert!(verify_access_token(&internal, &secret).is_err());
            }
        );
    }

    #[test]
    fn test_internal_token_wrong_aud_rejected() {
        test_group!("内部JWT");
        test_case!("間違った aud は拒否される", {
            use jsonwebtoken::{encode as jwt_encode, EncodingKey, Header};
            let secret = JwtSecret("test-secret-key-256-bits-long!!!".to_string());
            let now = Utc::now();
            let bad = InternalClaims {
                iss: "auth-worker".to_string(),
                aud: "wrong-aud".to_string(),
                iat: now.timestamp(),
                exp: (now + Duration::seconds(60)).timestamp(),
                env: None,
            };
            let token = jwt_encode(
                &Header::new(Algorithm::HS256),
                &bad,
                &EncodingKey::from_secret(secret.0.as_bytes()),
            )
            .unwrap();
            assert!(verify_internal_token(&token, &secret).is_err());
        });
    }

    #[test]
    fn test_internal_token_expired_rejected() {
        test_group!("内部JWT");
        test_case!("期限切れの内部トークンは拒否される", {
            use jsonwebtoken::{encode as jwt_encode, EncodingKey, Header};
            let secret = JwtSecret("test-secret-key-256-bits-long!!!".to_string());
            let now = Utc::now();
            // jsonwebtoken のデフォルト leeway は 60s なので、それを超える過去にする
            let expired = InternalClaims {
                iss: "auth-worker".to_string(),
                aud: INTERNAL_AUD.to_string(),
                iat: (now - Duration::seconds(7200)).timestamp(),
                exp: (now - Duration::seconds(3600)).timestamp(),
                env: None,
            };
            let token = jwt_encode(
                &Header::new(Algorithm::HS256),
                &expired,
                &EncodingKey::from_secret(secret.0.as_bytes()),
            )
            .unwrap();
            assert!(verify_internal_token(&token, &secret).is_err());
        });
    }

    #[test]
    fn test_refresh_token_generation() {
        test_group!("JWTトークン");
        test_case!("リフレッシュトークン生成", {
            let (raw, hash) = create_refresh_token();
            assert!(raw.starts_with("rt_"));
            assert_eq!(hash, hash_refresh_token(&raw));
        });
    }

    #[test]
    fn test_refresh_token_hash_consistency() {
        test_group!("JWTトークン");
        test_case!("リフレッシュトークンハッシュの一貫性", {
            let token = "rt_test123";
            let hash1 = hash_refresh_token(token);
            let hash2 = hash_refresh_token(token);
            assert_eq!(hash1, hash2);
        });
    }

    #[test]
    fn test_refresh_token_expires_at_is_in_future() {
        test_group!("JWTトークン");
        test_case!("リフレッシュトークン有効期限は未来", {
            let exp = refresh_token_expires_at();
            assert!(exp > Utc::now());
        });
    }

    // -------------------------------------------------------------------
    // Refs #218: env claim による cross-env token replay 防止のテスト
    // -------------------------------------------------------------------

    /// ENV_LOCK 取得 + `STAGING_MODE` env var の書き換え + Drop で復元する RAII guard。
    /// 本ファイル内のテストで `current_env_label()` が安定するよう必ず使う。
    struct EnvGuard {
        _lock: std::sync::MutexGuard<'static, ()>,
        prev: Option<String>,
    }
    impl EnvGuard {
        /// STAGING_MODE を `value` に設定 (`"true"` / `"false"` 等)。lock 取得込み。
        fn set(value: &str) -> Self {
            let lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
            let prev = std::env::var("STAGING_MODE").ok();
            // SAFETY: ENV_LOCK で本ファイル内テストの STAGING_MODE 操作を直列化済。
            unsafe {
                std::env::set_var("STAGING_MODE", value);
            }
            Self { _lock: lock, prev }
        }
    }
    impl Drop for EnvGuard {
        fn drop(&mut self) {
            unsafe {
                if let Some(prev) = &self.prev {
                    std::env::set_var("STAGING_MODE", prev);
                } else {
                    std::env::remove_var("STAGING_MODE");
                }
            }
        }
    }

    #[test]
    fn test_current_env_label_staging() {
        let _g = EnvGuard::set("true");
        assert_eq!(current_env_label(), "staging");
    }

    #[test]
    fn test_env_guard_restores_previous_value_on_drop() {
        // Drop の `Some(prev)` 分岐をカバー: STAGING_MODE が pre-set されている
        // 状態で EnvGuard::set → drop → 元値に復元されることを確認する。
        // ENV_LOCK は EnvGuard 内で取り直すので、setup/cleanup でのみ取得する。
        {
            let _setup = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
            unsafe {
                std::env::set_var("STAGING_MODE", "preset-value");
            }
        }

        {
            let _g = EnvGuard::set("override-value");
            assert_eq!(
                std::env::var("STAGING_MODE").as_deref(),
                Ok("override-value")
            );
        }
        // Drop が `Some(prev)` 分岐に入って "preset-value" を復元するはず
        assert_eq!(std::env::var("STAGING_MODE").as_deref(), Ok("preset-value"));

        {
            let _cleanup = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
            unsafe {
                std::env::remove_var("STAGING_MODE");
            }
        }
    }

    #[test]
    fn test_current_env_label_prod() {
        let _g = EnvGuard::set("false");
        assert_eq!(current_env_label(), "prod");
    }

    #[test]
    fn test_current_env_label_unknown_value_defaults_to_prod() {
        let _g = EnvGuard::set("anything-else");
        assert_eq!(current_env_label(), "prod");
    }

    #[test]
    fn test_access_token_carries_env_claim_staging() {
        let _g = EnvGuard::set("true");
        let input = test_input();
        let secret = JwtSecret("test-secret-key-256-bits-long!!!".to_string());
        let token = create_access_token(&input, &secret, None).unwrap();
        let claims = verify_access_token(&token, &secret).unwrap();
        assert_eq!(claims.env.as_deref(), Some("staging"));
    }

    #[test]
    fn test_access_token_carries_env_claim_prod() {
        let _g = EnvGuard::set("false");
        let input = test_input();
        let secret = JwtSecret("test-secret-key-256-bits-long!!!".to_string());
        let token = create_access_token(&input, &secret, None).unwrap();
        let claims = verify_access_token(&token, &secret).unwrap();
        assert_eq!(claims.env.as_deref(), Some("prod"));
    }

    #[test]
    fn test_internal_token_carries_env_claim() {
        let _g = EnvGuard::set("true");
        let secret = JwtSecret("test-secret-key-256-bits-long!!!".to_string());
        let token = create_internal_token(&secret, "auth-worker", 60).unwrap();
        let claims = verify_internal_token(&token, &secret).unwrap();
        assert_eq!(claims.env.as_deref(), Some("staging"));
    }

    #[test]
    fn test_access_token_env_mismatch_rejected() {
        // staging で発行 → prod で verify (= cross-env replay) は reject
        let secret = JwtSecret("shared-secret-key-256-bits-long!".to_string());
        let input = test_input();

        // sign 時は staging
        let _g_sign = EnvGuard::set("true");
        let token = create_access_token(&input, &secret, None).unwrap();
        drop(_g_sign);

        // verify 時は prod
        let _g_verify = EnvGuard::set("false");
        let err = verify_access_token(&token, &secret).unwrap_err();
        assert!(matches!(
            err.kind(),
            jsonwebtoken::errors::ErrorKind::InvalidIssuer
        ));
    }

    #[test]
    fn test_internal_token_env_mismatch_rejected() {
        let secret = JwtSecret("shared-secret-key-256-bits-long!".to_string());

        let _g_sign = EnvGuard::set("true");
        let token = create_internal_token(&secret, "auth-worker", 60).unwrap();
        drop(_g_sign);

        let _g_verify = EnvGuard::set("false");
        let err = verify_internal_token(&token, &secret).unwrap_err();
        assert!(matches!(
            err.kind(),
            jsonwebtoken::errors::ErrorKind::InvalidIssuer
        ));
    }

    #[test]
    fn test_access_token_without_env_claim_accepted_for_compat() {
        // 旧 token (env field 無し) は backward compat のため、いずれの env でも通す
        use jsonwebtoken::{encode as jwt_encode, EncodingKey, Header};
        let secret = JwtSecret("test-secret-key-256-bits-long!!!".to_string());
        let input = test_input();
        let now = Utc::now();
        let claims_without_env = AppClaims {
            sub: input.sub,
            email: input.email.clone(),
            name: input.name.clone(),
            tenant_id: input.tenant_id,
            role: input.role.clone(),
            org_slug: None,
            iat: now.timestamp(),
            exp: (now + Duration::seconds(60)).timestamp(),
            env: None,
        };
        let token = jwt_encode(
            &Header::new(Algorithm::HS256),
            &claims_without_env,
            &EncodingKey::from_secret(secret.0.as_bytes()),
        )
        .unwrap();
        let _g = EnvGuard::set("true");
        assert!(verify_access_token(&token, &secret).is_ok());
        drop(_g);
        let _g = EnvGuard::set("false");
        assert!(verify_access_token(&token, &secret).is_ok());
    }

    #[test]
    fn test_internal_token_without_env_claim_accepted_for_compat() {
        use jsonwebtoken::{encode as jwt_encode, EncodingKey, Header};
        let secret = JwtSecret("test-secret-key-256-bits-long!!!".to_string());
        let now = Utc::now();
        let claims_without_env = InternalClaims {
            iss: "auth-worker".to_string(),
            aud: INTERNAL_AUD.to_string(),
            iat: now.timestamp(),
            exp: (now + Duration::seconds(60)).timestamp(),
            env: None,
        };
        let token = jwt_encode(
            &Header::new(Algorithm::HS256),
            &claims_without_env,
            &EncodingKey::from_secret(secret.0.as_bytes()),
        )
        .unwrap();
        let _g = EnvGuard::set("true");
        assert!(verify_internal_token(&token, &secret).is_ok());
        drop(_g);
        let _g = EnvGuard::set("false");
        assert!(verify_internal_token(&token, &secret).is_ok());
    }
}
