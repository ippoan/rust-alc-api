#[macro_use]
mod common;

use serde_json::Value;

// ============================================================
// テナント分離
// ============================================================

#[tokio::test]
async fn test_tenant_isolation() {
    test_group!("テナント分離");
    test_case!(
        "テナントAの従業員がテナントBから見えないこと",
        {
            let state = common::setup_app_state().await;
            let base_url = common::spawn_test_server(state.clone()).await;

            let tenant_a = common::create_test_tenant(state.pool(), "Tenant A").await;
            let tenant_b = common::create_test_tenant(state.pool(), "Tenant B").await;

            let jwt_a = common::create_test_jwt(tenant_a, "admin");
            let jwt_b = common::create_test_jwt(tenant_b, "admin");

            let client = reqwest::Client::new();

            // テナント A に従業員を作成
            let res = client
                .post(format!("{base_url}/api/employees"))
                .header("Authorization", format!("Bearer {jwt_a}"))
                .json(&serde_json::json!({
                    "name": "Employee A",
                    "code": "A001"
                }))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 201, "Failed to create employee A");

            // テナント A で一覧取得 → 1件見える
            let res = client
                .get(format!("{base_url}/api/employees"))
                .header("Authorization", format!("Bearer {jwt_a}"))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
            let employees: Vec<Value> = res.json().await.unwrap();
            assert_eq!(employees.len(), 1, "Tenant A should see 1 employee");
            assert_eq!(employees[0]["name"], "Employee A");

            // テナント B で一覧取得 → 0件 (RLS で分離)
            let res = client
                .get(format!("{base_url}/api/employees"))
                .header("Authorization", format!("Bearer {jwt_b}"))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
            let employees: Vec<Value> = res.json().await.unwrap();
            assert_eq!(
                employees.len(),
                0,
                "Tenant B should see 0 employees (RLS isolation)"
            );
        }
    );
}

#[tokio::test]
async fn test_kiosk_mode_with_tenant_header() {
    test_group!("テナント分離");
    test_case!(
        "X-Tenant-IDヘッダーによるキオスクモード認証",
        {
            let state = common::setup_app_state().await;
            let base_url = common::spawn_test_server(state.clone()).await;

            let tenant_id = common::create_test_tenant(state.pool(), "Kiosk Tenant").await;

            let client = reqwest::Client::new();

            // X-Tenant-ID ヘッダーで従業員一覧取得
            let res = client
                .get(format!("{base_url}/api/employees"))
                .header("X-Tenant-ID", tenant_id.to_string())
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200, "X-Tenant-ID header should be accepted");
        }
    );
}
