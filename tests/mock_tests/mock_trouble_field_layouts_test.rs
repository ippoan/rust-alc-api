use std::sync::Arc;
use uuid::Uuid;

use crate::mock_helpers::MockTroubleFieldLayoutsRepository;

// ---------------------------------------------------------------------------
// Helper
// ---------------------------------------------------------------------------

async fn setup() -> (String, String) {
    let state = crate::mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let base = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let auth = format!("Bearer {jwt}");
    (base, auth)
}

async fn setup_failing() -> (String, String) {
    let mock = Arc::new(MockTroubleFieldLayoutsRepository::default());
    mock.fail_next
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let state = crate::mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let mut trouble_state = crate::mock_helpers::app_state::setup_mock_trouble_state();
    trouble_state.trouble_field_layouts = mock;
    let base =
        crate::mock_helpers::app_state::spawn_mock_server_with_trouble(state, trouble_state).await;
    let auth = format!("Bearer {jwt}");
    (base, auth)
}

fn client() -> reqwest::Client {
    reqwest::Client::new()
}

// ===========================================================================
// GET /api/trouble/field-layout — get_field_layout
// ===========================================================================

#[tokio::test]
async fn get_field_layout_default_empty() {
    let (base, auth) = setup().await;
    let res = client()
        .get(format!("{base}/api/trouble/field-layout"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["settings"], serde_json::json!([]));
}

#[tokio::test]
async fn get_field_layout_db_error() {
    let (base, auth) = setup_failing().await;
    let res = client()
        .get(format!("{base}/api/trouble/field-layout"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// ===========================================================================
// PUT /api/trouble/field-layout — update_field_layout
// ===========================================================================

#[tokio::test]
async fn update_field_layout_success_roundtrip() {
    let (base, auth) = setup().await;

    let payload = serde_json::json!({
        "settings": [
            { "key": "counterparty_vehicle", "visible": true, "width": "half", "sort_order": 10, "label": null },
            { "key": "progress_notes", "visible": false, "width": "half", "sort_order": 20, "label": "対応状況" }
        ]
    });

    let res = client()
        .put(format!("{base}/api/trouble/field-layout"))
        .header("Authorization", &auth)
        .json(&payload)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["settings"][0]["key"], "counterparty_vehicle");
    assert_eq!(body["settings"][1]["label"], "対応状況");

    // upsert された値が get で読み返せる
    let res = client()
        .get(format!("{base}/api/trouble/field-layout"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["settings"][1]["visible"], false);
}

#[tokio::test]
async fn update_field_layout_db_error() {
    let (base, auth) = setup_failing().await;
    let res = client()
        .put(format!("{base}/api/trouble/field-layout"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({ "settings": [] }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}
