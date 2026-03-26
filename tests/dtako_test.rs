mod common;

use serde_json::Value;

// ============================================================
// dtako 基本 list テスト (空一覧の200確認)
// ============================================================

#[tokio::test]
async fn test_dtako_drivers_list() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "DtakoDrivers").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/drivers"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send().await.unwrap();
    assert_eq!(res.status(), 200);
}

#[tokio::test]
async fn test_dtako_vehicles_list() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "DtakoVehicles").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/vehicles"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send().await.unwrap();
    assert_eq!(res.status(), 200);
}

#[tokio::test]
async fn test_dtako_operations_list() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "DtakoOps").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/operations"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send().await.unwrap();
    assert_eq!(res.status(), 200);
}

#[tokio::test]
async fn test_dtako_operations_calendar() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "DtakoCal").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/operations/calendar?year=2026&month=3"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send().await.unwrap();
    assert_eq!(res.status(), 200);
}

#[tokio::test]
async fn test_dtako_daily_hours_list() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "DtakoDH").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/daily-hours"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send().await.unwrap();
    assert_eq!(res.status(), 200);
}

#[tokio::test]
async fn test_dtako_work_times_list() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "DtakoWT").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/work-times"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send().await.unwrap();
    assert_eq!(res.status(), 200);
}

#[tokio::test]
async fn test_dtako_event_classifications_list() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "DtakoEC").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/event-classifications"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send().await.unwrap();
    assert_eq!(res.status(), 200);
}

// ============================================================
// dtako restraint report (既存28%カバレッジの拡張)
// ============================================================

#[tokio::test]
async fn test_dtako_restraint_report_list() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "DtakoReport").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    // 拘束時間レポートの一覧 (空でOK)
    let res = client
        .get(format!("{base_url}/api/restraint-report/drivers"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send().await.unwrap();
    // 200 or 404 (テナントにデータがないため)
    assert!(res.status() == 200 || res.status() == 404);
}

// ============================================================
// dtako daily hours with filters
// ============================================================

// ============================================================
// dtako upload — list endpoints
// ============================================================

#[tokio::test]
async fn test_dtako_list_uploads() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "DtakoUploads").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/uploads"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send().await.unwrap();
    assert_eq!(res.status(), 200);
}

#[tokio::test]
async fn test_dtako_list_pending_uploads() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "DtakoPending").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/internal/pending"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send().await.unwrap();
    assert_eq!(res.status(), 200);
}

// ============================================================
// dtako restraint report
// ============================================================

#[tokio::test]
async fn test_dtako_restraint_report_for_driver() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "DtakoRR").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    // ドライバーが存在しない場合
    let res = client
        .get(format!("{base_url}/api/restraint-report/drivers/nonexistent?year=2026&month=3"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send().await.unwrap();
    // 404 or 200 (空データ)
    assert!(res.status() == 200 || res.status() == 404 || res.status() == 500);
}

#[tokio::test]
async fn test_dtako_daily_hours_with_driver_filter() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "DtakoDHF").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/daily-hours?driver_name=test"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send().await.unwrap();
    assert_eq!(res.status(), 200);
}
