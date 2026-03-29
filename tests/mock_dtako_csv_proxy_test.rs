mod common;
mod mock_helpers;

use std::sync::atomic::Ordering;
use std::sync::Arc;

use mock_helpers::app_state::setup_mock_app_state;
use mock_helpers::MockDtakoCsvProxyRepository;

/// MockStorage に CSV を配置するヘルパー。
/// MockDtakoCsvProxyRepository は常に Ok(None) を返すため、
/// フォールバックキー: `{tenant_id}/unko/{unko_no}/{filename}` が使われる。
async fn upload_csv_to_mock_storage(
    storage: &dyn rust_alc_api::storage::StorageBackend,
    tenant_id: &uuid::Uuid,
    unko_no: &str,
    filename: &str,
    csv_content: &str,
) {
    let key = format!("{}/unko/{}/{}", tenant_id, unko_no, filename);
    storage
        .upload(&key, csv_content.as_bytes(), "text/csv")
        .await
        .unwrap();
}

/// テスト用テナントIDを固定生成 (JWT と一致させるため)
fn test_tenant_id() -> uuid::Uuid {
    // create_test_jwt が内部で new_v4() するため、create_test_jwt_for_user を使うか
    // 任意の UUID で JWT を発行して、その tenant_id を返す
    uuid::Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap()
}

fn test_auth_header() -> String {
    let tenant_id = test_tenant_id();
    let jwt = common::create_test_jwt_for_user(
        uuid::Uuid::new_v4(),
        tenant_id,
        "mock-test@example.com",
        "admin",
    );
    format!("Bearer {jwt}")
}

// =============================================================
// Success cases: all csv_type aliases
// =============================================================

#[tokio::test]
async fn test_csv_proxy_kudguri() {
    let state = setup_mock_app_state().await;
    let tenant_id = test_tenant_id();
    let storage = state.dtako_storage.as_ref().unwrap();
    upload_csv_to_mock_storage(
        &**storage,
        &tenant_id,
        "1001",
        "KUDGURI.csv",
        "col_a,col_b\nv1,v2\n",
    )
    .await;

    let base_url = common::spawn_test_server(state).await;
    let client = reqwest::Client::new();
    let res = client
        .get(format!("{base_url}/api/operations/1001/csv/kudguri"))
        .header("Authorization", test_auth_header())
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    let headers = body["headers"].as_array().unwrap();
    assert_eq!(headers.len(), 2);
    assert_eq!(headers[0], "col_a");
    assert_eq!(headers[1], "col_b");
    let rows = body["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0][0], "v1");
    assert_eq!(rows[0][1], "v2");
}

#[tokio::test]
async fn test_csv_proxy_kudgivt_alias() {
    let state = setup_mock_app_state().await;
    let tenant_id = test_tenant_id();
    let storage = state.dtako_storage.as_ref().unwrap();
    upload_csv_to_mock_storage(
        &**storage,
        &tenant_id,
        "2001",
        "KUDGIVT.csv",
        "h1,h2,h3\na,b,c\n",
    )
    .await;

    let base_url = common::spawn_test_server(state).await;
    let client = reqwest::Client::new();
    let res = client
        .get(format!("{base_url}/api/operations/2001/csv/kudgivt"))
        .header("Authorization", test_auth_header())
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
}

#[tokio::test]
async fn test_csv_proxy_events_alias() {
    let state = setup_mock_app_state().await;
    let tenant_id = test_tenant_id();
    let storage = state.dtako_storage.as_ref().unwrap();
    upload_csv_to_mock_storage(&**storage, &tenant_id, "2002", "KUDGIVT.csv", "x\n1\n").await;

    let base_url = common::spawn_test_server(state).await;
    let res = reqwest::Client::new()
        .get(format!("{base_url}/api/operations/2002/csv/events"))
        .header("Authorization", test_auth_header())
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
}

#[tokio::test]
async fn test_csv_proxy_ferry_aliases() {
    let state = setup_mock_app_state().await;
    let tenant_id = test_tenant_id();
    let storage = state.dtako_storage.as_ref().unwrap();
    upload_csv_to_mock_storage(&**storage, &tenant_id, "3001", "KUDGFRY.csv", "f1\nfv1\n").await;
    upload_csv_to_mock_storage(&**storage, &tenant_id, "3002", "KUDGFRY.csv", "f1\nfv1\n").await;
    upload_csv_to_mock_storage(&**storage, &tenant_id, "3003", "KUDGFRY.csv", "f1\nfv1\n").await;

    let base_url = common::spawn_test_server(state).await;
    let client = reqwest::Client::new();
    let auth = test_auth_header();

    for (unko, alias) in [("3001", "kudgfry"), ("3002", "ferry"), ("3003", "ferries")] {
        let res = client
            .get(format!("{base_url}/api/operations/{unko}/csv/{alias}"))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200, "Failed for alias: {alias}");
    }
}

#[tokio::test]
async fn test_csv_proxy_tolls_aliases() {
    let state = setup_mock_app_state().await;
    let tenant_id = test_tenant_id();
    let storage = state.dtako_storage.as_ref().unwrap();
    upload_csv_to_mock_storage(&**storage, &tenant_id, "4001", "KUDGSIR.csv", "t1\ntv1\n").await;
    upload_csv_to_mock_storage(&**storage, &tenant_id, "4002", "KUDGSIR.csv", "t1\ntv1\n").await;

    let base_url = common::spawn_test_server(state).await;
    let client = reqwest::Client::new();
    let auth = test_auth_header();

    for (unko, alias) in [("4001", "kudgsir"), ("4002", "tolls")] {
        let res = client
            .get(format!("{base_url}/api/operations/{unko}/csv/{alias}"))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200, "Failed for alias: {alias}");
    }
}

#[tokio::test]
async fn test_csv_proxy_speed_aliases() {
    let state = setup_mock_app_state().await;
    let tenant_id = test_tenant_id();
    let storage = state.dtako_storage.as_ref().unwrap();
    upload_csv_to_mock_storage(
        &**storage,
        &tenant_id,
        "5001",
        "SOKUDODATA.csv",
        "s1\nsv1\n",
    )
    .await;
    upload_csv_to_mock_storage(
        &**storage,
        &tenant_id,
        "5002",
        "SOKUDODATA.csv",
        "s1\nsv1\n",
    )
    .await;

    let base_url = common::spawn_test_server(state).await;
    let client = reqwest::Client::new();
    let auth = test_auth_header();

    for (unko, alias) in [("5001", "speed"), ("5002", "sokudo")] {
        let res = client
            .get(format!("{base_url}/api/operations/{unko}/csv/{alias}"))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200, "Failed for alias: {alias}");
    }
}

// =============================================================
// Some(prefix) branch coverage (line 50)
// =============================================================

#[tokio::test]
async fn test_csv_proxy_with_r2_prefix() {
    let mock_repo = Arc::new(MockDtakoCsvProxyRepository {
        fail_next: std::sync::atomic::AtomicBool::new(false),
        return_prefix: std::sync::Mutex::new(Some("custom/prefix".to_string())),
    });

    let mut state = setup_mock_app_state().await;
    // Upload CSV at the key that Some(prefix) branch will construct
    let storage = state.dtako_storage.as_ref().unwrap();
    storage
        .upload(
            "custom/prefix/KUDGURI.csv",
            b"col_x,col_y\nval1,val2\n",
            "text/csv",
        )
        .await
        .unwrap();

    state.dtako_csv_proxy = mock_repo;

    let base_url = common::spawn_test_server(state).await;
    let res = reqwest::Client::new()
        .get(format!("{base_url}/api/operations/anything/csv/kudguri"))
        .header("Authorization", test_auth_header())
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["headers"][0], "col_x");
    assert_eq!(body["rows"][0][0], "val1");
}

// =============================================================
// CSV parsing edge cases
// =============================================================

#[tokio::test]
async fn test_csv_proxy_empty_csv() {
    let state = setup_mock_app_state().await;
    let tenant_id = test_tenant_id();
    let storage = state.dtako_storage.as_ref().unwrap();
    upload_csv_to_mock_storage(&**storage, &tenant_id, "6001", "KUDGURI.csv", "h1,h2\n").await;

    let base_url = common::spawn_test_server(state).await;
    let res = reqwest::Client::new()
        .get(format!("{base_url}/api/operations/6001/csv/kudguri"))
        .header("Authorization", test_auth_header())
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["headers"].as_array().unwrap().len(), 2);
    assert!(body["rows"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn test_csv_proxy_multirow_csv() {
    let state = setup_mock_app_state().await;
    let tenant_id = test_tenant_id();
    let storage = state.dtako_storage.as_ref().unwrap();
    let csv = "name,age,city\nAlice,30,Tokyo\nBob,25,Osaka\nCharlie,35,Nagoya\n";
    upload_csv_to_mock_storage(&**storage, &tenant_id, "6002", "KUDGURI.csv", csv).await;

    let base_url = common::spawn_test_server(state).await;
    let res = reqwest::Client::new()
        .get(format!("{base_url}/api/operations/6002/csv/kudguri"))
        .header("Authorization", test_auth_header())
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    let rows = body["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0][0], "Alice");
    assert_eq!(rows[2][2], "Nagoya");
}

#[tokio::test]
async fn test_csv_proxy_empty_body() {
    let state = setup_mock_app_state().await;
    let tenant_id = test_tenant_id();
    let storage = state.dtako_storage.as_ref().unwrap();
    upload_csv_to_mock_storage(&**storage, &tenant_id, "6003", "KUDGURI.csv", "").await;

    let base_url = common::spawn_test_server(state).await;
    let res = reqwest::Client::new()
        .get(format!("{base_url}/api/operations/6003/csv/kudguri"))
        .header("Authorization", test_auth_header())
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    // empty string -> split(",") -> [""] -> headers has 1 empty element
    assert_eq!(body["headers"].as_array().unwrap().len(), 1);
    assert!(body["rows"].as_array().unwrap().is_empty());
}

// =============================================================
// Error cases
// =============================================================

#[tokio::test]
async fn test_csv_proxy_invalid_type_returns_400() {
    let state = setup_mock_app_state().await;
    let base_url = common::spawn_test_server(state).await;

    let res = reqwest::Client::new()
        .get(format!("{base_url}/api/operations/1001/csv/invalid_type"))
        .header("Authorization", test_auth_header())
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 400);
}

#[tokio::test]
async fn test_csv_proxy_file_not_found_returns_404() {
    let state = setup_mock_app_state().await;
    // MockStorage にファイルを配置しない → download で NotFound
    let base_url = common::spawn_test_server(state).await;

    let res = reqwest::Client::new()
        .get(format!("{base_url}/api/operations/9999/csv/kudguri"))
        .header("Authorization", test_auth_header())
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn test_csv_proxy_no_auth_returns_401() {
    let state = setup_mock_app_state().await;
    let base_url = common::spawn_test_server(state).await;

    let res = reqwest::Client::new()
        .get(format!("{base_url}/api/operations/1001/csv/kudguri"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);
}

#[tokio::test]
async fn test_csv_proxy_db_error_returns_500() {
    let mock_repo = Arc::new(MockDtakoCsvProxyRepository::default());
    mock_repo.fail_next.store(true, Ordering::SeqCst);

    let mut state = setup_mock_app_state().await;
    state.dtako_csv_proxy = mock_repo;

    let base_url = common::spawn_test_server(state).await;

    let res = reqwest::Client::new()
        .get(format!("{base_url}/api/operations/1001/csv/kudguri"))
        .header("Authorization", test_auth_header())
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

#[tokio::test]
async fn test_csv_proxy_no_dtako_storage_returns_500() {
    let mut state = setup_mock_app_state().await;
    state.dtako_storage = None;

    let base_url = common::spawn_test_server(state).await;

    let res = reqwest::Client::new()
        .get(format!("{base_url}/api/operations/1001/csv/kudguri"))
        .header("Authorization", test_auth_header())
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// =============================================================
// Case insensitivity
// =============================================================

#[tokio::test]
async fn test_csv_proxy_case_insensitive() {
    let state = setup_mock_app_state().await;
    let tenant_id = test_tenant_id();
    let storage = state.dtako_storage.as_ref().unwrap();
    upload_csv_to_mock_storage(&**storage, &tenant_id, "7001", "KUDGURI.csv", "x\n1\n").await;
    upload_csv_to_mock_storage(&**storage, &tenant_id, "7002", "KUDGURI.csv", "x\n1\n").await;

    let base_url = common::spawn_test_server(state).await;
    let client = reqwest::Client::new();
    let auth = test_auth_header();

    // KUDGURI (uppercase)
    let res = client
        .get(format!("{base_url}/api/operations/7001/csv/KUDGURI"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    // Kudguri (mixed case)
    let res = client
        .get(format!("{base_url}/api/operations/7002/csv/Kudguri"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
}
