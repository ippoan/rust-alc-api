use rust_alc_api::auth::google::GoogleTokenVerifier;
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// テスト用 RSA 秘密鍵 (PKCS#8 PEM)
const TEST_RSA_PRIVATE_KEY: &str = r#"-----BEGIN PRIVATE KEY-----
MIIEvgIBADANBgkqhkiG9w0BAQEFAASCBKgwggSkAgEAAoIBAQDkdxdShM55AWy+
QguzAjm5yFjtQxQ4fVLHAxKRLLXAP4mT244BGsFqj2c3s0qKxbgJ/wWeEXGHazuB
IW5CyB5ERxI1QtiGiVpczkRLmMfBGpRYFcOUU+9q6d7heLIqMdvlEQRse2tYUWVp
PtraeO/O9w2GUufUuTY7rsM26VgYKcrwVFuX3E84QC80usUV4c67jfssmeHFWiiM
QCfzKi916oXpcADWt4Aj/ZoBPk8efpXJCGN9nf3ijIuGI0WmISNO9gtu0GRy4wiU
tHuCAooLZeC0eMUQ5Mc+MBV8bHd36QuBr3TL9jo1jyrj/JRibZgXWkqsRmhxCYYG
BRJBEupjAgMBAAECggEAad8OPb0xeT3ByMEObtvfKErBeuGU40CgNX0lA4V2jCpl
dNFPkw91Bg6CRHufVYOeb3NwhPmMZLa6knKNiAD4ladhtrDNajsIVu13FJkuKDxK
9i7PvVLQJflOwamO1qLYReSG6kafTgQaPJVWEdvtNTmjWNXefON/UFUCPdYQbtE+
+wEQZuNtmpAqfDQ4Y6F+e85PZa+2hFm//UxuVPZWINEN0mR0pg24rSBFJ/WE2LgI
w44lAR55NWkOQ3zyG0F6n3/7EMKezwPYV8+qMok2B36YFw5bXuyRK3Tj2oTDan4S
thUB5V+NA7tqAtTs6LWFbR94AC47KnCrYijTOysO4QKBgQD26upw61TQo4I353qZ
HZytLI4XWO9Eha+/Yh0Um07ypy6ypaBGqhbN/iqUmexVuPHD2arg3REtYzNtCHfh
5bz0yQPP/zfz2d99PpT5CZPCslAkuytKt7Ee2Zdpo0HYGJMXl+65EbiuD85l7MFl
nMnsLW6Ev5iTTjydAclF0WpkGQKBgQDs3mtJidoutFIHC3HvCycfHrayLYfzcmiB
0fIFDVXKmMJG2mQxg1iRavQ6/t/mssXF87C4ZVi32df3jzniFtixiobWU+qDBwL5
+7WnThr8adIEJij1UnnFwrwufd8HS8ReDe8g9gaz47sF0bq89jSsJPPoDLEb0akj
MGbYFeqx2wKBgQCFVuDZr8vyi4njpKZpDzuvrPLims1DBKqewF4R5bjhgvTN1nFS
F8IO5aWa7/BXbnNonyAPJHKFPx/jToJmxAiha/gaF6ngjpSI7wXF4q0fo+lxnH3J
cJ8+mKSSkG4bQ1ITmKF64Z4IqVJ9ajgaJmxIlVQsbcb4LXTAGNnXUTqR6QKBgQC9
okPamBapFYwmP69zZUZoz7oMZA9Xg9zPMjnEeayZijrfZrCYQ0OBCFOHd83hcHaN
yE9PETQ53JnehDgfHZNWcEULChvR0qc7Y51G2G0ab83HrJVV8jWzcfgecH9B5BLO
CfHMPauYEVYFjqcl6Sa6Ostal+6jCvOSTInJraI7yQKBgCnfFGYcF8s6mfE9W8fo
fCPbActS+MGndpEi9W6VV53yJ/xJq9GizREK9wo7Xi25yy/rwnLwLE8ErWx0Ty0Q
qJm2xa03lltL4Da1aYSSe/iQuVDMciE44vQPZuIuiC2ov3wKf/YqAeHjUzwS6DCh
3LbWNlaUKN1w/TFQBw7Sv4wf
-----END PRIVATE KEY-----"#;

/// テスト用 JWKS レスポンス (上記キーの公開鍵)
const TEST_JWKS_JSON: &str = r#"{"keys":[{"kty":"RSA","kid":"test-kid-001","use":"sig","alg":"RS256","n":"5HcXUoTOeQFsvkILswI5uchY7UMUOH1SxwMSkSy1wD-Jk9uOARrBao9nN7NKisW4Cf8FnhFxh2s7gSFuQsgeREcSNULYholaXM5ES5jHwRqUWBXDlFPvaune4XiyKjHb5REEbHtrWFFlaT7a2njvzvcNhlLn1Lk2O67DNulYGCnK8FRbl9xPOEAvNLrFFeHOu437LJnhxVoojEAn8yovdeqF6XAA1reAI_2aAT5PHn6VyQhjfZ394oyLhiNFpiEjTvYLbtBkcuMIlLR7ggKKC2XgtHjFEOTHPjAVfGx3d-kLga90y_Y6NY8q4_yUYm2YF1pKrEZocQmGBgUSQRLqYw","e":"AQAB"}]}"#;

/// テスト用 RS256 JWT を生成
fn create_test_rs256_jwt(client_id: &str) -> String {
    use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};

    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some("test-kid-001".to_string());

    let now = chrono::Utc::now().timestamp() as u64;
    let claims = json!({
        "sub": "google-sub-real-12345",
        "email": "realuser@example.com",
        "name": "Real Google User",
        "picture": "https://example.com/photo.jpg",
        "email_verified": true,
        "aud": client_id,
        "iss": "https://accounts.google.com",
        "exp": now + 3600,
        "iat": now,
    });

    let key = EncodingKey::from_rsa_pem(TEST_RSA_PRIVATE_KEY.as_bytes()).unwrap();
    encode(&header, &claims, &key).unwrap()
}

// ============================================================
// GoogleTokenVerifier::new() + verify() — 実 JWT 検証パス
// ============================================================

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_google_verify_real_path() {
    test_group!("google auth カバレッジ");
    test_case!("RS256 JWT + JWKS モック → verify 成功", {
        let _db = crate::common::DB_RENAME_LOCK.lock().unwrap();
        let _flock = crate::common::db_rename_flock();
        let _env = crate::common::ENV_LOCK.lock().unwrap();
        let jwks_server = MockServer::start().await;

        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_string(TEST_JWKS_JSON))
            .mount(&jwks_server)
            .await;

        std::env::set_var("GOOGLE_JWKS_URL", format!("{}/certs", jwks_server.uri()));

        let client_id = "test-real-client-id";
        let verifier = GoogleTokenVerifier::new(client_id.to_string(), "secret".to_string());

        let token = create_test_rs256_jwt(client_id);
        let claims = verifier.verify(&token).await.unwrap();

        assert_eq!(claims.sub, "google-sub-real-12345");
        assert_eq!(claims.email, "realuser@example.com");
        assert!(claims.email_verified);

        std::env::remove_var("GOOGLE_JWKS_URL");
    });
}

// ============================================================
// verify — JWKS キャッシュヒット (2回目の verify)
// ============================================================

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_google_verify_jwks_cache_hit() {
    test_group!("google auth カバレッジ");
    test_case!("2回目の verify → JWKS キャッシュヒット", {
        let _db = crate::common::DB_RENAME_LOCK.lock().unwrap();
        let _flock = crate::common::db_rename_flock();
        let _env = crate::common::ENV_LOCK.lock().unwrap();
        let jwks_server = MockServer::start().await;

        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_string(TEST_JWKS_JSON))
            .expect(1) // JWKS は1回だけ取得
            .mount(&jwks_server)
            .await;

        std::env::set_var("GOOGLE_JWKS_URL", format!("{}/certs", jwks_server.uri()));

        let client_id = "test-cache-client-id";
        let verifier = GoogleTokenVerifier::new(client_id.to_string(), "secret".to_string());

        let token = create_test_rs256_jwt(client_id);

        // 1回目: JWKS フェッチ
        let _ = verifier.verify(&token).await.unwrap();
        // 2回目: キャッシュヒット
        let claims = verifier.verify(&token).await.unwrap();
        assert_eq!(claims.email, "realuser@example.com");

        std::env::remove_var("GOOGLE_JWKS_URL");
    });
}

// ============================================================
// verify — JWKS キャッシュ期限切れ → 再取得
// ============================================================

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_google_verify_jwks_cache_expired() {
    test_group!("google auth カバレッジ");
    test_case!("JWKS キャッシュ期限切れ → 再取得", {
        let _db = crate::common::DB_RENAME_LOCK.lock().unwrap();
        let _flock = crate::common::db_rename_flock();
        let _env = crate::common::ENV_LOCK.lock().unwrap();
        let jwks_server = MockServer::start().await;

        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_string(TEST_JWKS_JSON))
            .expect(2) // 期限切れ後に再取得
            .mount(&jwks_server)
            .await;

        std::env::set_var("GOOGLE_JWKS_URL", format!("{}/certs", jwks_server.uri()));
        // TTL を 0 に設定 → 常に期限切れ
        std::env::set_var("GOOGLE_JWKS_CACHE_TTL_SECS", "0");

        let client_id = "test-cache-expire";
        let verifier = GoogleTokenVerifier::new(client_id.to_string(), "secret".to_string());
        let token = create_test_rs256_jwt(client_id);

        // 1回目: JWKS フェッチ
        let _ = verifier.verify(&token).await.unwrap();
        // 2回目: TTL=0 なのでキャッシュ期限切れ → 再取得
        let claims = verifier.verify(&token).await.unwrap();
        assert_eq!(claims.email, "realuser@example.com");

        std::env::remove_var("GOOGLE_JWKS_URL");
        std::env::remove_var("GOOGLE_JWKS_CACHE_TTL_SECS");
    });
}

// ============================================================
// verify — 無効なトークン (decode_header 失敗)
// ============================================================

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_google_verify_invalid_token() {
    test_group!("google auth カバレッジ");
    test_case!("不正なトークン → InvalidToken", {
        let _db = crate::common::DB_RENAME_LOCK.lock().unwrap();
        let _flock = crate::common::db_rename_flock();
        let verifier = GoogleTokenVerifier::new("cid".to_string(), "secret".to_string());
        let err = verifier.verify("not-a-jwt").await.unwrap_err();
        assert_eq!(err.to_string(), "invalid token");
    });
}

// ============================================================
// verify — kid なし JWT
// ============================================================

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_google_verify_no_kid() {
    test_group!("google auth カバレッジ");
    test_case!("kid なし JWT → InvalidToken", {
        use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};

        let _db = crate::common::DB_RENAME_LOCK.lock().unwrap();
        let _flock = crate::common::db_rename_flock();
        let header = Header::new(Algorithm::RS256); // kid なし
        let claims = json!({ "sub": "x", "exp": 9999999999u64 });
        let key = EncodingKey::from_rsa_pem(TEST_RSA_PRIVATE_KEY.as_bytes()).unwrap();
        let token = encode(&header, &claims, &key).unwrap();

        let verifier = GoogleTokenVerifier::new("cid".to_string(), "secret".to_string());
        let err = verifier.verify(&token).await.unwrap_err();
        assert_eq!(err.to_string(), "invalid token");
    });
}

// ============================================================
// verify — JWKS フェッチ失敗
// ============================================================

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_google_verify_jwks_fetch_failed() {
    test_group!("google auth カバレッジ");
    test_case!("JWKS サーバーダウン → JwksFetchFailed", {
        let _db = crate::common::DB_RENAME_LOCK.lock().unwrap();
        let _flock = crate::common::db_rename_flock();
        let _env = crate::common::ENV_LOCK.lock().unwrap();
        // 存在しないサーバーを指定
        std::env::set_var("GOOGLE_JWKS_URL", "http://127.0.0.1:19998/certs");

        let client_id = "test-jwks-fail";
        let verifier = GoogleTokenVerifier::new(client_id.to_string(), "secret".to_string());
        let token = create_test_rs256_jwt(client_id);

        let err = verifier.verify(&token).await.unwrap_err();
        assert_eq!(err.to_string(), "failed to fetch JWKS");

        std::env::remove_var("GOOGLE_JWKS_URL");
    });
}

// ============================================================
// verify — JWKS パース失敗
// ============================================================

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_google_verify_jwks_parse_failed() {
    test_group!("google auth カバレッジ");
    test_case!("JWKS 不正 JSON → JwksFetchFailed", {
        let _db = crate::common::DB_RENAME_LOCK.lock().unwrap();
        let _flock = crate::common::db_rename_flock();
        let _env = crate::common::ENV_LOCK.lock().unwrap();
        let jwks_server = MockServer::start().await;

        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_string("not-json"))
            .mount(&jwks_server)
            .await;

        std::env::set_var("GOOGLE_JWKS_URL", format!("{}/certs", jwks_server.uri()));

        let client_id = "test-jwks-parse";
        let verifier = GoogleTokenVerifier::new(client_id.to_string(), "secret".to_string());
        let token = create_test_rs256_jwt(client_id);

        let err = verifier.verify(&token).await.unwrap_err();
        assert_eq!(err.to_string(), "failed to fetch JWKS");

        std::env::remove_var("GOOGLE_JWKS_URL");
    });
}

// ============================================================
// verify — JWKS に kid が見つからない
// ============================================================

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_google_verify_key_not_found() {
    test_group!("google auth カバレッジ");
    test_case!("JWKS に一致する kid なし → KeyNotFound", {
        let _db = crate::common::DB_RENAME_LOCK.lock().unwrap();
        let _flock = crate::common::db_rename_flock();
        let _env = crate::common::ENV_LOCK.lock().unwrap();
        let jwks_server = MockServer::start().await;

        // 別の kid の JWKS を返す
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "keys": [{ "kty": "RSA", "kid": "wrong-kid", "n": "AQAB", "e": "AQAB" }]
            })))
            .mount(&jwks_server)
            .await;

        std::env::set_var("GOOGLE_JWKS_URL", format!("{}/certs", jwks_server.uri()));

        let client_id = "test-key-notfound";
        let verifier = GoogleTokenVerifier::new(client_id.to_string(), "secret".to_string());
        let token = create_test_rs256_jwt(client_id);

        let err = verifier.verify(&token).await.unwrap_err();
        assert_eq!(err.to_string(), "key not found in JWKS");

        std::env::remove_var("GOOGLE_JWKS_URL");
    });
}

// ============================================================
// verify — email_verified = false
// ============================================================

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_google_verify_email_not_verified() {
    test_group!("google auth カバレッジ");
    test_case!("email_verified=false → EmailNotVerified", {
        let _db = crate::common::DB_RENAME_LOCK.lock().unwrap();
        let _flock = crate::common::db_rename_flock();
        let _env = crate::common::ENV_LOCK.lock().unwrap();
        let jwks_server = MockServer::start().await;

        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_string(TEST_JWKS_JSON))
            .mount(&jwks_server)
            .await;

        std::env::set_var("GOOGLE_JWKS_URL", format!("{}/certs", jwks_server.uri()));

        let client_id = "test-email-noverify";

        // email_verified = false の JWT を作成
        use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
        let mut header = Header::new(Algorithm::RS256);
        header.kid = Some("test-kid-001".to_string());
        let now = chrono::Utc::now().timestamp() as u64;
        let claims = json!({
            "sub": "sub-noverify",
            "email": "unverified@example.com",
            "name": "Unverified",
            "email_verified": false,
            "aud": client_id,
            "iss": "https://accounts.google.com",
            "exp": now + 3600,
            "iat": now,
        });
        let key = EncodingKey::from_rsa_pem(TEST_RSA_PRIVATE_KEY.as_bytes()).unwrap();
        let token = encode(&header, &claims, &key).unwrap();

        let verifier = GoogleTokenVerifier::new(client_id.to_string(), "secret".to_string());
        let err = verifier.verify(&token).await.unwrap_err();
        assert_eq!(err.to_string(), "email not verified");

        std::env::remove_var("GOOGLE_JWKS_URL");
    });
}

// ============================================================
// exchange_code — 実パス成功
// ============================================================

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_google_exchange_code_real_success() {
    test_group!("google auth カバレッジ");
    test_case!("code exchange → token → verify 成功", {
        let _db = crate::common::DB_RENAME_LOCK.lock().unwrap();
        let _flock = crate::common::db_rename_flock();
        let _env = crate::common::ENV_LOCK.lock().unwrap();
        let jwks_server = MockServer::start().await;
        let token_server = MockServer::start().await;

        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_string(TEST_JWKS_JSON))
            .mount(&jwks_server)
            .await;

        let client_id = "test-exchange-client";
        let id_token = create_test_rs256_jwt(client_id);

        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "id_token": id_token })))
            .mount(&token_server)
            .await;

        std::env::set_var("GOOGLE_JWKS_URL", format!("{}/certs", jwks_server.uri()));
        std::env::set_var("GOOGLE_TOKEN_URL", format!("{}/token", token_server.uri()));

        let verifier = GoogleTokenVerifier::new(client_id.to_string(), "secret".to_string());
        let claims = verifier
            .exchange_code("valid-code", "http://localhost/callback")
            .await
            .unwrap();

        assert_eq!(claims.email, "realuser@example.com");

        std::env::remove_var("GOOGLE_JWKS_URL");
        std::env::remove_var("GOOGLE_TOKEN_URL");
    });
}

// ============================================================
// exchange_code — token endpoint 接続失敗
// ============================================================

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_google_exchange_code_connection_error() {
    test_group!("google auth カバレッジ");
    test_case!("token endpoint 接続不可 → TokenExchangeFailed", {
        let _db = crate::common::DB_RENAME_LOCK.lock().unwrap();
        let _flock = crate::common::db_rename_flock();
        let _env = crate::common::ENV_LOCK.lock().unwrap();
        std::env::set_var("GOOGLE_TOKEN_URL", "http://127.0.0.1:19997/token");

        let verifier = GoogleTokenVerifier::new("cid".to_string(), "secret".to_string());
        let err = verifier
            .exchange_code("code", "http://localhost/cb")
            .await
            .unwrap_err();
        assert_eq!(err.to_string(), "failed to exchange authorization code");

        std::env::remove_var("GOOGLE_TOKEN_URL");
    });
}

// ============================================================
// exchange_code — token endpoint 非200
// ============================================================

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_google_exchange_code_error_response() {
    test_group!("google auth カバレッジ");
    test_case!("token endpoint 400 → TokenExchangeFailed", {
        let _db = crate::common::DB_RENAME_LOCK.lock().unwrap();
        let _flock = crate::common::db_rename_flock();
        let _env = crate::common::ENV_LOCK.lock().unwrap();
        let token_server = MockServer::start().await;

        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(400).set_body_string("invalid_grant"))
            .mount(&token_server)
            .await;

        std::env::set_var("GOOGLE_TOKEN_URL", format!("{}/token", token_server.uri()));

        let verifier = GoogleTokenVerifier::new("cid".to_string(), "secret".to_string());
        let err = verifier
            .exchange_code("bad-code", "http://localhost/cb")
            .await
            .unwrap_err();
        assert_eq!(err.to_string(), "failed to exchange authorization code");

        std::env::remove_var("GOOGLE_TOKEN_URL");
    });
}

// ============================================================
// exchange_code — token endpoint 不正 JSON
// ============================================================

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_google_exchange_code_invalid_json() {
    test_group!("google auth カバレッジ");
    test_case!("token endpoint 不正 JSON → TokenExchangeFailed", {
        let _db = crate::common::DB_RENAME_LOCK.lock().unwrap();
        let _flock = crate::common::db_rename_flock();
        let _env = crate::common::ENV_LOCK.lock().unwrap();
        let token_server = MockServer::start().await;

        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_string("not-json"))
            .mount(&token_server)
            .await;

        std::env::set_var("GOOGLE_TOKEN_URL", format!("{}/token", token_server.uri()));

        let verifier = GoogleTokenVerifier::new("cid".to_string(), "secret".to_string());
        let err = verifier
            .exchange_code("code", "http://localhost/cb")
            .await
            .unwrap_err();
        assert_eq!(err.to_string(), "failed to exchange authorization code");

        std::env::remove_var("GOOGLE_TOKEN_URL");
    });
}

// ============================================================
// verify — JWT 署名不一致 (InvalidToken via decode)
// ============================================================

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_google_verify_signature_mismatch() {
    test_group!("google auth カバレッジ");
    test_case!("署名不一致 JWT → InvalidToken", {
        let _db = crate::common::DB_RENAME_LOCK.lock().unwrap();
        let _flock = crate::common::db_rename_flock();
        let _env = crate::common::ENV_LOCK.lock().unwrap();
        let jwks_server = MockServer::start().await;

        // n を不正な値にした JWKS (署名検証失敗)
        let bad_jwks = json!({
            "keys": [{
                "kty": "RSA",
                "kid": "test-kid-001",
                "n": "0vx7agoebGcQSuuPiLJXZptN9nndrQmbXEps2aiAFbWhM78LhWx4cbbfAAtVT86zwu1RK7aPFFxuhDR1L6tSoc_BJECPebWKRXjBZCiFV4n3oknjhMstn64tZ_2W-5JsGY4Hc5n9yBXArwl93lqt7_RN5w6Cf0h4QyQ5v-65YGjQR0_FDW2QvzqY368QQMicAtaSqzs8KJZgnYb9c7d0zgdAZHzu6qMQvRL5hajrn1n91CbOpbISD08qNLyrdkt-bFTWhAI4vMQFh6WeZu0fM4lFd2NcRwr3XPksINHaQ-G_xBniIqbw0Ls1jF44-csFCur-kEgU8awapJzKnqDKgw",
                "e": "AQAB"
            }]
        });

        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_json(bad_jwks))
            .mount(&jwks_server)
            .await;

        std::env::set_var("GOOGLE_JWKS_URL", format!("{}/certs", jwks_server.uri()));

        let client_id = "test-sig-mismatch";
        let verifier = GoogleTokenVerifier::new(client_id.to_string(), "secret".to_string());
        let token = create_test_rs256_jwt(client_id);

        let err = verifier.verify(&token).await.unwrap_err();
        assert_eq!(err.to_string(), "invalid token");

        std::env::remove_var("GOOGLE_JWKS_URL");
    });
}
