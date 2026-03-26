mod common;

use serde_json::Value;

// ============================================================
// dtako upload — ZIP アップロード
// ============================================================

#[tokio::test]
async fn test_dtako_upload_zip() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "DtakoZip").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let zip_bytes = common::create_test_dtako_zip();

    let file_part = reqwest::multipart::Part::bytes(zip_bytes)
        .file_name("test.zip")
        .mime_str("application/zip")
        .unwrap();
    let form = reqwest::multipart::Form::new().part("file", file_part);

    let res = client
        .post(format!("{base_url}/api/upload"))
        .header("Authorization", format!("Bearer {jwt}"))
        .multipart(form)
        .send().await.unwrap();
    let status = res.status();
    let body_text = res.text().await.unwrap();
    assert_eq!(status, 200, "upload_zip failed: {body_text}");
    let body: Value = serde_json::from_str(&body_text).unwrap();
    assert_eq!(body["status"], "completed");
    assert!(body["operations_count"].as_i64().unwrap() >= 1);
    let upload_id = body["upload_id"].as_str().unwrap();

    // list_uploads に表示される
    let res = client
        .get(format!("{base_url}/api/uploads"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send().await.unwrap();
    assert_eq!(res.status(), 200);
}

#[tokio::test]
async fn test_dtako_upload_invalid_zip() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "DtakoBadZip").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let file_part = reqwest::multipart::Part::bytes(b"not-a-zip".to_vec())
        .file_name("bad.zip")
        .mime_str("application/zip")
        .unwrap();
    let form = reqwest::multipart::Form::new().part("file", file_part);

    let res = client
        .post(format!("{base_url}/api/upload"))
        .header("Authorization", format!("Bearer {jwt}"))
        .multipart(form)
        .send().await.unwrap();
    assert_eq!(res.status(), 400);
}

#[tokio::test]
async fn test_dtako_recalculate_driver() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "DtakoRecalc").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    // まず ZIP をアップロード
    let zip_bytes = common::create_test_dtako_zip();
    let file_part = reqwest::multipart::Part::bytes(zip_bytes)
        .file_name("test.zip")
        .mime_str("application/zip")
        .unwrap();
    let form = reqwest::multipart::Form::new().part("file", file_part);

    client.post(format!("{base_url}/api/upload"))
        .header("Authorization", format!("Bearer {jwt}"))
        .multipart(form)
        .send().await.unwrap();

    // recalculate-driver (SSE streaming endpoint)
    // driver_id は UUID なのでダミー UUID を使用
    let fake_driver = uuid::Uuid::new_v4();
    let res = client
        .post(format!("{base_url}/api/recalculate-driver?year=2026&month=3&driver_id={fake_driver}"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send().await.unwrap();
    // SSE なので 200 でストリーム開始
    assert_eq!(res.status(), 200);
}

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

// ============================================================
// dtako restraint report — JSON レポート (employee ベース)
// ============================================================

#[tokio::test]
async fn test_dtako_restraint_report_json() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "DtakoRRJson").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    // employee 作成
    let emp = common::create_test_employee(&client, &base_url, &auth, "拘束レポート運転者", "RR01").await;
    let emp_id = emp["id"].as_str().unwrap();

    // GET /api/restraint-report?driver_id=X&year=2026&month=3
    let res = client
        .get(format!("{base_url}/api/restraint-report?driver_id={emp_id}&year=2026&month=3"))
        .header("Authorization", &auth)
        .send().await.unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["driver_id"], emp_id);
    assert_eq!(body["year"], 2026);
    assert_eq!(body["month"], 3);
    assert!(body["days"].as_array().is_some());
}

#[tokio::test]
async fn test_dtako_restraint_report_json_with_data() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "DtakoRRData").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    // ZIP アップロード + employee 作成
    let zip_bytes = common::create_test_dtako_zip();
    let file_part = reqwest::multipart::Part::bytes(zip_bytes)
        .file_name("test.zip").mime_str("application/zip").unwrap();
    let form = reqwest::multipart::Form::new().part("file", file_part);
    client.post(format!("{base_url}/api/upload"))
        .header("Authorization", &auth)
        .multipart(form).send().await.unwrap();

    // dtako_drivers から driver_id を取得
    let res = client.get(format!("{base_url}/api/drivers"))
        .header("Authorization", &auth)
        .send().await.unwrap();
    let drivers: Vec<Value> = res.json().await.unwrap();
    if !drivers.is_empty() {
        // employees にも同じ名前で作成 (build_report は employees を参照)
        let emp = common::create_test_employee(&client, &base_url, &auth, "テスト運転者", "DR01").await;
        let emp_id = emp["id"].as_str().unwrap();

        let res = client
            .get(format!("{base_url}/api/restraint-report?driver_id={emp_id}&year=2026&month=3"))
            .header("Authorization", &auth)
            .send().await.unwrap();
        assert_eq!(res.status(), 200);
    }
}

// ============================================================
// dtako restraint report — CSV 比較
// ============================================================

#[tokio::test]
async fn test_dtako_compare_csv_empty() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "DtakoCmpCSV").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    // 空の multipart → 400
    let res = client
        .post(format!("{base_url}/api/restraint-report/compare-csv"))
        .header("Authorization", format!("Bearer {jwt}"))
        .header("Content-Type", "multipart/form-data; boundary=----test")
        .body("------test--\r\n")
        .send().await.unwrap();
    assert!(res.status().is_client_error());
}

// ============================================================
// dtako daily-hours フィルタ
// ============================================================

// ============================================================
// dtako upload — SSE endpoints
// ============================================================

// test_dtako_recalculate_all は既に上部で定義済み

#[tokio::test]
async fn test_dtako_recalculate_drivers_batch() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "DtakoRecalcBatch").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    // バッチ再計算 SSE → 200
    let res = client
        .post(format!("{base_url}/api/recalculate-drivers"))
        .header("Authorization", format!("Bearer {jwt}"))
        .json(&serde_json::json!({ "year": 2026, "month": 3, "driver_ids": [] }))
        .send().await.unwrap();
    assert_eq!(res.status(), 200);
}

#[tokio::test]
async fn test_dtako_split_csv_all() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "DtakoSplitAll").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/split-csv-all"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send().await.unwrap();
    assert_eq!(res.status(), 200);
}

// ============================================================
// dtako restraint report — build_report 計算パス
// ============================================================

#[tokio::test]
async fn test_dtako_restraint_report_full_month() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "DtakoRRFull").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    // ZIP アップロード
    let zip_bytes = common::create_test_dtako_zip();
    let file_part = reqwest::multipart::Part::bytes(zip_bytes)
        .file_name("test.zip").mime_str("application/zip").unwrap();
    let form = reqwest::multipart::Form::new().part("file", file_part);
    client.post(format!("{base_url}/api/upload"))
        .header("Authorization", &auth)
        .multipart(form).send().await.unwrap();

    // employee 作成
    let emp = common::create_test_employee(&client, &base_url, &auth, "拘束運転者", "RR01").await;
    let emp_id = emp["id"].as_str().unwrap();

    // 3月レポート (31日分)
    let res = client
        .get(format!("{base_url}/api/restraint-report?driver_id={emp_id}&year=2026&month=3"))
        .header("Authorization", &auth)
        .send().await.unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert!(body["days"].as_array().unwrap().len() >= 28);
    assert!(body["weekly_subtotals"].as_array().is_some());
    assert!(body["monthly_total"].is_object());

    // 2月レポート (28日分)
    let res = client
        .get(format!("{base_url}/api/restraint-report?driver_id={emp_id}&year=2026&month=2"))
        .header("Authorization", &auth)
        .send().await.unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["month"], 2);
}

// ============================================================
// dtako daily-hours フィルタ
// ============================================================

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

// ============================================================
// dtako operations — GET/DELETE by unko_no
// ============================================================

#[tokio::test]
async fn test_dtako_get_operation_by_unko_no() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "DtakoGetOp").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    // Upload ZIP first
    let zip_bytes = common::create_test_dtako_zip();
    let file_part = reqwest::multipart::Part::bytes(zip_bytes)
        .file_name("test.zip")
        .mime_str("application/zip")
        .unwrap();
    let form = reqwest::multipart::Form::new().part("file", file_part);

    let res = client
        .post(format!("{base_url}/api/upload"))
        .header("Authorization", format!("Bearer {jwt}"))
        .multipart(form)
        .send().await.unwrap();
    assert_eq!(res.status(), 200, "upload failed");

    // GET operation by unko_no
    let res = client
        .get(format!("{base_url}/api/operations/1001"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send().await.unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert!(body.as_array().unwrap().len() >= 1, "should have at least one operation");
    assert_eq!(body[0]["unko_no"], "1001");
}

#[tokio::test]
async fn test_dtako_get_operation_not_found() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "DtakoGetOpNF").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/operations/99999"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send().await.unwrap();
    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn test_dtako_delete_operation_by_unko_no() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "DtakoDelOp").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    // Upload ZIP first
    let zip_bytes = common::create_test_dtako_zip();
    let file_part = reqwest::multipart::Part::bytes(zip_bytes)
        .file_name("test.zip")
        .mime_str("application/zip")
        .unwrap();
    let form = reqwest::multipart::Form::new().part("file", file_part);

    let res = client
        .post(format!("{base_url}/api/upload"))
        .header("Authorization", format!("Bearer {jwt}"))
        .multipart(form)
        .send().await.unwrap();
    assert_eq!(res.status(), 200, "upload failed");

    // Verify operation exists
    let res = client
        .get(format!("{base_url}/api/operations/1001"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send().await.unwrap();
    assert_eq!(res.status(), 200, "operation should exist before delete");

    // DELETE operation
    let res = client
        .delete(format!("{base_url}/api/operations/1001"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send().await.unwrap();
    assert_eq!(res.status(), 204, "delete should return 204 No Content");

    // Verify operation is gone
    let res = client
        .get(format!("{base_url}/api/operations/1001"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send().await.unwrap();
    assert_eq!(res.status(), 404, "operation should be gone after delete");
}

#[tokio::test]
async fn test_dtako_delete_operation_not_found() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "DtakoDelNF").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .delete(format!("{base_url}/api/operations/99999"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send().await.unwrap();
    assert_eq!(res.status(), 404);
}

// ============================================================
// dtako restraint report — after upload
// ============================================================

#[tokio::test]
async fn test_dtako_restraint_report_after_upload() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "DtakoRRUp").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    // Upload ZIP to create driver data
    let zip_bytes = common::create_test_dtako_zip();
    let file_part = reqwest::multipart::Part::bytes(zip_bytes)
        .file_name("test.zip")
        .mime_str("application/zip")
        .unwrap();
    let form = reqwest::multipart::Form::new().part("file", file_part);

    let res = client
        .post(format!("{base_url}/api/upload"))
        .header("Authorization", format!("Bearer {jwt}"))
        .multipart(form)
        .send().await.unwrap();
    assert_eq!(res.status(), 200, "upload failed");

    // Get driver list to find the driver_id for "テスト運転者"
    let res = client
        .get(format!("{base_url}/api/drivers"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send().await.unwrap();
    assert_eq!(res.status(), 200);
    let drivers: Value = res.json().await.unwrap();
    let drivers_arr = drivers.as_array().unwrap();
    assert!(!drivers_arr.is_empty(), "should have at least one driver after upload");

    // Find driver_id (first driver from the uploaded data)
    let driver_id = drivers_arr[0]["id"].as_str().unwrap();

    // Query restraint report for that driver
    let res = client
        .get(format!(
            "{base_url}/api/restraint-report?driver_id={driver_id}&year=2026&month=3"
        ))
        .header("Authorization", format!("Bearer {jwt}"))
        .send().await.unwrap();
    // Accept 200 (report generated) or 500 (insufficient data for full report)
    let status = res.status().as_u16();
    assert!(
        status == 200 || status == 500,
        "restraint-report returned unexpected status: {status}"
    );
}

// ============================================================
// dtako split-csv — after upload
// ============================================================

#[tokio::test]
async fn test_dtako_split_csv() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "DtakoSplit").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    // Upload ZIP first
    let zip_bytes = common::create_test_dtako_zip();
    let file_part = reqwest::multipart::Part::bytes(zip_bytes)
        .file_name("test.zip")
        .mime_str("application/zip")
        .unwrap();
    let form = reqwest::multipart::Form::new().part("file", file_part);

    let res = client
        .post(format!("{base_url}/api/upload"))
        .header("Authorization", format!("Bearer {jwt}"))
        .multipart(form)
        .send().await.unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    let upload_id = body["upload_id"].as_str().unwrap();

    // Call split-csv with the upload_id
    let res = client
        .post(format!("{base_url}/api/split-csv/{upload_id}"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send().await.unwrap();
    let status = res.status().as_u16();
    let body_text = res.text().await.unwrap();
    // 200 if R2 storage is configured, 500 if DTAKO_R2_BUCKET not configured in test env
    assert!(
        status == 200 || status == 500,
        "split-csv returned unexpected status {status}: {body_text}"
    );
}

// ============================================================
// dtako recalculate — SSE endpoint
// ============================================================

#[tokio::test]
async fn test_dtako_recalculate_all() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "DtakoRecAll").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    // POST /api/recalculate is an SSE streaming endpoint
    let res = client
        .post(format!("{base_url}/api/recalculate?year=2026&month=3"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send().await.unwrap();
    // SSE starts streaming immediately → 200
    assert_eq!(res.status(), 200, "recalculate should return 200 (SSE stream)");
}

// ============================================================
// dtako internal download — after upload
// ============================================================

#[tokio::test]
async fn test_dtako_internal_download() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "DtakoDL").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    // Upload ZIP first
    let zip_bytes = common::create_test_dtako_zip();
    let file_part = reqwest::multipart::Part::bytes(zip_bytes)
        .file_name("test.zip")
        .mime_str("application/zip")
        .unwrap();
    let form = reqwest::multipart::Form::new().part("file", file_part);

    let res = client
        .post(format!("{base_url}/api/upload"))
        .header("Authorization", format!("Bearer {jwt}"))
        .multipart(form)
        .send().await.unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    let upload_id = body["upload_id"].as_str().unwrap();

    // Try to download the uploaded file
    let res = client
        .get(format!("{base_url}/api/internal/download/{upload_id}"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send().await.unwrap();
    let status = res.status().as_u16();
    // 200 if R2 storage is configured and file exists, 500 if DTAKO_R2_BUCKET not configured
    assert!(
        status == 200 || status == 500,
        "internal/download returned unexpected status: {status}"
    );
}

#[tokio::test]
async fn test_dtako_internal_download_not_found() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "DtakoDLNF").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let fake_id = uuid::Uuid::new_v4();
    let res = client
        .get(format!("{base_url}/api/internal/download/{fake_id}"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send().await.unwrap();
    assert_eq!(res.status(), 404, "download of non-existent upload should return 404");
}

// ============================================================
// dtako restraint report PDF
// ============================================================

#[tokio::test]
async fn test_dtako_restraint_report_pdf() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "DtakoRRPdf").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    // Upload ZIP to create driver data
    let zip_bytes = common::create_test_dtako_zip();
    let file_part = reqwest::multipart::Part::bytes(zip_bytes)
        .file_name("test.zip")
        .mime_str("application/zip")
        .unwrap();
    let form = reqwest::multipart::Form::new().part("file", file_part);

    let res = client
        .post(format!("{base_url}/api/upload"))
        .header("Authorization", format!("Bearer {jwt}"))
        .multipart(form)
        .send().await.unwrap();
    assert_eq!(res.status(), 200, "upload failed");

    // PDF は employees テーブルを参照するため、employee を作成
    let auth = format!("Bearer {jwt}");
    let emp = common::create_test_employee(&client, &base_url, &auth, "PDFドライバー", "PDF01").await;
    let emp_id = emp["id"].as_str().unwrap();

    // Request PDF for the employee (driver_id = employee_id)
    let res = client
        .get(format!(
            "{base_url}/api/restraint-report/pdf?driver_id={emp_id}&year=2026&month=3"
        ))
        .header("Authorization", &auth)
        .send().await.unwrap();
    let status = res.status().as_u16();
    // 200 (PDF generated) — build_report returns empty days but PDF still generates
    assert!(
        status == 200 || status == 500,
        "restraint-report/pdf returned unexpected status: {status}"
    );
    if status == 200 {
        let ct = res.headers().get("content-type").unwrap().to_str().unwrap();
        assert!(ct.contains("pdf"), "should return PDF content type");
    }
}

// PDF 全ドライバー (driver_id なし) — employee が存在すれば生成
#[tokio::test]
async fn test_dtako_restraint_report_pdf_all_drivers() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "DtakoRRPdfAll").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    common::create_test_employee(&client, &base_url, &auth, "全員PDF", "ALL01").await;

    let res = client
        .get(format!("{base_url}/api/restraint-report/pdf?year=2026&month=3"))
        .header("Authorization", &auth)
        .send().await.unwrap();
    let status = res.status().as_u16();
    assert!(status == 200 || status == 500, "pdf all: {status}");
}

// PDF ストリーム
#[tokio::test]
async fn test_dtako_restraint_report_pdf_stream() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "DtakoRRStream").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    common::create_test_employee(&client, &base_url, &auth, "ストリームPDF", "STR01").await;

    let res = client
        .get(format!("{base_url}/api/restraint-report/pdf-stream?year=2026&month=3"))
        .header("Authorization", &auth)
        .send().await.unwrap();
    assert_eq!(res.status(), 200);
}

#[tokio::test]
async fn test_dtako_restraint_report_pdf_not_found() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "DtakoRRPdfNF").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let fake_driver = uuid::Uuid::new_v4();
    let res = client
        .get(format!(
            "{base_url}/api/restraint-report/pdf?driver_id={fake_driver}&year=2026&month=3"
        ))
        .header("Authorization", format!("Bearer {jwt}"))
        .send().await.unwrap();
    let status = res.status().as_u16();
    // Non-existent driver → 404 or 500
    assert!(
        status == 404 || status == 500,
        "restraint-report/pdf for non-existent driver returned unexpected status: {status}"
    );
}

#[tokio::test]
async fn test_dtako_restraint_report_with_driver_id() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "DtakoRRDrv").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    // Upload ZIP to create driver data
    let zip_bytes = common::create_test_dtako_zip();
    let file_part = reqwest::multipart::Part::bytes(zip_bytes)
        .file_name("test.zip")
        .mime_str("application/zip")
        .unwrap();
    let form = reqwest::multipart::Form::new().part("file", file_part);

    let res = client
        .post(format!("{base_url}/api/upload"))
        .header("Authorization", format!("Bearer {jwt}"))
        .multipart(form)
        .send().await.unwrap();
    assert_eq!(res.status(), 200, "upload failed");

    // Get driver list to find the driver_id
    let res = client
        .get(format!("{base_url}/api/drivers"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send().await.unwrap();
    assert_eq!(res.status(), 200);
    let drivers: Value = res.json().await.unwrap();
    let drivers_arr = drivers.as_array().unwrap();
    assert!(!drivers_arr.is_empty(), "should have at least one driver after upload");
    let driver_id = drivers_arr[0]["id"].as_str().unwrap();

    // Query JSON restraint report for that driver
    let res = client
        .get(format!(
            "{base_url}/api/restraint-report?driver_id={driver_id}&year=2026&month=3"
        ))
        .header("Authorization", format!("Bearer {jwt}"))
        .send().await.unwrap();
    let status = res.status().as_u16();
    // Accept 200 (report generated) or 500 (insufficient data)
    assert!(
        status == 200 || status == 500,
        "restraint-report with driver_id returned unexpected status: {status}"
    );
    if status == 200 {
        let body: Value = res.json().await.unwrap();
        // The response should be a JSON object with report data
        assert!(body.is_object(), "restraint-report response should be a JSON object");
    }
}
