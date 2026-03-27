#[macro_use]
mod common;

use serde_json::Value;

// ============================================================
// ヘルパー
// ============================================================

async fn setup() -> (rust_alc_api::AppState, String, uuid::Uuid, String, reqwest::Client) {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "Meas Test").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();
    (state, base_url, tenant_id, jwt, client)
}

async fn setup_with_employee() -> (String, String, String, reqwest::Client) {
    let (_, base_url, _, jwt, client) = setup().await;
    let auth = format!("Bearer {jwt}");
    let emp = common::create_test_employee(&client, &base_url, &auth, "TestEmp", "TE01").await;
    let emp_id = emp["id"].as_str().unwrap().to_string();
    (base_url, auth, emp_id, client)
}

// ============================================================
// 測定作成
// ============================================================

#[tokio::test]
async fn test_create_measurement() {
    test_group!("測定作成");
    test_case!("基本的な測定作成", {
        let (base_url, auth, emp_id, client) = setup_with_employee().await;

        let res = client
            .post(format!("{base_url}/api/measurements"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({
                "employee_id": emp_id,
                "alcohol_value": 0.05,
                "result_type": "pass"
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 201);
        let m: Value = res.json().await.unwrap();
        assert_eq!(m["alcohol_value"], 0.05);
        assert_eq!(m["result_type"], "pass");
        assert_eq!(m["status"], "completed");
        assert_eq!(m["employee_id"], emp_id);
    });
}

#[tokio::test]
async fn test_create_measurement_with_medical() {
    test_group!("測定作成");
    test_case!("医療データ付き測定作成", {
        let (base_url, auth, emp_id, client) = setup_with_employee().await;

        let res = client
            .post(format!("{base_url}/api/measurements"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({
                "employee_id": emp_id,
                "alcohol_value": 0.0,
                "result_type": "pass",
                "temperature": 36.5,
                "systolic": 120,
                "diastolic": 80,
                "pulse": 72,
                "medical_manual_input": true
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 201);
        let m: Value = res.json().await.unwrap();
        assert_eq!(m["temperature"], 36.5);
        assert_eq!(m["systolic"], 120);
        assert_eq!(m["diastolic"], 80);
        assert_eq!(m["pulse"], 72);
        assert_eq!(m["medical_manual_input"], true);
    });
}

#[tokio::test]
async fn test_create_measurement_invalid_result_type() {
    test_group!("測定作成");
    test_case!("無効な result_type で 400 を返す", {
        let (base_url, auth, emp_id, client) = setup_with_employee().await;

        let res = client
            .post(format!("{base_url}/api/measurements"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({
                "employee_id": emp_id,
                "alcohol_value": 0.0,
                "result_type": "invalid"
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 400);
    });
}

#[tokio::test]
async fn test_start_measurement() {
    test_group!("測定作成");
    test_case!("測定開始 (started ステータス)", {
        let (base_url, auth, emp_id, client) = setup_with_employee().await;

        let res = client
            .post(format!("{base_url}/api/measurements/start"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({ "employee_id": emp_id }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 201);
        let m: Value = res.json().await.unwrap();
        assert_eq!(m["status"], "started");
        assert!(m["alcohol_value"].is_null());
    });
}

// ============================================================
// 測定一覧
// ============================================================

#[tokio::test]
async fn test_list_measurements_empty() {
    test_group!("測定一覧");
    test_case!("空の測定一覧", {
        let (base_url, auth, _emp_id, client) = setup_with_employee().await;

        let res = client
            .get(format!("{base_url}/api/measurements"))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        let body: Value = res.json().await.unwrap();
        assert_eq!(body["total"], 0);
        assert_eq!(body["measurements"].as_array().unwrap().len(), 0);
    });
}

#[tokio::test]
async fn test_list_measurements() {
    test_group!("測定一覧");
    test_case!("測定一覧取得", {
        let (base_url, auth, emp_id, client) = setup_with_employee().await;

        common::create_test_measurement(&client, &base_url, &auth, &emp_id).await;
        common::create_test_measurement(&client, &base_url, &auth, &emp_id).await;

        let res = client
            .get(format!("{base_url}/api/measurements"))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        let body: Value = res.json().await.unwrap();
        assert_eq!(body["total"], 2);
        assert_eq!(body["measurements"].as_array().unwrap().len(), 2);
    });
}

#[tokio::test]
async fn test_list_filter_by_employee() {
    test_group!("測定一覧");
    test_case!("従業員でフィルタ", {
        let (_, base_url, _, jwt, client) = setup().await;
        let auth = format!("Bearer {jwt}");

        let emp1 = common::create_test_employee(&client, &base_url, &auth, "Emp1", "E01").await;
        let emp2 = common::create_test_employee(&client, &base_url, &auth, "Emp2", "E02").await;
        let id1 = emp1["id"].as_str().unwrap();
        let id2 = emp2["id"].as_str().unwrap();

        common::create_test_measurement(&client, &base_url, &auth, id1).await;
        common::create_test_measurement(&client, &base_url, &auth, id2).await;
        common::create_test_measurement(&client, &base_url, &auth, id1).await;

        let res = client
            .get(format!("{base_url}/api/measurements?employee_id={id1}"))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();
        let body: Value = res.json().await.unwrap();
        assert_eq!(body["total"], 2);
    });
}

#[tokio::test]
async fn test_list_filter_by_result_type() {
    test_group!("測定一覧");
    test_case!("result_type でフィルタ", {
        let (base_url, auth, emp_id, client) = setup_with_employee().await;

        // pass
        common::create_test_measurement(&client, &base_url, &auth, &emp_id).await;
        // fail
        client
            .post(format!("{base_url}/api/measurements"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({
                "employee_id": emp_id,
                "alcohol_value": 0.3,
                "result_type": "fail"
            }))
            .send()
            .await
            .unwrap();

        let res = client
            .get(format!(
                "{base_url}/api/measurements?result_type=fail"
            ))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();
        let body: Value = res.json().await.unwrap();
        assert_eq!(body["total"], 1);
        assert_eq!(body["measurements"][0]["result_type"], "fail");
    });
}

#[tokio::test]
async fn test_list_filter_by_status() {
    test_group!("測定一覧");
    test_case!("status でフィルタ", {
        let (base_url, auth, emp_id, client) = setup_with_employee().await;

        // started
        client
            .post(format!("{base_url}/api/measurements/start"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({ "employee_id": emp_id }))
            .send()
            .await
            .unwrap();
        // completed
        common::create_test_measurement(&client, &base_url, &auth, &emp_id).await;

        let res = client
            .get(format!("{base_url}/api/measurements?status=started"))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();
        let body: Value = res.json().await.unwrap();
        assert_eq!(body["total"], 1);
        assert_eq!(body["measurements"][0]["status"], "started");
    });
}

#[tokio::test]
async fn test_list_pagination() {
    test_group!("測定一覧");
    test_case!("ページネーション", {
        let (base_url, auth, emp_id, client) = setup_with_employee().await;

        for _ in 0..3 {
            common::create_test_measurement(&client, &base_url, &auth, &emp_id).await;
        }

        let res = client
            .get(format!(
                "{base_url}/api/measurements?page=1&per_page=2"
            ))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();
        let body: Value = res.json().await.unwrap();
        assert_eq!(body["total"], 3);
        assert_eq!(body["measurements"].as_array().unwrap().len(), 2);
        assert_eq!(body["page"], 1);
        assert_eq!(body["per_page"], 2);
    });
}

// ============================================================
// 測定取得・更新
// ============================================================

#[tokio::test]
async fn test_get_measurement() {
    test_group!("測定取得・更新");
    test_case!("測定単体取得", {
        let (base_url, auth, emp_id, client) = setup_with_employee().await;

        let m = common::create_test_measurement(&client, &base_url, &auth, &emp_id).await;
        let id = m["id"].as_str().unwrap();

        let res = client
            .get(format!("{base_url}/api/measurements/{id}"))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        let got: Value = res.json().await.unwrap();
        assert_eq!(got["id"], id);
    });
}

#[tokio::test]
async fn test_get_measurement_not_found() {
    test_group!("測定取得・更新");
    test_case!("存在しない測定で 404", {
        let (base_url, auth, _emp_id, client) = setup_with_employee().await;
        let fake_id = uuid::Uuid::new_v4();

        let res = client
            .get(format!("{base_url}/api/measurements/{fake_id}"))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 404);
    });
}

#[tokio::test]
async fn test_update_measurement() {
    test_group!("測定取得・更新");
    test_case!("測定更新 (部分更新)", {
        let (base_url, auth, emp_id, client) = setup_with_employee().await;

        let m = common::create_test_measurement(&client, &base_url, &auth, &emp_id).await;
        let id = m["id"].as_str().unwrap();

        let res = client
            .put(format!("{base_url}/api/measurements/{id}"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({
                "alcohol_value": 0.15,
                "temperature": 37.0
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        let updated: Value = res.json().await.unwrap();
        assert_eq!(updated["alcohol_value"], 0.15);
        assert_eq!(updated["temperature"], 37.0);
        // 元の値は維持
        assert_eq!(updated["result_type"], "pass");
    });
}

#[tokio::test]
async fn test_update_measurement_complete_flow() {
    test_group!("測定取得・更新");
    test_case!("started → completed フロー", {
        let (base_url, auth, emp_id, client) = setup_with_employee().await;

        // start
        let res = client
            .post(format!("{base_url}/api/measurements/start"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({ "employee_id": emp_id }))
            .send()
            .await
            .unwrap();
        let m: Value = res.json().await.unwrap();
        let id = m["id"].as_str().unwrap();
        assert_eq!(m["status"], "started");

        // update → completed
        let res = client
            .put(format!("{base_url}/api/measurements/{id}"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({
                "status": "completed",
                "alcohol_value": 0.0,
                "result_type": "pass"
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        let updated: Value = res.json().await.unwrap();
        assert_eq!(updated["status"], "completed");
        assert_eq!(updated["result_type"], "pass");
    });
}

#[tokio::test]
async fn test_measurement_tenant_isolation() {
    test_group!("測定取得・更新");
    test_case!("テナント分離", {
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;

        let tenant_a = common::create_test_tenant(&state.pool, "Meas Iso A").await;
        let tenant_b = common::create_test_tenant(&state.pool, "Meas Iso B").await;

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

// ============================================================
// 顔写真・動画プロキシ
// ============================================================

#[tokio::test]
async fn test_measurement_face_photo_no_url() {
    test_group!("顔写真・動画プロキシ");
    test_case!("face_photo_url なしで 404", {
        let (base_url, auth, emp_id, client) = setup_with_employee().await;

        // face_photo_url なしで測定作成
        let m = common::create_test_measurement(&client, &base_url, &auth, &emp_id).await;
        let id = m["id"].as_str().unwrap();

        let res = client
            .get(format!("{base_url}/api/measurements/{id}/face-photo"))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 404);
    });
}

#[tokio::test]
async fn test_measurement_video_no_url() {
    test_group!("顔写真・動画プロキシ");
    test_case!("video_url なしで 404", {
        let (base_url, auth, emp_id, client) = setup_with_employee().await;

        // video_url なしで測定作成
        let m = common::create_test_measurement(&client, &base_url, &auth, &emp_id).await;
        let id = m["id"].as_str().unwrap();

        let res = client
            .get(format!("{base_url}/api/measurements/{id}/video"))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 404);
    });
}

#[tokio::test]
async fn test_measurement_face_photo_not_found() {
    test_group!("顔写真・動画プロキシ");
    test_case!("ストレージにデータなしで 500", {
        let (base_url, auth, emp_id, client) = setup_with_employee().await;

        // face_photo_url 付きで測定作成 (start → update で URL を設定)
        let res = client
            .post(format!("{base_url}/api/measurements/start"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({ "employee_id": emp_id }))
            .send()
            .await
            .unwrap();
        let m: Value = res.json().await.unwrap();
        let id = m["id"].as_str().unwrap();

        // MockStorage の URL 形式に合わせるが、実データは存在しない
        let res = client
            .put(format!("{base_url}/api/measurements/{id}"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({
                "face_photo_url": "https://mock-storage/test-bucket/faces/nonexistent.jpg"
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);

        // ストレージにデータがないので 500
        let res = client
            .get(format!("{base_url}/api/measurements/{id}/face-photo"))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 500);
    });
}

// ============================================================
// 更新バリデーション
// ============================================================

#[tokio::test]
async fn test_update_measurement_invalid_status() {
    test_group!("更新バリデーション");
    test_case!("無効な status で 400", {
        let (base_url, auth, emp_id, client) = setup_with_employee().await;

        let m = common::create_test_measurement(&client, &base_url, &auth, &emp_id).await;
        let id = m["id"].as_str().unwrap();

        let res = client
            .put(format!("{base_url}/api/measurements/{id}"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({ "status": "invalid" }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 400);
    });
}

#[tokio::test]
async fn test_update_measurement_not_found() {
    test_group!("更新バリデーション");
    test_case!("存在しない測定の更新で 404", {
        let (base_url, auth, _emp_id, client) = setup_with_employee().await;
        let fake_id = uuid::Uuid::new_v4();

        let res = client
            .put(format!("{base_url}/api/measurements/{fake_id}"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({ "alcohol_value": 0.1 }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 404);
    });
}

// ============================================================
// 更新 COALESCE
// ============================================================

#[tokio::test]
async fn test_update_measurement_coalesce_partial() {
    test_group!("更新 COALESCE");
    test_case!("部分更新で既存値が保持される", {
        let (base_url, auth, emp_id, client) = setup_with_employee().await;

        // Create with medical data
        let res = client
            .post(format!("{base_url}/api/measurements"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({
                "employee_id": emp_id,
                "alcohol_value": 0.05,
                "result_type": "pass",
                "temperature": 36.5,
                "systolic": 120,
                "diastolic": 80,
                "pulse": 72
            }))
            .send().await.unwrap();
        assert_eq!(res.status(), 201);
        let m: Value = res.json().await.unwrap();
        let id = m["id"].as_str().unwrap();

        // Update only alcohol_value, everything else should be preserved
        let res = client
            .put(format!("{base_url}/api/measurements/{id}"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({ "alcohol_value": 0.10 }))
            .send().await.unwrap();
        assert_eq!(res.status(), 200);
        let updated: Value = res.json().await.unwrap();
        assert_eq!(updated["alcohol_value"], 0.10);
        assert_eq!(updated["result_type"], "pass", "result_type should be preserved");
        assert_eq!(updated["temperature"], 36.5, "temperature should be preserved");
        assert_eq!(updated["systolic"], 120, "systolic should be preserved");
        assert_eq!(updated["diastolic"], 80, "diastolic should be preserved");
        assert_eq!(updated["pulse"], 72, "pulse should be preserved");
    });
}

#[tokio::test]
async fn test_update_measurement_coalesce_medical_only() {
    test_group!("更新 COALESCE");
    test_case!("医療データのみ更新でアルコール値が保持される", {
        let (base_url, auth, emp_id, client) = setup_with_employee().await;

        let m = common::create_test_measurement(&client, &base_url, &auth, &emp_id).await;
        let id = m["id"].as_str().unwrap();

        // Update only temperature
        let res = client
            .put(format!("{base_url}/api/measurements/{id}"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({
                "temperature": 37.2,
                "systolic": 130,
                "diastolic": 85,
                "pulse": 80
            }))
            .send().await.unwrap();
        assert_eq!(res.status(), 200);
        let updated: Value = res.json().await.unwrap();
        assert_eq!(updated["temperature"], 37.2);
        assert_eq!(updated["systolic"], 130);
        assert_eq!(updated["alcohol_value"], 0.0, "alcohol_value should be preserved");
        assert_eq!(updated["result_type"], "pass", "result_type should be preserved");
    });
}

// ============================================================
// 開始→完了フロー
// ============================================================

#[tokio::test]
async fn test_start_then_complete_with_medical() {
    test_group!("開始→完了フロー");
    test_case!("開始→全フィールド更新→完了確認", {
        let (base_url, auth, emp_id, client) = setup_with_employee().await;

        // Start
        let res = client
            .post(format!("{base_url}/api/measurements/start"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({ "employee_id": emp_id }))
            .send().await.unwrap();
        assert_eq!(res.status(), 201);
        let m: Value = res.json().await.unwrap();
        let id = m["id"].as_str().unwrap();
        assert_eq!(m["status"], "started");
        assert!(m["alcohol_value"].is_null());
        assert!(m["temperature"].is_null());

        // Complete with all fields
        let res = client
            .put(format!("{base_url}/api/measurements/{id}"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({
                "status": "completed",
                "alcohol_value": 0.0,
                "result_type": "pass",
                "temperature": 36.3,
                "systolic": 115,
                "diastolic": 75,
                "pulse": 68,
                "medical_manual_input": true,
                "face_verified": true
            }))
            .send().await.unwrap();
        assert_eq!(res.status(), 200);
        let updated: Value = res.json().await.unwrap();
        assert_eq!(updated["status"], "completed");
        assert_eq!(updated["alcohol_value"], 0.0);
        assert_eq!(updated["result_type"], "pass");
        assert_eq!(updated["temperature"], 36.3);
        assert_eq!(updated["systolic"], 115);
        assert_eq!(updated["diastolic"], 75);
        assert_eq!(updated["pulse"], 68);
        assert_eq!(updated["medical_manual_input"], true);
        assert_eq!(updated["face_verified"], true);

        // Verify via GET
        let res = client
            .get(format!("{base_url}/api/measurements/{id}"))
            .header("Authorization", &auth)
            .send().await.unwrap();
        assert_eq!(res.status(), 200);
        let got: Value = res.json().await.unwrap();
        assert_eq!(got["status"], "completed");
        assert_eq!(got["temperature"], 36.3);
    });
}

#[tokio::test]
async fn test_start_then_incremental_updates() {
    test_group!("開始→完了フロー");
    test_case!("開始→段階的更新→値の蓄積確認", {
        let (base_url, auth, emp_id, client) = setup_with_employee().await;

        // Start
        let res = client
            .post(format!("{base_url}/api/measurements/start"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({ "employee_id": emp_id }))
            .send().await.unwrap();
        let m: Value = res.json().await.unwrap();
        let id = m["id"].as_str().unwrap();

        // First update: face photo
        let res = client
            .put(format!("{base_url}/api/measurements/{id}"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({
                "face_photo_url": "https://mock/face.jpg",
                "face_verified": true
            }))
            .send().await.unwrap();
        assert_eq!(res.status(), 200);
        let u1: Value = res.json().await.unwrap();
        assert_eq!(u1["status"], "started", "status unchanged");
        assert_eq!(u1["face_verified"], true);

        // Second update: alcohol
        let res = client
            .put(format!("{base_url}/api/measurements/{id}"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({
                "alcohol_value": 0.0,
                "result_type": "pass"
            }))
            .send().await.unwrap();
        assert_eq!(res.status(), 200);
        let u2: Value = res.json().await.unwrap();
        assert_eq!(u2["face_verified"], true, "face_verified preserved from first update");
        assert_eq!(u2["alcohol_value"], 0.0);

        // Third update: complete
        let res = client
            .put(format!("{base_url}/api/measurements/{id}"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({ "status": "completed" }))
            .send().await.unwrap();
        assert_eq!(res.status(), 200);
        let u3: Value = res.json().await.unwrap();
        assert_eq!(u3["status"], "completed");
        assert_eq!(u3["alcohol_value"], 0.0, "alcohol preserved");
        assert_eq!(u3["face_verified"], true, "face_verified preserved");
    });
}

#[tokio::test]
async fn test_update_measurement_invalid_result_type() {
    test_group!("開始→完了フロー");
    test_case!("無効な result_type で更新すると 400", {
        let (base_url, auth, emp_id, client) = setup_with_employee().await;

        let m = common::create_test_measurement(&client, &base_url, &auth, &emp_id).await;
        let id = m["id"].as_str().unwrap();

        let res = client
            .put(format!("{base_url}/api/measurements/{id}"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({ "result_type": "unknown" }))
            .send().await.unwrap();
        assert_eq!(res.status(), 400);
    });
}

#[tokio::test]
async fn test_update_measurement_video_url() {
    test_group!("開始→完了フロー");
    test_case!("video_url の更新と保持確認", {
        let (base_url, auth, emp_id, client) = setup_with_employee().await;

        let res = client
            .post(format!("{base_url}/api/measurements/start"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({ "employee_id": emp_id }))
            .send().await.unwrap();
        let m: Value = res.json().await.unwrap();
        let id = m["id"].as_str().unwrap();

        let res = client
            .put(format!("{base_url}/api/measurements/{id}"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({
                "video_url": "https://mock/video.mp4",
                "status": "completed",
                "alcohol_value": 0.0,
                "result_type": "pass"
            }))
            .send().await.unwrap();
        assert_eq!(res.status(), 200);
        let updated: Value = res.json().await.unwrap();
        assert_eq!(updated["video_url"], "https://mock/video.mp4");

        // Verify via GET
        let res = client
            .get(format!("{base_url}/api/measurements/{id}"))
            .header("Authorization", &auth)
            .send().await.unwrap();
        let got: Value = res.json().await.unwrap();
        assert_eq!(got["video_url"], "https://mock/video.mp4");
    });
}
