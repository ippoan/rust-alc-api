use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use ring::aead::{Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM};
use ring::rand::{SecureRandom, SystemRandom};
use serde_json::Value;
use uuid::Uuid;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::common;

/// AES-256-GCM で暗号化 (sso_admin と同一ロジック)
fn encrypt_test_secret(plaintext: &str, key_material: &str) -> String {
    let mut key_bytes = [0u8; 32];
    let hash = <sha2::Sha256 as sha2::Digest>::digest(key_material.as_bytes());
    key_bytes.copy_from_slice(&hash);
    let unbound = UnboundKey::new(&AES_256_GCM, &key_bytes).unwrap();
    let key = LessSafeKey::new(unbound);
    let rng = SystemRandom::new();
    let mut nonce_bytes = [0u8; 12];
    rng.fill(&mut nonce_bytes).unwrap();
    let nonce = Nonce::assume_unique_for_key(nonce_bytes);
    let mut in_out = plaintext.as_bytes().to_vec();
    key.seal_in_place_append_tag(nonce, Aad::empty(), &mut in_out)
        .unwrap();
    let mut data = nonce_bytes.to_vec();
    data.extend_from_slice(&in_out);
    BASE64.encode(&data)
}

/// SSO config をテスト DB に挿入
async fn insert_sso_config(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    external_org_id: &str,
    client_secret_encrypted: &str,
    woff_id: Option<&str>,
) {
    sqlx::query(
        r#"
        INSERT INTO sso_provider_configs (tenant_id, provider, client_id, client_secret_encrypted, external_org_id, woff_id)
        VALUES ($1, 'lineworks', $2, $3, $4, $5)
        ON CONFLICT (provider, external_org_id) DO UPDATE
        SET tenant_id = $1, client_id = $2, client_secret_encrypted = $3, woff_id = $5
        "#,
    )
    .bind(tenant_id)
    .bind(format!("test-client-{}", Uuid::new_v4().simple()))
    .bind(client_secret_encrypted)
    .bind(external_org_id)
    .bind(woff_id)
    .execute(pool)
    .await
    .expect("Failed to insert SSO config");
}

// ============================================================
// A: my_orgs テナント未存在 → vec![] (line 345)
// ============================================================

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_my_orgs_tenant_not_found() {
    test_group!("auth カバレッジ");
    test_case!("存在しないテナントで my_orgs → 空配列", {
        let _db = crate::common::DB_RENAME_LOCK.lock().unwrap();
        let _flock = crate::common::db_rename_flock();
        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state.clone()).await;

        // 存在しない tenant_id で JWT を発行 (my_orgs は JWT 内の tenant_id で DB 検索)
        let ghost_tenant_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let jwt = crate::common::create_test_jwt_for_user(
            user_id,
            ghost_tenant_id,
            "ghost@test.com",
            "admin",
        );

        let client = reqwest::Client::new();
        let res = client
            .post(format!("{base_url}/api/my-orgs"))
            .header("Authorization", format!("Bearer {jwt}"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        let body: Value = res.json().await.unwrap();
        let orgs = body["organizations"].as_array().unwrap();
        assert_eq!(orgs.len(), 0, "Should return empty organizations");
    });
}

// ============================================================
// B: google_callback exchange failure → 502 (lines 427-429)
// ============================================================

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_google_callback_exchange_failure() {
    test_group!("auth カバレッジ");
    test_case!("有効 state + 無効 code で 502", {
        let _db = crate::common::DB_RENAME_LOCK.lock().unwrap();
        let _flock = crate::common::db_rename_flock();
        let _env = crate::common::ENV_LOCK.lock().unwrap();
        std::env::set_var("OAUTH_STATE_SECRET", "test-oauth-state-secret");
        std::env::set_var("API_ORIGIN", "http://localhost:9999");

        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state).await;

        // 有効な state を生成
        let state_payload = rust_alc_api::auth::lineworks::state::StatePayload {
            redirect_uri: "https://example.com/login".into(),
            nonce: "test-nonce".into(),
            provider: "google".into(),
            external_org_id: String::new(),
        };
        let signed_state =
            rust_alc_api::auth::lineworks::state::sign(&state_payload, "test-oauth-state-secret");

        let client = reqwest::ClientBuilder::new()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();

        // invalid-code → exchange_code failure → 502 BAD_GATEWAY
        let res = client
            .get(format!(
                "{base_url}/api/auth/google/callback?code=invalid-code&state={signed_state}"
            ))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 502, "Invalid code should return 502");
    });
}

// ============================================================
// C: lineworks_redirect happy path (lines 471-525)
// ============================================================

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_lineworks_redirect_happy_path() {
    test_group!("auth カバレッジ");
    test_case!(
        "SSO config 存在 → LINE WORKS authorize URL にリダイレクト",
        {
            let _db = crate::common::DB_RENAME_LOCK.lock().unwrap();
            let _flock = crate::common::db_rename_flock();
            let _env = crate::common::ENV_LOCK.lock().unwrap();
            std::env::set_var("OAUTH_STATE_SECRET", "test-oauth-state-secret-lw");

            let state = crate::common::setup_app_state().await;
            let base_url = crate::common::spawn_test_server(state.clone()).await;

            let tenant_id = crate::common::create_test_tenant(state.pool(), "LW Redirect").await;
            let encrypted = encrypt_test_secret("dummy-secret", crate::common::TEST_JWT_SECRET);
            insert_sso_config(state.pool(), tenant_id, "test-lw-domain", &encrypted, None).await;

            let client = reqwest::ClientBuilder::new()
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .unwrap();

            let res = client
            .get(format!(
                "{base_url}/api/auth/lineworks/redirect?domain=test-lw-domain&redirect_uri=https://example.com/callback"
            ))
            .send()
            .await
            .unwrap();

            assert_eq!(res.status(), 307, "Should redirect to LINE WORKS");
            let location = res.headers().get("location").unwrap().to_str().unwrap();
            assert!(
                location.contains("auth.worksmobile.com"),
                "Should redirect to LINE WORKS authorize URL, got: {location}"
            );
            assert!(location.contains("state="), "Should contain state param");
        }
    );
}

// ============================================================
// D: lineworks_callback happy path (lines 553-654, 774-806, 825)
// ============================================================

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_lineworks_callback_happy_path() {
    test_group!("auth カバレッジ");
    test_case!(
        "wiremock で LINE WORKS API をモック → 307 リダイレクト",
        {
            let _db = crate::common::DB_RENAME_LOCK.lock().unwrap();
            let _flock = crate::common::db_rename_flock();
            let _env = crate::common::ENV_LOCK.lock().unwrap();
            let mock_server = MockServer::start().await;

            // TOKEN_URL mock
            Mock::given(method("POST"))
                .and(path("/oauth2/v2.0/token"))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "access_token": "mock-lw-access-token",
                    "token_type": "Bearer",
                    "expires_in": 3600
                })))
                .mount(&mock_server)
                .await;

            // USERINFO_URL mock
            Mock::given(method("GET"))
                .and(path("/v1.0/users/me"))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "userId": "lw-user-001",
                    "userName": {
                        "lastName": "テスト",
                        "firstName": "太郎"
                    },
                    "email": "taro@test-lw-cb.example.com"
                })))
                .mount(&mock_server)
                .await;

            // env vars for mock server
            std::env::set_var(
                "LINEWORKS_TOKEN_URL",
                format!("{}/oauth2/v2.0/token", mock_server.uri()),
            );
            std::env::set_var(
                "LINEWORKS_USERINFO_URL",
                format!("{}/v1.0/users/me", mock_server.uri()),
            );
            std::env::set_var("OAUTH_STATE_SECRET", "test-oauth-state-secret-cb");
            std::env::set_var("API_ORIGIN", "http://localhost:9999");

            let state = crate::common::setup_app_state().await;
            let base_url = crate::common::spawn_test_server(state.clone()).await;

            // 前回テストの残存ユーザーを削除 (INSERT パスをカバーするため)
            sqlx::query("DELETE FROM users WHERE lineworks_id = 'lw-user-001'")
                .execute(state.pool())
                .await
                .unwrap();

            let tenant_id = crate::common::create_test_tenant(state.pool(), "LW Callback").await;
            let encrypted =
                encrypt_test_secret("test-client-secret", crate::common::TEST_JWT_SECRET);
            let ext_org_id = format!("test-lw-cb-{}", Uuid::new_v4().simple());
            insert_sso_config(state.pool(), tenant_id, &ext_org_id, &encrypted, None).await;

            // 有効な HMAC state 生成 (subdomain URL for extract_parent_domain coverage)
            let state_payload = rust_alc_api::auth::lineworks::state::StatePayload {
                redirect_uri: "https://items.mtamaramu.com/login".into(),
                nonce: Uuid::new_v4().to_string(),
                provider: "lineworks".into(),
                external_org_id: ext_org_id,
            };
            let signed_state = rust_alc_api::auth::lineworks::state::sign(
                &state_payload,
                "test-oauth-state-secret-cb",
            );

            let client = reqwest::ClientBuilder::new()
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .unwrap();

            let res = client
                .get(format!(
                "{base_url}/api/auth/lineworks/callback?code=mock-auth-code&state={signed_state}"
            ))
                .send()
                .await
                .unwrap();

            assert_eq!(res.status(), 307, "Should redirect with tokens");
            let location = res.headers().get("location").unwrap().to_str().unwrap();
            assert!(
                location.contains("token="),
                "Redirect should contain token, got: {location}"
            );
            assert!(
                location.contains("refresh_token="),
                "Redirect should contain refresh_token"
            );

            // Cookie should set parent domain
            let cookie = res.headers().get("set-cookie").unwrap().to_str().unwrap();
            assert!(
                cookie.contains("mtamaramu.com"),
                "Cookie should use parent domain, got: {cookie}"
            );

            // Cleanup env
            std::env::remove_var("LINEWORKS_TOKEN_URL");
            std::env::remove_var("LINEWORKS_USERINFO_URL");
        }
    );
}

// ============================================================
// E: woff_config happy path (lines 675-691)
// ============================================================

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_woff_config_happy_path() {
    test_group!("auth カバレッジ");
    test_case!("SSO config + woff_id あり → woffId を返す", {
        let _db = crate::common::DB_RENAME_LOCK.lock().unwrap();
        let _flock = crate::common::db_rename_flock();
        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state.clone()).await;

        let tenant_id = crate::common::create_test_tenant(state.pool(), "WOFF Config").await;
        let encrypted = encrypt_test_secret("dummy", crate::common::TEST_JWT_SECRET);
        let domain = format!("woff-cfg-{}", Uuid::new_v4().simple());
        insert_sso_config(
            state.pool(),
            tenant_id,
            &domain,
            &encrypted,
            Some("test-woff-id-123"),
        )
        .await;

        let client = reqwest::Client::new();
        let res = client
            .get(format!("{base_url}/api/auth/woff-config?domain={domain}"))
            .send()
            .await
            .unwrap();

        assert_eq!(res.status(), 200);
        let body: Value = res.json().await.unwrap();
        assert_eq!(body["woffId"], "test-woff-id-123");
    });
}

// ============================================================
// E2: woff_config woff_id なし → 404 (lines 686-689)
// ============================================================

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_woff_config_no_woff_id() {
    test_group!("auth カバレッジ");
    test_case!("SSO config あるが woff_id なし → 404", {
        let _db = crate::common::DB_RENAME_LOCK.lock().unwrap();
        let _flock = crate::common::db_rename_flock();
        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state.clone()).await;

        let tenant_id = crate::common::create_test_tenant(state.pool(), "WOFF NoId").await;
        let encrypted = encrypt_test_secret("dummy", crate::common::TEST_JWT_SECRET);
        let domain = format!("woff-noid-{}", Uuid::new_v4().simple());
        insert_sso_config(state.pool(), tenant_id, &domain, &encrypted, None).await;

        let client = reqwest::Client::new();
        let res = client
            .get(format!("{base_url}/api/auth/woff-config?domain={domain}"))
            .send()
            .await
            .unwrap();

        assert_eq!(res.status(), 404, "No woff_id should return 404");
    });
}

// ============================================================
// F: woff_auth happy path (lines 715-768, 774-806)
// ============================================================

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_woff_auth_happy_path() {
    test_group!("auth カバレッジ");
    test_case!("wiremock で profile API をモック → 200 + token", {
        let _db = crate::common::DB_RENAME_LOCK.lock().unwrap();
        let _flock = crate::common::db_rename_flock();
        let _env = crate::common::ENV_LOCK.lock().unwrap();
        let mock_server = MockServer::start().await;

        // USERINFO_URL mock
        Mock::given(method("GET"))
            .and(path("/v1.0/users/me"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "userId": "woff-user-unique-001",
                "userName": {
                    "lastName": "WOFF",
                    "firstName": "テスト"
                },
                "email": "woff@test.example.com"
            })))
            .mount(&mock_server)
            .await;

        std::env::set_var(
            "LINEWORKS_USERINFO_URL",
            format!("{}/v1.0/users/me", mock_server.uri()),
        );

        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state.clone()).await;

        // 前回テストの残存ユーザーを削除
        sqlx::query("DELETE FROM users WHERE lineworks_id = 'woff-user-unique-001'")
            .execute(state.pool())
            .await
            .unwrap();

        let tenant_id = crate::common::create_test_tenant(state.pool(), "WOFF Auth").await;
        let encrypted = encrypt_test_secret("dummy", crate::common::TEST_JWT_SECRET);
        let domain = format!("woff-auth-{}", Uuid::new_v4().simple());
        insert_sso_config(
            state.pool(),
            tenant_id,
            &domain,
            &encrypted,
            Some("woff-id"),
        )
        .await;

        let client = reqwest::Client::new();
        let res = client
            .post(format!("{base_url}/api/auth/woff"))
            .json(&serde_json::json!({
                "access_token": "mock-woff-access-token",
                "domain_id": domain
            }))
            .send()
            .await
            .unwrap();

        assert_eq!(res.status(), 200, "WOFF auth should succeed");
        let body: Value = res.json().await.unwrap();
        assert!(body["token"].as_str().is_some(), "Should have token");
        assert!(
            body["expiresAt"].as_str().is_some(),
            "Should have expiresAt"
        );
        assert!(body["tenantId"].as_str().is_some(), "Should have tenantId");

        // Cleanup env
        std::env::remove_var("LINEWORKS_USERINFO_URL");
    });
}

// ============================================================
// Error paths: OAUTH_STATE_SECRET not set (lines 471-473)
// ============================================================

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_lineworks_redirect_no_oauth_secret() {
    test_group!("auth カバレッジ");
    test_case!("OAUTH_STATE_SECRET 未設定で 500", {
        let _db = crate::common::DB_RENAME_LOCK.lock().unwrap();
        let _flock = crate::common::db_rename_flock();
        let _env = crate::common::ENV_LOCK.lock().unwrap();
        std::env::remove_var("OAUTH_STATE_SECRET");

        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state.clone()).await;

        let tenant_id = crate::common::create_test_tenant(state.pool(), "LW NoSecret").await;
        let encrypted = encrypt_test_secret("dummy", crate::common::TEST_JWT_SECRET);
        let domain = format!("nosecret-{}", Uuid::new_v4().simple());
        insert_sso_config(state.pool(), tenant_id, &domain, &encrypted, None).await;

        let client = reqwest::ClientBuilder::new()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();

        let res = client
            .get(format!(
                "{base_url}/api/auth/lineworks/redirect?domain={domain}&redirect_uri=https://example.com/cb"
            ))
            .send()
            .await
            .unwrap();
        assert_eq!(
            res.status(),
            500,
            "Missing OAUTH_STATE_SECRET should return 500"
        );
    });
}

// ============================================================
// Error paths: lineworks_callback SSO lookup error (lines 566-568)
// ============================================================

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_lineworks_callback_sso_lookup_error() {
    test_group!("auth カバレッジ");
    test_case!("callback SSO lookup → 存在しない org → 500", {
        let _db = crate::common::DB_RENAME_LOCK.lock().unwrap();
        let _flock = crate::common::db_rename_flock();
        let _env = crate::common::ENV_LOCK.lock().unwrap();
        std::env::set_var("OAUTH_STATE_SECRET", "test-oauth-state-secret-dberr2");
        std::env::set_var("API_ORIGIN", "http://localhost:9999");

        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state.clone()).await;

        // 有効な state を生成 (external_org_id は存在しない → fetch_one エラー)
        let state_payload = rust_alc_api::auth::lineworks::state::StatePayload {
            redirect_uri: "https://example.com/login".into(),
            nonce: Uuid::new_v4().to_string(),
            provider: "lineworks".into(),
            external_org_id: "nonexistent-org-for-db-error".into(),
        };
        let signed_state = rust_alc_api::auth::lineworks::state::sign(
            &state_payload,
            "test-oauth-state-secret-dberr2",
        );

        let client = reqwest::ClientBuilder::new()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();

        let res = client
            .get(format!(
                "{base_url}/api/auth/lineworks/callback?code=any&state={signed_state}"
            ))
            .send()
            .await
            .unwrap();

        assert_eq!(
            res.status(),
            500,
            "Missing SSO config → fetch_one error → 500"
        );
    });
}

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_lineworks_callback_decrypt_error() {
    test_group!("auth カバレッジ");
    test_case!("callback 不正な暗号化シークレット → 500", {
        let _db = crate::common::DB_RENAME_LOCK.lock().unwrap();
        let _flock = crate::common::db_rename_flock();
        let _env = crate::common::ENV_LOCK.lock().unwrap();
        std::env::set_var("OAUTH_STATE_SECRET", "test-oauth-state-secret-decrypt");
        std::env::set_var("API_ORIGIN", "http://localhost:9999");

        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state.clone()).await;

        let tenant_id = crate::common::create_test_tenant(state.pool(), "LW Decrypt Err").await;
        let ext_org_id = format!("decrypt-err-{}", Uuid::new_v4().simple());
        // 不正な暗号文を挿入
        insert_sso_config(
            state.pool(),
            tenant_id,
            &ext_org_id,
            "invalid-base64-not-encrypted",
            None,
        )
        .await;

        let state_payload = rust_alc_api::auth::lineworks::state::StatePayload {
            redirect_uri: "https://example.com/login".into(),
            nonce: Uuid::new_v4().to_string(),
            provider: "lineworks".into(),
            external_org_id: ext_org_id,
        };
        let signed_state = rust_alc_api::auth::lineworks::state::sign(
            &state_payload,
            "test-oauth-state-secret-decrypt",
        );

        let client = reqwest::ClientBuilder::new()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();

        let res = client
            .get(format!(
                "{base_url}/api/auth/lineworks/callback?code=any&state={signed_state}"
            ))
            .send()
            .await
            .unwrap();

        assert_eq!(
            res.status(),
            500,
            "Invalid encrypted secret → decryption error → 500"
        );
    });
}

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_lineworks_callback_token_exchange_error() {
    test_group!("auth カバレッジ");
    test_case!("callback token exchange 失敗 → 502", {
        let _db = crate::common::DB_RENAME_LOCK.lock().unwrap();
        let _flock = crate::common::db_rename_flock();
        let _env = crate::common::ENV_LOCK.lock().unwrap();
        let mock_server = MockServer::start().await;

        // TOKEN_URL mock → 500 error
        Mock::given(method("POST"))
            .and(path("/oauth2/v2.0/token"))
            .respond_with(ResponseTemplate::new(500).set_body_string("Internal Server Error"))
            .mount(&mock_server)
            .await;

        std::env::set_var(
            "LINEWORKS_TOKEN_URL",
            format!("{}/oauth2/v2.0/token", mock_server.uri()),
        );
        std::env::set_var("OAUTH_STATE_SECRET", "test-oauth-state-secret-texch");
        std::env::set_var("API_ORIGIN", "http://localhost:9999");

        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state.clone()).await;

        let tenant_id = crate::common::create_test_tenant(state.pool(), "LW TokExch Err").await;
        let encrypted = encrypt_test_secret("test-secret", crate::common::TEST_JWT_SECRET);
        let ext_org_id = format!("texch-err-{}", Uuid::new_v4().simple());
        insert_sso_config(state.pool(), tenant_id, &ext_org_id, &encrypted, None).await;

        let state_payload = rust_alc_api::auth::lineworks::state::StatePayload {
            redirect_uri: "https://example.com/login".into(),
            nonce: Uuid::new_v4().to_string(),
            provider: "lineworks".into(),
            external_org_id: ext_org_id,
        };
        let signed_state = rust_alc_api::auth::lineworks::state::sign(
            &state_payload,
            "test-oauth-state-secret-texch",
        );

        let client = reqwest::ClientBuilder::new()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();

        let res = client
            .get(format!(
                "{base_url}/api/auth/lineworks/callback?code=bad-code&state={signed_state}"
            ))
            .send()
            .await
            .unwrap();

        assert_eq!(res.status(), 502, "Token exchange failure → 502");

        std::env::remove_var("LINEWORKS_TOKEN_URL");
    });
}

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_lineworks_callback_profile_error() {
    test_group!("auth カバレッジ");
    test_case!("callback user profile 取得失敗 → 502", {
        let _db = crate::common::DB_RENAME_LOCK.lock().unwrap();
        let _flock = crate::common::db_rename_flock();
        let _env = crate::common::ENV_LOCK.lock().unwrap();
        let mock_server = MockServer::start().await;

        // TOKEN_URL mock → success
        Mock::given(method("POST"))
            .and(path("/oauth2/v2.0/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "mock-token",
                "token_type": "Bearer",
                "expires_in": 3600
            })))
            .mount(&mock_server)
            .await;

        // USERINFO_URL mock → 401 error
        Mock::given(method("GET"))
            .and(path("/v1.0/users/me"))
            .respond_with(ResponseTemplate::new(401).set_body_string("Unauthorized"))
            .mount(&mock_server)
            .await;

        std::env::set_var(
            "LINEWORKS_TOKEN_URL",
            format!("{}/oauth2/v2.0/token", mock_server.uri()),
        );
        std::env::set_var(
            "LINEWORKS_USERINFO_URL",
            format!("{}/v1.0/users/me", mock_server.uri()),
        );
        std::env::set_var("OAUTH_STATE_SECRET", "test-oauth-state-secret-prof");
        std::env::set_var("API_ORIGIN", "http://localhost:9999");

        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state.clone()).await;

        let tenant_id = crate::common::create_test_tenant(state.pool(), "LW Prof Err").await;
        let encrypted = encrypt_test_secret("test-secret", crate::common::TEST_JWT_SECRET);
        let ext_org_id = format!("prof-err-{}", Uuid::new_v4().simple());
        insert_sso_config(state.pool(), tenant_id, &ext_org_id, &encrypted, None).await;

        let state_payload = rust_alc_api::auth::lineworks::state::StatePayload {
            redirect_uri: "https://example.com/login".into(),
            nonce: Uuid::new_v4().to_string(),
            provider: "lineworks".into(),
            external_org_id: ext_org_id,
        };
        let signed_state = rust_alc_api::auth::lineworks::state::sign(
            &state_payload,
            "test-oauth-state-secret-prof",
        );

        let client = reqwest::ClientBuilder::new()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();

        let res = client
            .get(format!(
                "{base_url}/api/auth/lineworks/callback?code=some-code&state={signed_state}"
            ))
            .send()
            .await
            .unwrap();

        assert_eq!(res.status(), 502, "Profile fetch failure → 502");

        std::env::remove_var("LINEWORKS_TOKEN_URL");
        std::env::remove_var("LINEWORKS_USERINFO_URL");
    });
}

// ============================================================
// Error path: woff_auth profile error (lines 731-733)
// ============================================================

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_woff_auth_profile_error() {
    test_group!("auth カバレッジ");
    test_case!("woff_auth profile 取得失敗 → 401", {
        let _db = crate::common::DB_RENAME_LOCK.lock().unwrap();
        let _flock = crate::common::db_rename_flock();
        let _env = crate::common::ENV_LOCK.lock().unwrap();
        let mock_server = MockServer::start().await;

        // USERINFO_URL mock → 401
        Mock::given(method("GET"))
            .and(path("/v1.0/users/me"))
            .respond_with(ResponseTemplate::new(401).set_body_string("Unauthorized"))
            .mount(&mock_server)
            .await;

        std::env::set_var(
            "LINEWORKS_USERINFO_URL",
            format!("{}/v1.0/users/me", mock_server.uri()),
        );

        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state.clone()).await;

        let tenant_id = crate::common::create_test_tenant(state.pool(), "WOFF Prof Err").await;
        let encrypted = encrypt_test_secret("dummy", crate::common::TEST_JWT_SECRET);
        let domain = format!("woff-proferr-{}", Uuid::new_v4().simple());
        insert_sso_config(
            state.pool(),
            tenant_id,
            &domain,
            &encrypted,
            Some("woff-id"),
        )
        .await;

        let client = reqwest::Client::new();
        let res = client
            .post(format!("{base_url}/api/auth/woff"))
            .json(&serde_json::json!({
                "access_token": "invalid-woff-token",
                "domain_id": domain
            }))
            .send()
            .await
            .unwrap();

        assert_eq!(res.status(), 401, "Profile fetch failure → 401");

        std::env::remove_var("LINEWORKS_USERINFO_URL");
    });
}

// ============================================================
// 既存ユーザーパス (line 790): 2回目の woff_auth で既存ユーザー検出
// ============================================================

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_woff_auth_existing_user() {
    test_group!("auth カバレッジ");
    test_case!(
        "woff_auth 2回呼び → 2回目は既存ユーザー検出",
        {
            let _db = crate::common::DB_RENAME_LOCK.lock().unwrap();
            let _flock = crate::common::db_rename_flock();
            let _env = crate::common::ENV_LOCK.lock().unwrap();
            let mock_server = MockServer::start().await;

            Mock::given(method("GET"))
                .and(path("/v1.0/users/me"))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "userId": "woff-existing-user-002",
                    "userName": { "lastName": "既存", "firstName": "ユーザー" },
                    "email": "existing@test.example.com"
                })))
                .mount(&mock_server)
                .await;

            std::env::set_var(
                "LINEWORKS_USERINFO_URL",
                format!("{}/v1.0/users/me", mock_server.uri()),
            );

            let state = crate::common::setup_app_state().await;
            let base_url = crate::common::spawn_test_server(state.clone()).await;

            // 前回の残存ユーザーを削除
            sqlx::query("DELETE FROM users WHERE lineworks_id = 'woff-existing-user-002'")
                .execute(state.pool())
                .await
                .unwrap();

            let tenant_id = crate::common::create_test_tenant(state.pool(), "WOFF Exist").await;
            let encrypted = encrypt_test_secret("dummy", crate::common::TEST_JWT_SECRET);
            let domain = format!("woff-exist-{}", Uuid::new_v4().simple());
            insert_sso_config(
                state.pool(),
                tenant_id,
                &domain,
                &encrypted,
                Some("woff-id"),
            )
            .await;

            let client = reqwest::Client::new();
            let body = serde_json::json!({
                "access_token": "mock-woff-token",
                "domain_id": domain
            });

            // 1回目: 新規ユーザー作成
            let res = client
                .post(format!("{base_url}/api/auth/woff"))
                .json(&body)
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200, "First call should succeed");

            // 2回目: 既存ユーザー検出 (line 790 カバー)
            let res = client
                .post(format!("{base_url}/api/auth/woff"))
                .json(&body)
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200, "Second call should find existing user");

            std::env::remove_var("LINEWORKS_USERINFO_URL");
        }
    );
}

// ============================================================
// DB query error: resolve_sso_config 関数を DROP → エラー (lines 496-498, 681-683, 721-723)
// ============================================================

/// resolve_sso_config 関数の再作成 SQL
const RECREATE_RESOLVE_SSO_CONFIG: &str = r#"
CREATE OR REPLACE FUNCTION alc_api.resolve_sso_config(
    p_provider TEXT, p_lookup_key TEXT
) RETURNS TABLE (
    tenant_id UUID, client_id TEXT, client_secret_encrypted TEXT,
    external_org_id TEXT, woff_id TEXT
) AS $$
BEGIN
    RETURN QUERY
    SELECT sso.tenant_id, sso.client_id, sso.client_secret_encrypted,
           sso.external_org_id, sso.woff_id
    FROM alc_api.sso_provider_configs sso
    WHERE sso.provider = p_provider AND sso.external_org_id = p_lookup_key
      AND sso.enabled = true LIMIT 1;
END;
$$ LANGUAGE plpgsql SECURITY DEFINER SET search_path = alc_api
"#;

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_sso_query_error_lineworks_redirect() {
    test_group!("auth カバレッジ");
    test_case!("resolve_sso_config DROP → lineworks redirect 500", {
        let _db = crate::common::DB_RENAME_LOCK.lock().unwrap();
        let _flock = crate::common::db_rename_flock();
        let _env = crate::common::ENV_LOCK.lock().unwrap();
        std::env::set_var("OAUTH_STATE_SECRET", "test-oauth-state-secret-drop1");

        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state.clone()).await;

        // 関数を DROP (コミット済み)
        sqlx::query("DROP FUNCTION IF EXISTS alc_api.resolve_sso_config(TEXT, TEXT)")
            .execute(state.pool())
            .await
            .unwrap();

        let client = reqwest::ClientBuilder::new()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();

        let res = client
            .get(format!(
                "{base_url}/api/auth/lineworks/redirect?domain=any&redirect_uri=https://example.com/cb"
            ))
            .send()
            .await
            .unwrap();

        assert_eq!(res.status(), 500, "Dropped function → 500");

        // 再作成
        sqlx::query(RECREATE_RESOLVE_SSO_CONFIG)
            .execute(state.pool())
            .await
            .unwrap();
    });
}

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_sso_query_error_woff_config() {
    test_group!("auth カバレッジ");
    test_case!("resolve_sso_config DROP → woff-config 500", {
        let _db = crate::common::DB_RENAME_LOCK.lock().unwrap();
        let _flock = crate::common::db_rename_flock();
        let _env = crate::common::ENV_LOCK.lock().unwrap();
        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state.clone()).await;

        sqlx::query("DROP FUNCTION IF EXISTS alc_api.resolve_sso_config(TEXT, TEXT)")
            .execute(state.pool())
            .await
            .unwrap();

        let client = reqwest::Client::new();
        let res = client
            .get(format!("{base_url}/api/auth/woff-config?domain=any"))
            .send()
            .await
            .unwrap();

        assert_eq!(res.status(), 500, "Dropped function → 500");

        sqlx::query(RECREATE_RESOLVE_SSO_CONFIG)
            .execute(state.pool())
            .await
            .unwrap();
    });
}

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_sso_query_error_woff_auth() {
    test_group!("auth カバレッジ");
    test_case!("resolve_sso_config DROP → woff auth 500", {
        let _db = crate::common::DB_RENAME_LOCK.lock().unwrap();
        let _flock = crate::common::db_rename_flock();
        let _env = crate::common::ENV_LOCK.lock().unwrap();
        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state.clone()).await;

        sqlx::query("DROP FUNCTION IF EXISTS alc_api.resolve_sso_config(TEXT, TEXT)")
            .execute(state.pool())
            .await
            .unwrap();

        let client = reqwest::Client::new();
        let res = client
            .post(format!("{base_url}/api/auth/woff"))
            .json(&serde_json::json!({
                "access_token": "any",
                "domain_id": "any"
            }))
            .send()
            .await
            .unwrap();

        assert_eq!(res.status(), 500, "Dropped function → 500");

        sqlx::query(RECREATE_RESOLVE_SSO_CONFIG)
            .execute(state.pool())
            .await
            .unwrap();
    });
}

// ============================================================
// User INSERT error (lines 802-804): lineworks_id に CHECK 制約違反
// ============================================================

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_upsert_lineworks_user_insert_error() {
    test_group!("auth カバレッジ");
    test_case!("user INSERT 失敗 → 500", {
        let _db = crate::common::DB_RENAME_LOCK.lock().unwrap();
        let _flock = crate::common::db_rename_flock();
        let _env = crate::common::ENV_LOCK.lock().unwrap();
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/oauth2/v2.0/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "mock-token", "token_type": "Bearer", "expires_in": 3600
            })))
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path("/v1.0/users/me"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "userId": "insert-err-user-001",
                "email": "insert-err@test.example.com"
            })))
            .mount(&mock_server)
            .await;

        std::env::set_var(
            "LINEWORKS_TOKEN_URL",
            format!("{}/oauth2/v2.0/token", mock_server.uri()),
        );
        std::env::set_var(
            "LINEWORKS_USERINFO_URL",
            format!("{}/v1.0/users/me", mock_server.uri()),
        );
        std::env::set_var("OAUTH_STATE_SECRET", "test-oauth-state-secret-inserr");
        std::env::set_var("API_ORIGIN", "http://localhost:9999");

        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state.clone()).await;

        // 前回の残存ユーザーを削除
        sqlx::query("DELETE FROM users WHERE lineworks_id = 'insert-err-user-001'")
            .execute(state.pool())
            .await
            .unwrap();

        // SSO config の tenant_id に存在しない UUID を使う → FK 違反で INSERT 失敗
        let fake_tenant_id = Uuid::new_v4();
        let ext_org_id = format!("inserr-{}", Uuid::new_v4().simple());
        sqlx::query(
            r#"INSERT INTO sso_provider_configs (tenant_id, provider, client_id, client_secret_encrypted, external_org_id)
               VALUES ($1, 'lineworks', $2, $3, $4)
               ON CONFLICT (provider, external_org_id) DO UPDATE SET tenant_id = $1"#,
        )
        .bind(fake_tenant_id)
        .bind(format!("cli-{}", Uuid::new_v4().simple()))
        .bind(&encrypt_test_secret("test", crate::common::TEST_JWT_SECRET))
        .bind(&ext_org_id)
        .execute(state.pool())
        .await
        .unwrap();

        let state_payload = rust_alc_api::auth::lineworks::state::StatePayload {
            redirect_uri: "https://example.com/login".into(),
            nonce: Uuid::new_v4().to_string(),
            provider: "lineworks".into(),
            external_org_id: ext_org_id,
        };
        let signed_state = rust_alc_api::auth::lineworks::state::sign(
            &state_payload,
            "test-oauth-state-secret-inserr",
        );

        let client = reqwest::ClientBuilder::new()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();

        let res = client
            .get(format!(
                "{base_url}/api/auth/lineworks/callback?code=ok&state={signed_state}"
            ))
            .send()
            .await
            .unwrap();

        assert_eq!(res.status(), 500, "FK violation on user INSERT → 500");

        // Cleanup
        sqlx::query("DELETE FROM sso_provider_configs WHERE external_org_id = $1")
            .bind(&state_payload.external_org_id)
            .execute(state.pool())
            .await
            .unwrap();
        std::env::remove_var("LINEWORKS_TOKEN_URL");
        std::env::remove_var("LINEWORKS_USERINFO_URL");
    });
}
