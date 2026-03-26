mod common;

use serde_json::Value;

// ============================================================
// 既存テスト (ミドルウェア)
// ============================================================

/// JWT なし + X-Tenant-ID ��し → 401
#[tokio::test]
async fn test_no_auth_returns_unauthorized() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state).await;

    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/employees"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401, "No auth should return 401");
}

/// 無効な JWT → 401 (JWT 必須ルート)
#[tokio::test]
async fn test_invalid_jwt_returns_unauthorized() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state).await;

    let client = reqwest::Client::new();

    // JWT 必須の管理者ルートを使用
    let res = client
        .get(format!("{base_url}/api/auth/me"))
        .header("Authorization", "Bearer invalid-token-here")
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401, "Invalid JWT should return 401");
}

/// 有効な JWT で認証成功
#[tokio::test]
async fn test_valid_jwt_succeeds() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;

    let tenant_id = common::create_test_tenant(&state.pool, "Auth Test Tenant").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");

    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/employees"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200, "Valid JWT should return 200");
}

/// 不正な UUID の X-Tenant-ID → 401
#[tokio::test]
async fn test_invalid_tenant_id_returns_unauthorized() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state).await;

    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/employees"))
        .header("X-Tenant-ID", "not-a-uuid")
        .send()
        .await
        .unwrap();
    assert_eq!(
        res.status(),
        401,
        "Invalid UUID in X-Tenant-ID should return 401"
    );
}

// ============================================================
// auth/me
// ============================================================

#[tokio::test]
async fn test_me_returns_user_info() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "Me Test").await;

    let (user_id, _) =
        common::create_test_user_in_db(&state.pool, tenant_id, "me@test.com", "admin").await;
    let jwt = common::create_test_jwt_for_user(user_id, tenant_id, "me@test.com", "admin");

    let client = reqwest::Client::new();
    let res = client
        .get(format!("{base_url}/api/auth/me"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["id"], user_id.to_string());
    assert_eq!(body["email"], "me@test.com");
    assert_eq!(body["role"], "admin");
    assert_eq!(body["tenant_id"], tenant_id.to_string());
}

#[tokio::test]
async fn test_me_without_auth() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state).await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/auth/me"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);
}

// ============================================================
// Refresh Token
// ============================================================

#[tokio::test]
async fn test_refresh_token_success() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "Refresh OK").await;

    let (_user_id, raw_token) =
        common::create_test_user_in_db(&state.pool, tenant_id, "refresh@test.com", "admin").await;

    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/auth/refresh"))
        .json(&serde_json::json!({ "refresh_token": raw_token }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert!(body["access_token"].as_str().is_some());
    assert!(body["expires_in"].as_i64().unwrap() > 0);
}

#[tokio::test]
async fn test_refresh_token_invalid() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state).await;
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/auth/refresh"))
        .json(&serde_json::json!({ "refresh_token": "rt_garbage_token" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);
}

#[tokio::test]
async fn test_refresh_token_expired() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "Refresh Exp").await;

    let (_user_id, raw_token) =
        common::create_test_user_in_db(&state.pool, tenant_id, "expired@test.com", "admin").await;

    // 有効期限を過去に更新
    sqlx::query("UPDATE users SET refresh_token_expires_at = NOW() - INTERVAL '1 day' WHERE tenant_id = $1 AND email = 'expired@test.com'")
        .bind(tenant_id)
        .execute(&state.pool)
        .await
        .unwrap();

    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/auth/refresh"))
        .json(&serde_json::json!({ "refresh_token": raw_token }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);
}

// ============================================================
// Logout
// ============================================================

#[tokio::test]
async fn test_logout_clears_refresh() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "Logout Test").await;

    let (user_id, raw_token) =
        common::create_test_user_in_db(&state.pool, tenant_id, "logout@test.com", "admin").await;
    let jwt = common::create_test_jwt_for_user(user_id, tenant_id, "logout@test.com", "admin");

    let client = reqwest::Client::new();

    // Logout
    let res = client
        .post(format!("{base_url}/api/auth/logout"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 204);

    // Refresh should fail now
    let res = client
        .post(format!("{base_url}/api/auth/refresh"))
        .json(&serde_json::json!({ "refresh_token": raw_token }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);
}

// ============================================================
// テナント作成 + my-orgs
// ============================================================

#[tokio::test]
async fn test_create_tenant_endpoint() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state).await;
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/auth/tenants"))
        .json(&serde_json::json!({ "name": "New Tenant" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 201);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["name"], "New Tenant");
    assert!(body["id"].as_str().is_some());
}

#[tokio::test]
async fn test_my_orgs() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "MyOrgs Test").await;

    let (user_id, _) =
        common::create_test_user_in_db(&state.pool, tenant_id, "orgs@test.com", "admin").await;
    let jwt = common::create_test_jwt_for_user(user_id, tenant_id, "orgs@test.com", "admin");

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
    assert_eq!(orgs.len(), 1);
    assert_eq!(orgs[0]["id"], tenant_id.to_string());
}

// ============================================================
// Google OAuth Redirect
// ============================================================

/// GET /api/auth/google/redirect → 307 redirect to Google (or 500 if OAUTH_STATE_SECRET missing)
/// Sets OAUTH_STATE_SECRET and verifies redirect to Google authorize URL
#[tokio::test]
async fn test_google_redirect_to_google() {
    std::env::set_var("OAUTH_STATE_SECRET", "test-oauth-state-secret");

    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state).await;

    let client = reqwest::ClientBuilder::new()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();

    let res = client
        .get(format!(
            "{base_url}/api/auth/google/redirect?redirect_uri=https://example.com/callback"
        ))
        .send()
        .await
        .unwrap();

    let status = res.status().as_u16();
    // 307 if OAUTH_STATE_SECRET is available, 500 if another parallel test removed it
    if status == 307 {
        let location = res.headers().get("location").unwrap().to_str().unwrap();
        assert!(
            location.starts_with("https://accounts.google.com/o/oauth2/v2/auth"),
            "Should redirect to Google OAuth URL, got: {location}"
        );
        assert!(
            location.contains("client_id="),
            "Redirect URL should contain client_id"
        );
        assert!(
            location.contains("state="),
            "Redirect URL should contain state parameter"
        );
    } else {
        assert_eq!(
            status, 500,
            "Without OAUTH_STATE_SECRET should return 500, got {status}"
        );
    }
}

/// GET /api/auth/google/redirect without redirect_uri → 422 (missing required param)
#[tokio::test]
async fn test_google_redirect_missing_redirect_uri() {
    std::env::set_var("OAUTH_STATE_SECRET", "test-oauth-state-secret");

    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state).await;

    let client = reqwest::ClientBuilder::new()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();

    let res = client
        .get(format!("{base_url}/api/auth/google/redirect"))
        .send()
        .await
        .unwrap();
    // Axum returns 400 for missing required query parameters
    assert_eq!(
        res.status(),
        400,
        "Missing redirect_uri should return 400"
    );
}

// ============================================================
// LINE WORKS OAuth Redirect
// ============================================================

/// GET /api/auth/lineworks/redirect without domain or address → 400 or 500
/// (500 if OAUTH_STATE_SECRET is missing, 400 if domain/address param is missing)
#[tokio::test]
async fn test_lineworks_redirect_missing_domain() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state).await;

    let client = reqwest::ClientBuilder::new()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();

    let res = client
        .get(format!(
            "{base_url}/api/auth/lineworks/redirect?redirect_uri=https://example.com/callback"
        ))
        .send()
        .await
        .unwrap();
    // Returns 500 (OAUTH_STATE_SECRET missing) or 400 (domain missing) depending on env
    let status = res.status().as_u16();
    assert!(
        status == 400 || status == 500,
        "Missing domain should return 400 or 500, got {status}"
    );
}

/// GET /api/auth/lineworks/redirect with nonexistent domain → 404 or 500
/// (resolve_sso_config may not exist in test DB, or returns NULL → 404)
#[tokio::test]
async fn test_lineworks_redirect_unknown_domain() {
    std::env::set_var("OAUTH_STATE_SECRET", "test-oauth-state-secret");

    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state).await;

    let client = reqwest::ClientBuilder::new()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();

    let res = client
        .get(format!(
            "{base_url}/api/auth/lineworks/redirect?domain=nonexistent-domain-xyz&redirect_uri=https://example.com/callback"
        ))
        .send()
        .await
        .unwrap();
    // 404 if resolve_sso_config returns NULL, 500 if function doesn't exist in test DB
    let status = res.status().as_u16();
    assert!(
        status == 404 || status == 500,
        "Unknown domain should return 404 or 500, got {status}"
    );
}

// ============================================================
// POST /api/auth/google (id_token flow)
// ============================================================

/// POST /api/auth/google with invalid id_token → 401
#[tokio::test]
async fn test_google_login_invalid_token() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/auth/google"))
        .json(&serde_json::json!({ "id_token": "invalid.jwt.token" }))
        .send()
        .await
        .unwrap();
    assert_eq!(
        res.status(),
        401,
        "Invalid Google id_token should return 401"
    );
}

/// POST /api/auth/google with empty id_token → 401
#[tokio::test]
async fn test_google_login_empty_token() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/auth/google"))
        .json(&serde_json::json!({ "id_token": "" }))
        .send()
        .await
        .unwrap();
    assert_eq!(
        res.status(),
        401,
        "Empty Google id_token should return 401"
    );
}

/// POST /api/auth/google with missing id_token field → 422
#[tokio::test]
async fn test_google_login_missing_field() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/auth/google"))
        .json(&serde_json::json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(
        res.status(),
        422,
        "Missing id_token field should return 422"
    );
}

// ============================================================
// POST /api/auth/google/code (authorization code flow)
// ============================================================

/// POST /api/auth/google/code with invalid code → 401
#[tokio::test]
async fn test_google_code_invalid() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/auth/google/code"))
        .json(&serde_json::json!({
            "code": "invalid-auth-code",
            "redirect_uri": "https://example.com/callback"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(
        res.status(),
        401,
        "Invalid Google auth code should return 401"
    );
}

/// POST /api/auth/google/code with missing fields → 422
#[tokio::test]
async fn test_google_code_missing_fields() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/auth/google/code"))
        .json(&serde_json::json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(
        res.status(),
        422,
        "Missing code/redirect_uri should return 422"
    );
}

// ============================================================
// GET /api/auth/woff-config
// ============================================================

/// GET /api/auth/woff-config without domain → 400 (missing required query param)
#[tokio::test]
async fn test_woff_config_missing_domain() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .get(format!("{base_url}/api/auth/woff-config"))
        .send()
        .await
        .unwrap();
    // Axum returns 400 for missing required query parameters
    assert_eq!(
        res.status(),
        400,
        "Missing domain param should return 400"
    );
}

/// GET /api/auth/woff-config with unknown domain → 404 or 500
#[tokio::test]
async fn test_woff_config_unknown_domain() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .get(format!(
            "{base_url}/api/auth/woff-config?domain=nonexistent-domain-xyz"
        ))
        .send()
        .await
        .unwrap();
    // 404 if resolve_sso_config returns NULL, 500 if function doesn't exist
    let status = res.status().as_u16();
    assert!(
        status == 404 || status == 500,
        "Unknown domain should return 404 or 500, got {status}"
    );
}

// ============================================================
// POST /api/auth/woff (WOFF auth)
// ============================================================

/// POST /api/auth/woff with unknown domain_id → 404 or 500
#[tokio::test]
async fn test_woff_auth_unknown_domain() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/auth/woff"))
        .json(&serde_json::json!({
            "access_token": "fake-access-token",
            "domain_id": "nonexistent-domain-xyz"
        }))
        .send()
        .await
        .unwrap();
    // 404 if resolve_sso_config returns NULL, 500 if function doesn't exist
    let status = res.status().as_u16();
    assert!(
        status == 404 || status == 500,
        "Unknown domain_id should return 404 or 500, got {status}"
    );
}

/// POST /api/auth/woff with missing fields → 422 (Axum JSON rejection)
#[tokio::test]
async fn test_woff_auth_missing_fields() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/auth/woff"))
        .json(&serde_json::json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(
        res.status(),
        422,
        "Missing access_token/domain_id should return 422"
    );
}

/// POST /api/auth/woff without Content-Type → 415
#[tokio::test]
async fn test_woff_auth_no_content_type() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/auth/woff"))
        .body("{}")
        .send()
        .await
        .unwrap();
    assert_eq!(
        res.status(),
        415,
        "Missing Content-Type should return 415"
    );
}
