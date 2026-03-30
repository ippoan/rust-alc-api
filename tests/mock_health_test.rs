mod common;
mod mock_helpers;

use mock_helpers::app_state::setup_mock_app_state;

// ---------------------------------------------------------------------------
// GET /api/health — returns {"status": "ok"}
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_health_check_returns_ok() {
    let state = setup_mock_app_state();
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
}
