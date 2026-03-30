#[macro_use]
mod common;

use serde_json::Value;

#[tokio::test]
async fn test_measurement_tenant_isolation() {
    test_group!("測定取得・更新");
    test_case!("テナント分離", {
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;

        let tenant_a = common::create_test_tenant(state.pool(), "Meas Iso A").await;
        let tenant_b = common::create_test_tenant(state.pool(), "Meas Iso B").await;

        let jwt_a = common::create_test_jwt(tenant_a, "admin");
        let jwt_b = common::create_test_jwt(tenant_b, "admin");
        let auth_a = format!("Bearer {jwt_a}");
        let auth_b = format!("Bearer {jwt_b}");
        let client = reqwest::Client::new();

        // テナント A に従業員+測定作成
        let emp = common::create_test_employee(&client, &base_url, &auth_a, "EmpA", "EA1").await;
        let emp_id = emp["id"].as_str().unwrap();
        let m = common::create_test_measurement(&client, &base_url, &auth_a, emp_id).await;
        let m_id = m["id"].as_str().unwrap();

        // テナント A → 見える
        let res = client
            .get(format!("{base_url}/api/measurements"))
            .header("Authorization", &auth_a)
            .send()
            .await
            .unwrap();
        let body: Value = res.json().await.unwrap();
        assert_eq!(body["total"], 1);

        // テナント B → 見えない
        let res = client
            .get(format!("{base_url}/api/measurements"))
            .header("Authorization", &auth_b)
            .send()
            .await
            .unwrap();
        let body: Value = res.json().await.unwrap();
        assert_eq!(body["total"], 0);

        // テナント B → 個別取得 404
        let res = client
            .get(format!("{base_url}/api/measurements/{m_id}"))
            .header("Authorization", &auth_b)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 404);
    });
}
