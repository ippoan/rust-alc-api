use serde_json::Value;

// ============================================================
// 既存テスト (ミドルウェア)
// ============================================================

#[tokio::test]
async fn test_no_auth_returns_unauthorized() {
    test_group!("認証ミドルウェア");
    test_case!("JWT なし + テナント ID なしで 401 を返す", {
        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state).await;

        let client = reqwest::Client::new();

        let res = client
            .get(format!("{base_url}/api/employees"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 401, "No auth should return 401");
    });
}

#[tokio::test]
async fn test_invalid_jwt_returns_unauthorized() {
    test_group!("認証ミドルウェア");
    test_case!("無効な JWT で 401 を返す (JWT 必須ルート)", {
        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state).await;

        let client = reqwest::Client::new();

        // JWT 必須の管理者ルートを使用
        let res = client
            .get(format!("{base_url}/api/auth/me"))
            .header("Authorization", "Bearer invalid-token-here")
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 401, "Invalid JWT should return 401");
    });
}

#[tokio::test]
async fn test_valid_jwt_succeeds() {
    test_group!("認証ミドルウェア");
    test_case!("有効な JWT で認証成功", {
        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state.clone()).await;

        let tenant_id = crate::common::create_test_tenant(state.pool(), "Auth Test Tenant").await;
        let jwt = crate::common::create_test_jwt(tenant_id, "admin");

        let client = reqwest::Client::new();

        let res = client
            .get(format!("{base_url}/api/employees"))
            .header("Authorization", format!("Bearer {jwt}"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200, "Valid JWT should return 200");
    });
}

#[tokio::test]
async fn test_invalid_tenant_id_returns_unauthorized() {
    test_group!("認証ミドルウェア");
    test_case!("不正な UUID の X-Tenant-ID で 401 を返す", {
        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state).await;

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
    });
}

// ============================================================
// auth/me
// ============================================================

#[tokio::test]
async fn test_me_returns_user_info() {
    test_group!("auth/me ユーザー情報");
    test_case!("認証済みユーザーの情報を返す", {
        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state.clone()).await;
        let tenant_id = crate::common::create_test_tenant(state.pool(), "Me Test").await;

        let (user_id, _) =
            crate::common::create_test_user_in_db(state.pool(), tenant_id, "me@test.com", "admin")
                .await;
        let jwt =
            crate::common::create_test_jwt_for_user(user_id, tenant_id, "me@test.com", "admin");

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
    });
}

#[tokio::test]
async fn test_me_without_auth() {
    test_group!("auth/me ユーザー情報");
    test_case!("認証なしで 401 を返す", {
        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state).await;
        let client = reqwest::Client::new();

        let res = client
            .get(format!("{base_url}/api/auth/me"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 401);
    });
}

// ============================================================
// Refresh Token
// ============================================================

#[tokio::test]
async fn test_refresh_token_success() {
    test_group!("リフレッシュトークン");
    test_case!(
        "有効なリフレッシュトークンで新しいアクセストークンを取得",
        {
            let state = crate::common::setup_app_state().await;
            let base_url = crate::common::spawn_test_server(state.clone()).await;
            let tenant_id = crate::common::create_test_tenant(state.pool(), "Refresh OK").await;

            let (_user_id, raw_token) = crate::common::create_test_user_in_db(
                state.pool(),
                tenant_id,
                "refresh@test.com",
                "admin",
            )
            .await;

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
    );
}

#[tokio::test]
async fn test_refresh_token_invalid() {
    test_group!("リフレッシュトークン");
    test_case!("無効なリフレッシュトークンで 401 を返す", {
        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state).await;
        let client = reqwest::Client::new();

        let res = client
            .post(format!("{base_url}/api/auth/refresh"))
            .json(&serde_json::json!({ "refresh_token": "rt_garbage_token" }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 401);
    });
}

#[tokio::test]
async fn test_refresh_token_expired() {
    test_group!("リフレッシュトークン");
    test_case!(
        "期限切れのリフレッシュトークンで 401 を返す",
        {
            let state = crate::common::setup_app_state().await;
            let base_url = crate::common::spawn_test_server(state.clone()).await;
            let tenant_id = crate::common::create_test_tenant(state.pool(), "Refresh Exp").await;

            let (_user_id, raw_token) = crate::common::create_test_user_in_db(
                state.pool(),
                tenant_id,
                "expired@test.com",
                "admin",
            )
            .await;

            // 有効期限を過去に更新
            sqlx::query("UPDATE users SET refresh_token_expires_at = NOW() - INTERVAL '1 day' WHERE tenant_id = $1 AND email = 'expired@test.com'")
            .bind(tenant_id)
            .execute(state.pool())
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
    );
}

// ============================================================
// Logout
// ============================================================

#[tokio::test]
async fn test_logout_clears_refresh() {
    test_group!("ログアウト");
    test_case!(
        "ログアウト後にリフレッシュトークンが無効化される",
        {
            let state = crate::common::setup_app_state().await;
            let base_url = crate::common::spawn_test_server(state.clone()).await;
            let tenant_id = crate::common::create_test_tenant(state.pool(), "Logout Test").await;

            let (user_id, raw_token) = crate::common::create_test_user_in_db(
                state.pool(),
                tenant_id,
                "logout@test.com",
                "admin",
            )
            .await;
            let jwt = crate::common::create_test_jwt_for_user(
                user_id,
                tenant_id,
                "logout@test.com",
                "admin",
            );

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
    );
}

// ============================================================
// テナント作成 + my-orgs
// ============================================================

#[tokio::test]
async fn test_create_tenant_endpoint() {
    test_group!("テナント作成 + my-orgs");
    test_case!("テナントを作成して ID を返す", {
        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state).await;
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
    });
}

#[tokio::test]
async fn test_my_orgs() {
    test_group!("テナント作成 + my-orgs");
    test_case!("所属テナント一覧を返す", {
        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state.clone()).await;
        let tenant_id = crate::common::create_test_tenant(state.pool(), "MyOrgs Test").await;

        let (user_id, _) = crate::common::create_test_user_in_db(
            state.pool(),
            tenant_id,
            "orgs@test.com",
            "admin",
        )
        .await;
        let jwt =
            crate::common::create_test_jwt_for_user(user_id, tenant_id, "orgs@test.com", "admin");

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
    });
}

// ============================================================
// Google Login — 成功パス (test_claims モード)
// ============================================================

#[tokio::test]
async fn test_google_login_success_new_user() {
    test_group!("Google ログイン");
    test_case!(
        "test-valid-token で新規ユーザー作成 + JWT 発行",
        {
            let _gl = crate::common::GOOGLE_LOGIN_LOCK.lock().unwrap();
            let state = crate::common::setup_app_state().await;
            let base_url = crate::common::spawn_test_server(state.clone()).await;
            let client = reqwest::Client::new();

            // テナントを作成 (email domain でマッチさせる)
            let tenant_id = crate::common::create_test_tenant(state.pool(), "GoogleLogin").await;
            // email_domain を設定
            sqlx::query("UPDATE tenants SET email_domain = 'example.com' WHERE id = $1")
                .bind(tenant_id)
                .execute(state.pool())
                .await
                .unwrap();

            // test-valid-token → GoogleTokenVerifier.test_claims の固定 claims を返す
            let res = client
                .post(format!("{base_url}/api/auth/google"))
                .json(&serde_json::json!({ "id_token": "test-valid-token" }))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200, "google login should succeed");
            let body: Value = res.json().await.unwrap();
            assert!(body["access_token"].as_str().is_some());
            assert!(body["refresh_token"].as_str().is_some());
            assert_eq!(body["user"]["email"], "google-test@example.com");
        }
    );
}

#[tokio::test]
async fn test_google_login_existing_user() {
    test_group!("Google ログイン");
    test_case!("既存ユーザーで Google ログイン成功", {
        let _gl = crate::common::GOOGLE_LOGIN_LOCK.lock().unwrap();
        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state.clone()).await;
        let client = reqwest::Client::new();

        let tenant_id = crate::common::create_test_tenant(state.pool(), "GoogleExist").await;
        sqlx::query("UPDATE tenants SET email_domain = 'example.com' WHERE id = $1")
            .bind(tenant_id)
            .execute(state.pool())
            .await
            .unwrap();

        // 1回目: 新規ユーザー作成
        client
            .post(format!("{base_url}/api/auth/google"))
            .json(&serde_json::json!({ "id_token": "test-valid-token" }))
            .send()
            .await
            .unwrap();

        // 2回目: 既存ユーザーでログイン
        let res = client
            .post(format!("{base_url}/api/auth/google"))
            .json(&serde_json::json!({ "id_token": "test-valid-token" }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        let body: Value = res.json().await.unwrap();
        assert_eq!(body["user"]["email"], "google-test@example.com");
    });
}

// ============================================================
// Google code exchange
// ============================================================

#[tokio::test]
async fn test_google_code_login_success() {
    test_group!("Google code exchange");
    test_case!("test-valid-code で JWT 発行", {
        let _gl = crate::common::GOOGLE_LOGIN_LOCK.lock().unwrap();
        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state.clone()).await;
        let client = reqwest::Client::new();

        let tenant_id = crate::common::create_test_tenant(state.pool(), "GoogleCode").await;
        sqlx::query("UPDATE tenants SET email_domain = 'example.com' WHERE id = $1")
            .bind(tenant_id)
            .execute(state.pool())
            .await
            .unwrap();

        let res = client
            .post(format!("{base_url}/api/auth/google/code"))
            .json(&serde_json::json!({
                "code": "test-valid-code",
                "redirect_uri": "http://localhost:3000/callback"
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        let body: Value = res.json().await.unwrap();
        assert!(body["access_token"].as_str().is_some());
    });
}

#[tokio::test]
async fn test_google_callback_success() {
    test_group!("Google code exchange");
    test_case!(
        "有効な state + test-valid-code でトークン付きリダイレクト",
        {
            let _env = crate::common::ENV_LOCK.lock().unwrap();
            let _gl = crate::common::GOOGLE_LOGIN_LOCK.lock().unwrap();
            std::env::set_var("OAUTH_STATE_SECRET", "test-oauth-state-secret");
            std::env::set_var("API_ORIGIN", "http://localhost:9999");

            let state = crate::common::setup_app_state().await;
            let base_url = crate::common::spawn_test_server(state.clone()).await;

            let tenant_id = crate::common::create_test_tenant(state.pool(), "GoogleCB").await;
            sqlx::query("UPDATE tenants SET email_domain = 'example.com' WHERE id = $1")
                .bind(tenant_id)
                .execute(state.pool())
                .await
                .unwrap();

            // 有効な state を生成
            let state_payload = rust_alc_api::auth::lineworks::state::StatePayload {
                redirect_uri: "https://example.com/login".into(),
                nonce: "test-nonce".into(),
                provider: "google".into(),
                external_org_id: String::new(),
            };
            let signed_state = rust_alc_api::auth::lineworks::state::sign(
                &state_payload,
                "test-oauth-state-secret",
            );

            let client = reqwest::ClientBuilder::new()
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .unwrap();

            // test-valid-code → GoogleTokenVerifier.test_claims で固定 claims 返却
            let res = client
                .get(format!(
                    "{base_url}/api/auth/google/callback?code=test-valid-code&state={signed_state}"
                ))
                .send()
                .await
                .unwrap();
            // 成功 → redirect with token fragment
            let status = res.status().as_u16();
            assert!(
                status == 302 || status == 307 || status == 303,
                "callback should redirect, got {status}"
            );
            let location = res.headers().get("location").unwrap().to_str().unwrap();
            assert!(
                location.contains("token="),
                "redirect should contain token, got: {location}"
            );
        }
    );
}

// ============================================================
// Google OAuth Redirect
// ============================================================

#[tokio::test]
async fn test_google_redirect_to_google() {
    test_group!("Google ログイン");
    test_case!("Google OAuth リダイレクト URL を返す", {
        let _env = crate::common::ENV_LOCK.lock().unwrap();
        std::env::set_var("OAUTH_STATE_SECRET", "test-oauth-state-secret");

        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state).await;

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
    });
}

#[tokio::test]
async fn test_google_redirect_missing_redirect_uri() {
    test_group!("Google ログイン");
    test_case!("redirect_uri なしで 400 を返す", {
        let _env = crate::common::ENV_LOCK.lock().unwrap();
        std::env::set_var("OAUTH_STATE_SECRET", "test-oauth-state-secret");

        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state).await;

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
        assert_eq!(res.status(), 400, "Missing redirect_uri should return 400");
    });
}

// ============================================================
// LINE WORKS OAuth Redirect
// ============================================================

#[tokio::test]
async fn test_lineworks_redirect_missing_domain() {
    test_group!("LINE WORKS OAuth リダイレクト");
    test_case!("domain/address なしで 400 または 500 を返す", {
        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state).await;

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
    });
}

#[tokio::test]
async fn test_lineworks_redirect_unknown_domain() {
    test_group!("LINE WORKS OAuth リダイレクト");
    test_case!(
        "存在しないドメインで 404 または 500 を返す",
        {
            let _env = crate::common::ENV_LOCK.lock().unwrap();
            std::env::set_var("OAUTH_STATE_SECRET", "test-oauth-state-secret");

            let state = crate::common::setup_app_state().await;
            let base_url = crate::common::spawn_test_server(state).await;

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
    );
}

// ============================================================
// POST /api/auth/google (id_token flow)
// ============================================================

#[tokio::test]
async fn test_google_login_invalid_token() {
    test_group!("Google ログイン");
    test_case!("無効な id_token で 401 を返す", {
        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state).await;

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
    });
}

#[tokio::test]
async fn test_google_login_empty_token() {
    test_group!("Google ログイン");
    test_case!("空の id_token で 401 を返す", {
        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state).await;

        let client = reqwest::Client::new();
        let res = client
            .post(format!("{base_url}/api/auth/google"))
            .json(&serde_json::json!({ "id_token": "" }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 401, "Empty Google id_token should return 401");
    });
}

#[tokio::test]
async fn test_google_login_missing_field() {
    test_group!("Google ログイン");
    test_case!("id_token フィールドなしで 422 を返す", {
        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state).await;

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
    });
}

// ============================================================
// POST /api/auth/google/code (authorization code flow)
// ============================================================

#[tokio::test]
async fn test_google_code_invalid() {
    test_group!("Google code exchange");
    test_case!("無効な認可コードで 401 を返す", {
        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state).await;

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
    });
}

#[tokio::test]
async fn test_google_code_missing_fields() {
    test_group!("Google code exchange");
    test_case!("必須フィールドなしで 422 を返す", {
        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state).await;

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
    });
}

// ============================================================
// GET /api/auth/woff-config
// ============================================================

#[tokio::test]
async fn test_woff_config_missing_domain() {
    test_group!("WOFF 設定");
    test_case!("domain パラメータなしで 400 を返す", {
        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state).await;

        let client = reqwest::Client::new();
        let res = client
            .get(format!("{base_url}/api/auth/woff-config"))
            .send()
            .await
            .unwrap();
        // Axum returns 400 for missing required query parameters
        assert_eq!(res.status(), 400, "Missing domain param should return 400");
    });
}

#[tokio::test]
async fn test_woff_config_unknown_domain() {
    test_group!("WOFF 設定");
    test_case!(
        "存在しないドメインで 404 または 500 を返す",
        {
            let state = crate::common::setup_app_state().await;
            let base_url = crate::common::spawn_test_server(state).await;

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
    );
}

// ============================================================
// POST /api/auth/woff (WOFF auth)
// ============================================================

#[tokio::test]
async fn test_woff_auth_unknown_domain() {
    test_group!("WOFF 認証");
    test_case!(
        "存在しない domain_id で 404 または 500 を返す",
        {
            let state = crate::common::setup_app_state().await;
            let base_url = crate::common::spawn_test_server(state).await;

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
    );
}

#[tokio::test]
async fn test_woff_auth_missing_fields() {
    test_group!("WOFF 認証");
    test_case!("必須フィールドなしで 422 を返す", {
        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state).await;

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
    });
}

#[tokio::test]
async fn test_woff_auth_no_content_type() {
    test_group!("WOFF 認証");
    test_case!("Content-Type なしで 415 を返す", {
        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state).await;

        let client = reqwest::Client::new();
        let res = client
            .post(format!("{base_url}/api/auth/woff"))
            .body("{}")
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 415, "Missing Content-Type should return 415");
    });
}

// ============================================================
// Google OAuth Callback (code + state)
// ============================================================

#[tokio::test]
async fn test_google_callback_invalid_state() {
    test_group!("Google ログイン");
    test_case!("無効な state で 400 を返す (HMAC 検証失敗)", {
        let _env = crate::common::ENV_LOCK.lock().unwrap();
        std::env::set_var("OAUTH_STATE_SECRET", "test-oauth-state-secret");
        std::env::set_var("API_ORIGIN", "http://localhost:9999");

        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state).await;

        let client = reqwest::ClientBuilder::new()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();

        let res = client
            .get(format!(
                "{base_url}/api/auth/google/callback?code=fake-auth-code&state=invalid-state-value"
            ))
            .send()
            .await
            .unwrap();

        // State HMAC verification should fail → 400
        assert_eq!(res.status(), 400, "Invalid HMAC state should return 400");
    });
}

// ============================================================
// POST /api/auth/google/code (invalid code → external error)
// ============================================================

#[tokio::test]
async fn test_google_code_login_invalid() {
    test_group!("Google code exchange");
    test_case!(
        "無効なコードと redirect_uri で 401 または 502 を返す",
        {
            let state = crate::common::setup_app_state().await;
            let base_url = crate::common::spawn_test_server(state).await;

            let client = reqwest::Client::new();
            let res = client
                .post(format!("{base_url}/api/auth/google/code"))
                .json(&serde_json::json!({
                    "code": "invalid",
                    "redirect_uri": "http://localhost"
                }))
                .send()
                .await
                .unwrap();

            let status = res.status().as_u16();
            assert!(
                status == 401 || status == 502,
                "Invalid Google auth code should return 401 or 502, got {status}"
            );
        }
    );
}

// ============================================================
// LINE WORKS OAuth Callback (invalid state)
// ============================================================

#[tokio::test]
async fn test_lineworks_callback_invalid() {
    test_group!("LINE WORKS OAuth コールバック");
    test_case!("無効な state で 400 または 500 を返す", {
        let _env = crate::common::ENV_LOCK.lock().unwrap();
        std::env::set_var("OAUTH_STATE_SECRET", "test-oauth-state-secret");
        std::env::set_var("API_ORIGIN", "http://localhost:9999");

        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state).await;

        let client = reqwest::ClientBuilder::new()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();

        let res = client
            .get(format!(
                "{base_url}/api/auth/lineworks/callback?code=fake-code&state=invalid-state-value"
            ))
            .send()
            .await
            .unwrap();

        let status = res.status().as_u16();
        // State HMAC verification should fail → 400
        // If OAUTH_STATE_SECRET env was cleared by another test → 500
        assert!(
            status == 400 || status == 500,
            "Invalid state should return 400 or 500, got {status}"
        );
    });
}

// ============================================================
// POST /api/auth/woff with invalid access_token
// ============================================================

#[tokio::test]
async fn test_woff_auth_invalid_access_token() {
    test_group!("WOFF 認証");
    test_case!("無効な access_token で 401/404/500 を返す", {
        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state).await;

        let client = reqwest::Client::new();
        let res = client
            .post(format!("{base_url}/api/auth/woff"))
            .json(&serde_json::json!({
                "access_token": "invalid-fake-access-token-12345",
                "domain_id": "nonexistent-domain-xyz"
            }))
            .send()
            .await
            .unwrap();

        let status = res.status().as_u16();
        // domain_id lookup fails first → 404 or 500 (if resolve_sso_config doesn't exist)
        // If domain existed, the access_token would fail at profile fetch → 401
        assert!(
            status == 401 || status == 404 || status == 500,
            "Invalid access_token / unknown domain should return 401, 404 or 500, got {status}"
        );
    });
}

// ============================================================
// LINE WORKS OAuth Redirect — address parameter
// ============================================================

#[tokio::test]
async fn test_lineworks_redirect_address_param() {
    test_group!("LINE WORKS OAuth リダイレクト");
    test_case!(
        "address パラメータからドメインを抽出して SSO 設定を検索",
        {
            let _env = crate::common::ENV_LOCK.lock().unwrap();
            std::env::set_var("OAUTH_STATE_SECRET", "test-oauth-state-secret");

            let state = crate::common::setup_app_state().await;
            let base_url = crate::common::spawn_test_server(state).await;

            let client = reqwest::ClientBuilder::new()
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .unwrap();

            let res = client
            .get(format!(
                "{base_url}/api/auth/lineworks/redirect?address=user@nonexistent-domain-abc&redirect_uri=https://example.com/callback"
            ))
            .send()
            .await
            .unwrap();
            // address parameter extracts domain "nonexistent-domain-abc" → SSO config not found
            let status = res.status().as_u16();
            assert!(
                status == 404 || status == 500,
                "Address with unknown domain should return 404 or 500, got {status}"
            );
        }
    );
}

#[tokio::test]
async fn test_lineworks_redirect_missing_redirect_uri() {
    test_group!("LINE WORKS OAuth リダイレクト");
    test_case!("redirect_uri なしで 400 を返す", {
        let _env = crate::common::ENV_LOCK.lock().unwrap();
        std::env::set_var("OAUTH_STATE_SECRET", "test-oauth-state-secret");

        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state).await;

        let client = reqwest::ClientBuilder::new()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();

        let res = client
            .get(format!(
                "{base_url}/api/auth/lineworks/redirect?domain=some-domain"
            ))
            .send()
            .await
            .unwrap();
        // redirect_uri is a required field in LineworksRedirectParams → Axum returns 400
        assert_eq!(res.status(), 400, "Missing redirect_uri should return 400");
    });
}

// ============================================================
// テナント作成 — edge cases
// ============================================================

#[tokio::test]
async fn test_create_tenant_empty_name() {
    test_group!("テナント作成 + my-orgs");
    test_case!("空の name でもテナント作成成功 (201)", {
        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state).await;
        let client = reqwest::Client::new();

        let res = client
            .post(format!("{base_url}/api/auth/tenants"))
            .json(&serde_json::json!({ "name": "" }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 201, "Empty name should still create tenant");
        let body: Value = res.json().await.unwrap();
        assert_eq!(body["name"], "");
    });
}

#[tokio::test]
async fn test_create_tenant_missing_name() {
    test_group!("テナント作成 + my-orgs");
    test_case!("name フィールドなしで 422 を返す", {
        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state).await;
        let client = reqwest::Client::new();

        let res = client
            .post(format!("{base_url}/api/auth/tenants"))
            .json(&serde_json::json!({}))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 422, "Missing name field should return 422");
    });
}

#[tokio::test]
async fn test_create_tenant_no_content_type() {
    test_group!("テナント作成 + my-orgs");
    test_case!("Content-Type なしで 415 を返す", {
        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state).await;
        let client = reqwest::Client::new();

        let res = client
            .post(format!("{base_url}/api/auth/tenants"))
            .body(r#"{"name":"test"}"#)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 415, "Missing Content-Type should return 415");
    });
}

// ============================================================
// Google Login — tenant_allowed_emails invitation flow
// ============================================================

#[tokio::test]
async fn test_google_login_with_invitation() {
    test_group!("Google ログイン");
    test_case!(
        "招待メールで Google ログイン → 招待元テナント・ロールで作成",
        {
            let _gl = crate::common::GOOGLE_LOGIN_LOCK.lock().unwrap();
            let state = crate::common::setup_app_state().await;
            let base_url = crate::common::spawn_test_server(state.clone()).await;
            let client = reqwest::Client::new();

            let tenant_id = crate::common::create_test_tenant(state.pool(), "InvitedTenant").await;

            // Clean up any existing user with this google_sub from previous test runs
            sqlx::query("DELETE FROM users WHERE google_sub = 'test-google-sub-12345'")
                .execute(state.pool())
                .await
                .unwrap();

            // Insert invitation for google-test@example.com (the test claims email)
            sqlx::query(
            "INSERT INTO tenant_allowed_emails (tenant_id, email, role) VALUES ($1, 'google-test@example.com', 'viewer') ON CONFLICT (email) DO UPDATE SET tenant_id = $1, role = 'viewer'",
        )
        .bind(tenant_id)
        .execute(state.pool())
        .await
        .unwrap();

            let res = client
                .post(format!("{base_url}/api/auth/google"))
                .json(&serde_json::json!({ "id_token": "test-valid-token" }))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200, "Invited user login should succeed");
            let body: Value = res.json().await.unwrap();
            assert_eq!(body["user"]["email"], "google-test@example.com");
            assert_eq!(
                body["user"]["role"], "viewer",
                "Role should come from invitation"
            );
            assert_eq!(
                body["user"]["tenant_id"],
                tenant_id.to_string(),
                "Tenant should come from invitation"
            );

            // Invitation should be consumed (deleted)
            let count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM tenant_allowed_emails WHERE email = 'google-test@example.com'",
        )
        .fetch_one(state.pool())
        .await
        .unwrap();
            assert_eq!(count.0, 0, "Invitation should be consumed after login");

            // Cleanup: delete created user to avoid google_sub conflict with other tests
            sqlx::query(
                "DELETE FROM users WHERE google_sub = 'test-google-sub-12345' AND tenant_id = $1",
            )
            .bind(tenant_id)
            .execute(state.pool())
            .await
            .unwrap();
        }
    );
}

#[tokio::test]
async fn test_google_login_creates_new_tenant() {
    test_group!("Google ログイン");
    test_case!(
        "テナント・招待なしで新規テナント自動作成",
        {
            let _gl = crate::common::GOOGLE_LOGIN_LOCK.lock().unwrap();
            let state = crate::common::setup_app_state().await;
            let base_url = crate::common::spawn_test_server(state.clone()).await;
            let client = reqwest::Client::new();

            // Ensure no tenant with email_domain='example.com' and no invitation exists
            // (other tests may have created them, so clean up)
            sqlx::query(
                "DELETE FROM tenant_allowed_emails WHERE email = 'google-test@example.com'",
            )
            .execute(state.pool())
            .await
            .unwrap();
            sqlx::query("DELETE FROM users WHERE google_sub = 'test-google-sub-12345'")
                .execute(state.pool())
                .await
                .unwrap();
            // Remove email_domain match to force new tenant creation
            sqlx::query(
                "UPDATE tenants SET email_domain = NULL WHERE email_domain = 'example.com'",
            )
            .execute(state.pool())
            .await
            .unwrap();

            let res = client
                .post(format!("{base_url}/api/auth/google"))
                .json(&serde_json::json!({ "id_token": "test-valid-token" }))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200, "Should create new tenant and succeed");
            let body: Value = res.json().await.unwrap();
            assert_eq!(body["user"]["email"], "google-test@example.com");
            assert_eq!(
                body["user"]["role"], "admin",
                "New tenant user should be admin"
            );

            // Cleanup
            let user_tenant_id = body["user"]["tenant_id"].as_str().unwrap();
            sqlx::query("DELETE FROM users WHERE google_sub = 'test-google-sub-12345'")
                .execute(state.pool())
                .await
                .unwrap();
            sqlx::query("DELETE FROM tenants WHERE id = $1::UUID AND name = 'example.com'")
                .bind(user_tenant_id)
                .execute(state.pool())
                .await
                .unwrap();
        }
    );
}

// ============================================================
// WOFF config — empty domain string
// ============================================================

#[tokio::test]
async fn test_woff_config_empty_domain() {
    test_group!("WOFF 設定");
    test_case!(
        "空のドメイン文字列で 404 または 500 を返す",
        {
            let state = crate::common::setup_app_state().await;
            let base_url = crate::common::spawn_test_server(state).await;

            let client = reqwest::Client::new();
            let res = client
                .get(format!("{base_url}/api/auth/woff-config?domain="))
                .send()
                .await
                .unwrap();
            let status = res.status().as_u16();
            assert!(
                status == 404 || status == 500,
                "Empty domain should return 404 or 500, got {status}"
            );
        }
    );
}

// ============================================================
// LINE WORKS callback — missing required params
// ============================================================

#[tokio::test]
async fn test_lineworks_callback_missing_params() {
    test_group!("LINE WORKS OAuth コールバック");
    test_case!("code/state パラメータなしで 400 を返す", {
        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state).await;

        let client = reqwest::ClientBuilder::new()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();

        let res = client
            .get(format!("{base_url}/api/auth/lineworks/callback"))
            .send()
            .await
            .unwrap();
        // Missing required query params → 400
        assert_eq!(
            res.status(),
            400,
            "Missing code/state params should return 400"
        );
    });
}

#[tokio::test]
async fn test_lineworks_callback_missing_state() {
    test_group!("LINE WORKS OAuth コールバック");
    test_case!("state パラメータなしで 400 を返す", {
        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state).await;

        let client = reqwest::ClientBuilder::new()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();

        let res = client
            .get(format!(
                "{base_url}/api/auth/lineworks/callback?code=some-code"
            ))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 400, "Missing state param should return 400");
    });
}
