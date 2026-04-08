use std::sync::atomic::Ordering;
use std::sync::Arc;

use crate::mock_helpers::app_state::setup_mock_app_state;
use crate::mock_helpers::MockDtakoLogsRepository;

// =============================================================================
// GET /api/dtako-logs/current
// =============================================================================

#[tokio::test]
async fn current_list_all_success_returns_empty() {
    let state = setup_mock_app_state();
    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();
    let tenant_id = uuid::Uuid::new_v4();
    let token = crate::common::create_test_jwt(tenant_id, "admin");

    let res = client
        .get(format!("{base_url}/api/dtako-logs/current"))
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: Vec<serde_json::Value> = res.json().await.unwrap();
    assert!(body.is_empty());
}

#[tokio::test]
async fn current_list_all_no_auth_returns_401() {
    let state = setup_mock_app_state();
    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/dtako-logs/current"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 401);
}

#[tokio::test]
async fn current_list_all_db_error_returns_500() {
    let mock = Arc::new(MockDtakoLogsRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);

    let mut state = setup_mock_app_state();
    state.dtako_logs = mock;

    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();
    let tenant_id = uuid::Uuid::new_v4();
    let token = crate::common::create_test_jwt(tenant_id, "admin");

    let res = client
        .get(format!("{base_url}/api/dtako-logs/current"))
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

#[tokio::test]
async fn current_list_all_with_tenant_header_returns_200() {
    let state = setup_mock_app_state();
    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();
    let tenant_id = uuid::Uuid::new_v4();

    let res = client
        .get(format!("{base_url}/api/dtako-logs/current"))
        .header("X-Tenant-ID", tenant_id.to_string())
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
}

// =============================================================================
// GET /api/dtako-logs/by-date
// =============================================================================

#[tokio::test]
async fn get_by_date_success() {
    let state = setup_mock_app_state();
    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();
    let tenant_id = uuid::Uuid::new_v4();
    let token = crate::common::create_test_jwt(tenant_id, "admin");

    let res = client
        .get(format!(
            "{base_url}/api/dtako-logs/by-date?date_time=26/04/04%2010:00"
        ))
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: Vec<serde_json::Value> = res.json().await.unwrap();
    assert!(body.is_empty());
}

#[tokio::test]
async fn get_by_date_with_vehicle_cd() {
    let state = setup_mock_app_state();
    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();
    let tenant_id = uuid::Uuid::new_v4();
    let token = crate::common::create_test_jwt(tenant_id, "admin");

    let res = client
        .get(format!(
            "{base_url}/api/dtako-logs/by-date?date_time=26/04/04%2010:00&vehicle_cd=123"
        ))
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
}

#[tokio::test]
async fn get_by_date_no_auth_returns_401() {
    let state = setup_mock_app_state();
    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!(
            "{base_url}/api/dtako-logs/by-date?date_time=26/04/04%2010:00"
        ))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 401);
}

#[tokio::test]
async fn get_by_date_db_error_returns_500() {
    let mock = Arc::new(MockDtakoLogsRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);

    let mut state = setup_mock_app_state();
    state.dtako_logs = mock;

    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();
    let tenant_id = uuid::Uuid::new_v4();
    let token = crate::common::create_test_jwt(tenant_id, "admin");

    let res = client
        .get(format!(
            "{base_url}/api/dtako-logs/by-date?date_time=26/04/04%2010:00"
        ))
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

// =============================================================================
// GET /api/dtako-logs/current/select
// =============================================================================

#[tokio::test]
async fn current_list_select_success() {
    let state = setup_mock_app_state();
    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();
    let tenant_id = uuid::Uuid::new_v4();
    let token = crate::common::create_test_jwt(tenant_id, "admin");

    let res = client
        .get(format!("{base_url}/api/dtako-logs/current/select"))
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: Vec<serde_json::Value> = res.json().await.unwrap();
    assert!(body.is_empty());
}

#[tokio::test]
async fn current_list_select_with_filters() {
    let state = setup_mock_app_state();
    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();
    let tenant_id = uuid::Uuid::new_v4();
    let token = crate::common::create_test_jwt(tenant_id, "admin");

    let res = client
        .get(format!(
            "{base_url}/api/dtako-logs/current/select?address_disp_p=Tokyo&branch_cd=1&vehicle_cds=10,20,30"
        ))
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
}

#[tokio::test]
async fn current_list_select_no_auth_returns_401() {
    let state = setup_mock_app_state();
    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/dtako-logs/current/select"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 401);
}

#[tokio::test]
async fn current_list_select_db_error_returns_500() {
    let mock = Arc::new(MockDtakoLogsRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);

    let mut state = setup_mock_app_state();
    state.dtako_logs = mock;

    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();
    let tenant_id = uuid::Uuid::new_v4();
    let token = crate::common::create_test_jwt(tenant_id, "admin");

    let res = client
        .get(format!("{base_url}/api/dtako-logs/current/select"))
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

// =============================================================================
// GET /api/dtako-logs/by-date-range
// =============================================================================

#[tokio::test]
async fn get_by_date_range_success() {
    let state = setup_mock_app_state();
    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();
    let tenant_id = uuid::Uuid::new_v4();
    let token = crate::common::create_test_jwt(tenant_id, "admin");

    let res = client
        .get(format!(
            "{base_url}/api/dtako-logs/by-date-range?start_date_time=2026-04-01T00:00:00Z&end_date_time=2026-04-04T23:59:59Z"
        ))
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: Vec<serde_json::Value> = res.json().await.unwrap();
    assert!(body.is_empty());
}

#[tokio::test]
async fn get_by_date_range_with_vehicle_cd() {
    let state = setup_mock_app_state();
    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();
    let tenant_id = uuid::Uuid::new_v4();
    let token = crate::common::create_test_jwt(tenant_id, "admin");

    let res = client
        .get(format!(
            "{base_url}/api/dtako-logs/by-date-range?start_date_time=2026-04-01T00:00:00Z&end_date_time=2026-04-04T23:59:59Z&vehicle_cd=42"
        ))
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
}

#[tokio::test]
async fn get_by_date_range_no_auth_returns_401() {
    let state = setup_mock_app_state();
    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!(
            "{base_url}/api/dtako-logs/by-date-range?start_date_time=2026-04-01T00:00:00Z&end_date_time=2026-04-04T23:59:59Z"
        ))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 401);
}

#[tokio::test]
async fn get_by_date_range_db_error_returns_500() {
    let mock = Arc::new(MockDtakoLogsRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);

    let mut state = setup_mock_app_state();
    state.dtako_logs = mock;

    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();
    let tenant_id = uuid::Uuid::new_v4();
    let token = crate::common::create_test_jwt(tenant_id, "admin");

    let res = client
        .get(format!(
            "{base_url}/api/dtako-logs/by-date-range?start_date_time=2026-04-01T00:00:00Z&end_date_time=2026-04-04T23:59:59Z"
        ))
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

// =============================================================================
// POST /api/dtako-logs/bulk
// =============================================================================

fn sample_bulk_body() -> serde_json::Value {
    serde_json::json!([{
        "DataDateTime": "2024-11-28T10:37:00+09:00",
        "VehicleCD": 1,
        "__type": "Vehicle",
        "VehicleName": "Truck-1",
        "DriverName": "Driver A",
        "GPSDirection": 180,
        "GPSLatitude": 35123456,
        "GPSLongitude": 139123456,
        "Speed": 60.5,
        "AllState": "Drive",
        "AddressDispP": "Shibuya"
    }])
}

#[tokio::test]
async fn bulk_upsert_success() {
    let state = setup_mock_app_state();
    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();
    let tenant_id = uuid::Uuid::new_v4();
    let token = crate::common::create_test_jwt(tenant_id, "admin");

    let res = client
        .post(format!("{base_url}/api/dtako-logs/bulk"))
        .header("Authorization", format!("Bearer {token}"))
        .json(&sample_bulk_body())
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["success"], true);
    assert_eq!(body["records_added"], 1);
    assert_eq!(body["total_records"], 1);
}

#[tokio::test]
async fn bulk_upsert_empty_array() {
    let state = setup_mock_app_state();
    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();
    let tenant_id = uuid::Uuid::new_v4();
    let token = crate::common::create_test_jwt(tenant_id, "admin");

    let res = client
        .post(format!("{base_url}/api/dtako-logs/bulk"))
        .header("Authorization", format!("Bearer {token}"))
        .json(&serde_json::json!([]))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["success"], true);
    assert_eq!(body["records_added"], 0);
    assert_eq!(body["total_records"], 0);
}

#[tokio::test]
async fn bulk_upsert_no_auth_returns_401() {
    let state = setup_mock_app_state();
    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/dtako-logs/bulk"))
        .json(&sample_bulk_body())
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 401);
}

#[tokio::test]
async fn bulk_upsert_db_error_returns_500() {
    let mock = Arc::new(MockDtakoLogsRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);

    let mut state = setup_mock_app_state();
    state.dtako_logs = mock;

    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();
    let tenant_id = uuid::Uuid::new_v4();
    let token = crate::common::create_test_jwt(tenant_id, "admin");

    let res = client
        .post(format!("{base_url}/api/dtako-logs/bulk"))
        .header("Authorization", format!("Bearer {token}"))
        .json(&sample_bulk_body())
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

#[tokio::test]
async fn bulk_upsert_with_tenant_header_returns_200() {
    let state = setup_mock_app_state();
    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();
    let tenant_id = uuid::Uuid::new_v4();

    let res = client
        .post(format!("{base_url}/api/dtako-logs/bulk"))
        .header("X-Tenant-ID", tenant_id.to_string())
        .json(&sample_bulk_body())
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["success"], true);
    assert_eq!(body["records_added"], 1);
}

// =============================================================================
// GET /api/dtako-logs/by-date-range — R2 archive dedup/sort + error fallback
// =============================================================================

use rust_alc_api::db::models::DtakologRow;

fn make_dtakolog_row(datetime: &str, vehicle_cd: i32) -> DtakologRow {
    DtakologRow {
        gps_direction: 0.0,
        gps_latitude: 35.0,
        gps_longitude: 139.0,
        vehicle_cd,
        vehicle_name: "Truck-1".to_string(),
        driver_name: Some("Driver A".to_string()),
        address_disp_c: None,
        data_date_time: datetime.to_string(),
        address_disp_p: None,
        sub_driver_cd: 0,
        all_state: None,
        recive_type_color_name: None,
        all_state_ex: None,
        state2: None,
        all_state_font_color: None,
        speed: 60.0,
    }
}

fn create_r2_manifest_and_archive(
    tenant_id: &str,
    date: &str,
    rows_json: &[serde_json::Value],
) -> (Vec<u8>, String, Vec<u8>) {
    use flate2::write::GzEncoder;
    use flate2::Compression;
    use std::io::Write;

    let r2_key = format!("archive/alc_api/dtakologs/{tenant_id}/{date}.jsonl.gz");

    // Build manifest
    let manifest = serde_json::json!({
        "archived_dates": {
            tenant_id: {
                date: {
                    "row_count": rows_json.len(),
                    "r2_key": r2_key,
                    "archived_at": "2026-04-08T00:00:00Z"
                }
            }
        }
    });
    let manifest_bytes = serde_json::to_vec(&manifest).unwrap();

    // Build gzipped JSONL
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    for row in rows_json {
        let line = serde_json::to_string(row).unwrap();
        writeln!(encoder, "{}", line).unwrap();
    }
    let compressed = encoder.finish().unwrap();

    (manifest_bytes, r2_key, compressed)
}

#[tokio::test]
async fn get_by_date_range_r2_dedup_and_sort() {
    // DB returns a row at 10:00, R2 returns the same + one at 08:00.
    // Result should be sorted and deduplicated.
    let tenant_id = uuid::Uuid::new_v4();

    let db_row = make_dtakolog_row("2026-04-05T10:00:00", 1);

    let r2_rows_json = vec![
        serde_json::json!({
            "data_date_time": "2026-04-05T08:00:00",
            "vehicle_cd": 1,
            "vehicle_name": "Truck-1",
            "gps_direction": 0.0,
            "gps_latitude": 35.0,
            "gps_longitude": 139.0,
            "speed": 50.0,
            "sub_driver_cd": 0
        }),
        serde_json::json!({
            "data_date_time": "2026-04-05T10:00:00",
            "vehicle_cd": 1,
            "vehicle_name": "Truck-1",
            "gps_direction": 0.0,
            "gps_latitude": 35.0,
            "gps_longitude": 139.0,
            "speed": 60.0,
            "sub_driver_cd": 0
        }),
    ];

    let (manifest_bytes, r2_key, compressed) =
        create_r2_manifest_and_archive(&tenant_id.to_string(), "2026-04-05", &r2_rows_json);

    let dtako_storage = Arc::new(crate::common::mock_storage::MockStorage::new(
        "dtako-bucket",
    ));
    dtako_storage.insert_file("archive/alc_api/dtakologs/_manifest.json", manifest_bytes);
    dtako_storage.insert_file(&r2_key, compressed);

    let mock_logs = Arc::new(MockDtakoLogsRepository::default());
    *mock_logs.return_date_range.lock().unwrap() = vec![db_row];

    let mut state = setup_mock_app_state();
    state.dtako_logs = mock_logs;
    state.dtako_storage = Some(dtako_storage as Arc<dyn rust_alc_api::storage::StorageBackend>);

    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();
    let token = crate::common::create_test_jwt(tenant_id, "admin");

    let res = client
        .get(format!(
            "{base_url}/api/dtako-logs/by-date-range?start_date_time=2026-04-05T00:00:00&end_date_time=2026-04-05T23:59:59"
        ))
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: Vec<serde_json::Value> = res.json().await.unwrap();
    // 2 unique rows (08:00 from R2 + 10:00 deduplicated)
    assert_eq!(body.len(), 2);
    // Sorted by data_date_time
    let dt0 = body[0]["DataDateTime"].as_str().unwrap();
    let dt1 = body[1]["DataDateTime"].as_str().unwrap();
    assert!(dt0 < dt1, "should be sorted: {dt0} < {dt1}");
}

#[tokio::test]
async fn get_by_date_range_r2_corrupted_manifest() {
    // Corrupted manifest → unwrap_or_default() in archive_reader → empty result
    // → only DB rows returned (covers the R2 Ok-but-empty path)
    let tenant_id = uuid::Uuid::new_v4();

    let db_row = make_dtakolog_row("2026-04-05T10:00:00", 1);

    let dtako_storage = Arc::new(crate::common::mock_storage::MockStorage::new(
        "dtako-bucket",
    ));
    dtako_storage.insert_file(
        "archive/alc_api/dtakologs/_manifest.json",
        b"not-valid-json!!!".to_vec(),
    );

    let mock_logs = Arc::new(MockDtakoLogsRepository::default());
    *mock_logs.return_date_range.lock().unwrap() = vec![db_row];

    let mut state = setup_mock_app_state();
    state.dtako_logs = mock_logs;
    state.dtako_storage = Some(dtako_storage as Arc<dyn rust_alc_api::storage::StorageBackend>);

    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();
    let token = crate::common::create_test_jwt(tenant_id, "admin");

    let res = client
        .get(format!(
            "{base_url}/api/dtako-logs/by-date-range?start_date_time=2026-04-05T00:00:00&end_date_time=2026-04-05T23:59:59"
        ))
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: Vec<serde_json::Value> = res.json().await.unwrap();
    assert_eq!(body.len(), 1);
}

#[tokio::test]
async fn get_by_date_range_r2_decompress_error() {
    // R2 archive file is not valid gzip → decompression fails in archive_reader
    // → logs warning → continues → dtako_logs gets Ok(empty) from R2
    // → only DB results returned
    let tenant_id = uuid::Uuid::new_v4();

    let db_row = make_dtakolog_row("2026-04-05T10:00:00", 1);

    let r2_key = format!(
        "archive/alc_api/dtakologs/{}/2026-04-05.jsonl.gz",
        tenant_id
    );
    let manifest = serde_json::json!({
        "archived_dates": {
            tenant_id.to_string(): {
                "2026-04-05": {
                    "row_count": 1,
                    "r2_key": r2_key,
                    "archived_at": "2026-04-08T00:00:00Z"
                }
            }
        }
    });

    let dtako_storage = Arc::new(crate::common::mock_storage::MockStorage::new(
        "dtako-bucket",
    ));
    dtako_storage.insert_file(
        "archive/alc_api/dtakologs/_manifest.json",
        serde_json::to_vec(&manifest).unwrap(),
    );
    // Insert invalid gzip data
    dtako_storage.insert_file(&r2_key, b"not-gzip-data".to_vec());

    let mock_logs = Arc::new(MockDtakoLogsRepository::default());
    *mock_logs.return_date_range.lock().unwrap() = vec![db_row];

    let mut state = setup_mock_app_state();
    state.dtako_logs = mock_logs;
    state.dtako_storage = Some(dtako_storage as Arc<dyn rust_alc_api::storage::StorageBackend>);

    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();
    let token = crate::common::create_test_jwt(tenant_id, "admin");

    let res = client
        .get(format!(
            "{base_url}/api/dtako-logs/by-date-range?start_date_time=2026-04-05T00:00:00&end_date_time=2026-04-05T23:59:59"
        ))
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: Vec<serde_json::Value> = res.json().await.unwrap();
    // R2 decompression fails → archive_reader skips that file → returns Ok(empty)
    // → only DB row returned
    assert_eq!(body.len(), 1);
}

#[tokio::test]
async fn get_by_date_range_no_dtako_storage() {
    // dtako_storage is None → R2 path is skipped entirely
    let tenant_id = uuid::Uuid::new_v4();

    let db_row = make_dtakolog_row("2026-04-05T10:00:00", 1);

    let mock_logs = Arc::new(MockDtakoLogsRepository::default());
    *mock_logs.return_date_range.lock().unwrap() = vec![db_row];

    let mut state = setup_mock_app_state();
    state.dtako_logs = mock_logs;
    state.dtako_storage = None;

    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();
    let token = crate::common::create_test_jwt(tenant_id, "admin");

    let res = client
        .get(format!(
            "{base_url}/api/dtako-logs/by-date-range?start_date_time=2026-04-05T00:00:00&end_date_time=2026-04-05T23:59:59"
        ))
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: Vec<serde_json::Value> = res.json().await.unwrap();
    assert_eq!(body.len(), 1);
}
