#[macro_use]
mod common;

#[tokio::test]
async fn test_health_check() {
    test_group!("ヘルスチェック");
    test_case!("GET /api/health で status=ok を返す", {
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state).await;

        let client = reqwest::Client::new();
        let res = client
            .get(format!("{base_url}/api/health"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        let body: serde_json::Value = res.json().await.unwrap();
        assert_eq!(body["status"], "ok");
    });
}
