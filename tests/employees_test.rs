mod common;

use serde_json::Value;

/// RLS テナント分離テスト: テナント A の従業員がテナント B から見えないこと
#[tokio::test]
async fn test_tenant_isolation() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;

    let tenant_a = common::create_test_tenant(&state.pool, "Tenant A").await;
    let tenant_b = common::create_test_tenant(&state.pool, "Tenant B").await;

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

/// X-Tenant-ID ヘッダーによるキオスクモード認証テスト
#[tokio::test]
async fn test_kiosk_mode_with_tenant_header() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;

    let tenant_id = common::create_test_tenant(&state.pool, "Kiosk Tenant").await;

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

// ============================================================
// CRUD テスト
// ============================================================

#[tokio::test]
async fn test_create_employee() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "Create Emp").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/employees"))
        .header("Authorization", format!("Bearer {jwt}"))
        .json(&serde_json::json!({ "name": "Taro", "code": "T001" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 201);
    let emp: Value = res.json().await.unwrap();
    assert_eq!(emp["name"], "Taro");
    assert_eq!(emp["code"], "T001");
    assert!(emp["id"].as_str().is_some());
    assert_eq!(emp["tenant_id"], tenant_id.to_string());
}

#[tokio::test]
async fn test_create_employee_with_optional_fields() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "Opt Fields").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/employees"))
        .header("Authorization", format!("Bearer {jwt}"))
        .json(&serde_json::json!({
            "name": "Jiro",
            "code": "J001",
            "nfc_id": "NFC-123",
            "role": ["driver", "manager"]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 201);
    let emp: Value = res.json().await.unwrap();
    assert_eq!(emp["nfc_id"], "NFC-123");
    let roles = emp["role"].as_array().unwrap();
    assert!(roles.contains(&Value::String("driver".into())));
    assert!(roles.contains(&Value::String("manager".into())));
}

#[tokio::test]
async fn test_list_employees_empty() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "Empty List").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/employees"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let employees: Vec<Value> = res.json().await.unwrap();
    assert_eq!(employees.len(), 0);
}

#[tokio::test]
async fn test_list_employees_returns_created() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "List Emp").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    common::create_test_employee(&client, &base_url, &auth, "Alice", "A01").await;
    common::create_test_employee(&client, &base_url, &auth, "Bob", "B01").await;

    let res = client
        .get(format!("{base_url}/api/employees"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let employees: Vec<Value> = res.json().await.unwrap();
    assert_eq!(employees.len(), 2);
    // ORDER BY name
    assert_eq!(employees[0]["name"], "Alice");
    assert_eq!(employees[1]["name"], "Bob");
}

#[tokio::test]
async fn test_get_employee_by_id() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "Get Emp").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    let emp = common::create_test_employee(&client, &base_url, &auth, "Taro", "T01").await;
    let id = emp["id"].as_str().unwrap();

    let res = client
        .get(format!("{base_url}/api/employees/{id}"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let got: Value = res.json().await.unwrap();
    assert_eq!(got["name"], "Taro");
}

#[tokio::test]
async fn test_get_employee_not_found() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "Not Found").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let fake_id = uuid::Uuid::new_v4();
    let res = client
        .get(format!("{base_url}/api/employees/{fake_id}"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn test_update_employee() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "Update Emp").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    let emp = common::create_test_employee(&client, &base_url, &auth, "OldName", "OLD1").await;
    let id = emp["id"].as_str().unwrap();

    let res = client
        .put(format!("{base_url}/api/employees/{id}"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({ "name": "NewName", "code": "NEW1" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let updated: Value = res.json().await.unwrap();
    assert_eq!(updated["name"], "NewName");
    assert_eq!(updated["code"], "NEW1");
}

#[tokio::test]
async fn test_delete_employee() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "Delete Emp").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    let emp = common::create_test_employee(&client, &base_url, &auth, "ToDelete", "DEL1").await;
    let id = emp["id"].as_str().unwrap();

    let res = client
        .delete(format!("{base_url}/api/employees/{id}"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 204);

    // GET → 404 (soft deleted)
    let res = client
        .get(format!("{base_url}/api/employees/{id}"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn test_delete_employee_not_found() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "Del NF").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let fake_id = uuid::Uuid::new_v4();
    let res = client
        .delete(format!("{base_url}/api/employees/{fake_id}"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn test_get_employee_by_nfc() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "NFC Emp").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    // nfc_id 付きで作成
    client
        .post(format!("{base_url}/api/employees"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({ "name": "NfcUser", "code": "NFC1", "nfc_id": "AA:BB:CC:DD" }))
        .send()
        .await
        .unwrap();

    let res = client
        .get(format!("{base_url}/api/employees/by-nfc/AA:BB:CC:DD"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let emp: Value = res.json().await.unwrap();
    assert_eq!(emp["name"], "NfcUser");
}

#[tokio::test]
async fn test_get_employee_by_code() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "Code Emp").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    common::create_test_employee(&client, &base_url, &auth, "CodeUser", "MYCODE").await;

    let res = client
        .get(format!("{base_url}/api/employees/by-code/MYCODE"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let emp: Value = res.json().await.unwrap();
    assert_eq!(emp["name"], "CodeUser");
}

// ============================================================
// Face / NFC / License 更新
// ============================================================

#[tokio::test]
async fn test_update_nfc_id() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "NfcUpdate").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    let emp = common::create_test_employee(&client, &base_url, &auth, "NfcUpd", "NU01").await;
    let id = emp["id"].as_str().unwrap();

    let res = client
        .put(format!("{base_url}/api/employees/{id}/nfc"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({ "nfc_id": "NEW-NFC-ID" }))
        .send().await.unwrap();
    assert_eq!(res.status(), 200);
    let updated: Value = res.json().await.unwrap();
    assert_eq!(updated["nfc_id"], "NEW-NFC-ID");
}

#[tokio::test]
async fn test_update_license() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "LicUpdate").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    let emp = common::create_test_employee(&client, &base_url, &auth, "LicUpd", "LU01").await;
    let id = emp["id"].as_str().unwrap();

    let res = client
        .put(format!("{base_url}/api/employees/{id}/license"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "license_issue_date": "2020-01-01",
            "license_expiry_date": "2030-01-01"
        }))
        .send().await.unwrap();
    assert_eq!(res.status(), 200);
}

#[tokio::test]
async fn test_update_face_invalid_embedding() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "FaceInv").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    let emp = common::create_test_employee(&client, &base_url, &auth, "FaceInv", "FI01").await;
    let id = emp["id"].as_str().unwrap();

    // 1024次元でない embedding → 400
    let res = client
        .put(format!("{base_url}/api/employees/{id}/face"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "face_embedding": [0.1, 0.2, 0.3]
        }))
        .send().await.unwrap();
    assert_eq!(res.status(), 400);
}

#[tokio::test]
async fn test_update_face_and_approve() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "FaceApprove").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    let emp = common::create_test_employee(&client, &base_url, &auth, "FaceApp", "FA01").await;
    let id = emp["id"].as_str().unwrap();

    // 1024次元の embedding で顔登録
    let embedding: Vec<f64> = (0..1024).map(|i| (i as f64) * 0.001).collect();
    let res = client
        .put(format!("{base_url}/api/employees/{id}/face"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "face_photo_url": "https://mock-storage/test-bucket/face.jpg",
            "face_embedding": embedding,
            "face_model_version": "test-v1"
        }))
        .send().await.unwrap();
    assert_eq!(res.status(), 200);
    let updated: Value = res.json().await.unwrap();
    assert_eq!(updated["face_approval_status"], "pending");

    // 承認
    let res = client
        .put(format!("{base_url}/api/employees/{id}/face/approve"))
        .header("Authorization", &auth)
        .send().await.unwrap();
    assert_eq!(res.status(), 200);
    let approved: Value = res.json().await.unwrap();
    assert_eq!(approved["face_approval_status"], "approved");

    // face-data に表示
    let res = client
        .get(format!("{base_url}/api/employees/face-data"))
        .header("Authorization", &auth)
        .send().await.unwrap();
    let data: Vec<Value> = res.json().await.unwrap();
    assert_eq!(data.len(), 1);
}

#[tokio::test]
async fn test_update_face_and_reject() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "FaceReject").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    let emp = common::create_test_employee(&client, &base_url, &auth, "FaceRej", "FR01").await;
    let id = emp["id"].as_str().unwrap();

    let embedding: Vec<f64> = (0..1024).map(|i| (i as f64) * 0.001).collect();
    client.put(format!("{base_url}/api/employees/{id}/face"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "face_embedding": embedding,
            "face_model_version": "test-v1"
        }))
        .send().await.unwrap();

    let res = client
        .put(format!("{base_url}/api/employees/{id}/face/reject"))
        .header("Authorization", &auth)
        .send().await.unwrap();
    assert_eq!(res.status(), 200);
    let rejected: Value = res.json().await.unwrap();
    assert_eq!(rejected["face_approval_status"], "rejected");
}

#[tokio::test]
async fn test_list_face_data_empty() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "Face Data").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/employees/face-data"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let data: Vec<Value> = res.json().await.unwrap();
    assert_eq!(data.len(), 0);
}

// ============================================================
// Face flow edge cases
// ============================================================

/// update_face with valid 1024-dim embedding sets status to pending
#[tokio::test]
async fn test_update_face_valid_embedding_pending() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "FacePend").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    let emp = common::create_test_employee(&client, &base_url, &auth, "FacePendEmp", "FP01").await;
    let id = emp["id"].as_str().unwrap();

    let embedding: Vec<f64> = (0..1024).map(|i| (i as f64) * 0.001).collect();
    let res = client
        .put(format!("{base_url}/api/employees/{id}/face"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "face_embedding": embedding,
            "face_model_version": "faceres-wasm-v1"
        }))
        .send().await.unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["face_approval_status"], "pending");
    assert_eq!(body["face_model_version"], "faceres-wasm-v1");

    // face-data should NOT include pending entries
    let res = client
        .get(format!("{base_url}/api/employees/face-data"))
        .header("Authorization", &auth)
        .send().await.unwrap();
    let data: Vec<Value> = res.json().await.unwrap();
    assert_eq!(data.len(), 0, "pending face should not appear in face-data");
}

/// Full flow: update_face -> approve -> visible in face-data
#[tokio::test]
async fn test_face_approve_visible_in_face_data() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "FaceVis").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    let emp = common::create_test_employee(&client, &base_url, &auth, "FaceVisEmp", "FV01").await;
    let id = emp["id"].as_str().unwrap();

    let embedding: Vec<f64> = (0..1024).map(|i| (i as f64) * 0.002).collect();
    client.put(format!("{base_url}/api/employees/{id}/face"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "face_photo_url": "https://mock/face.jpg",
            "face_embedding": embedding,
            "face_model_version": "test-v2"
        }))
        .send().await.unwrap();

    // Approve
    let res = client
        .put(format!("{base_url}/api/employees/{id}/face/approve"))
        .header("Authorization", &auth)
        .send().await.unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["face_approval_status"], "approved");
    assert!(body["face_approved_at"].as_str().is_some());

    // face-data should include approved entry with embedding
    let res = client
        .get(format!("{base_url}/api/employees/face-data"))
        .header("Authorization", &auth)
        .send().await.unwrap();
    let data: Vec<Value> = res.json().await.unwrap();
    assert_eq!(data.len(), 1);
    assert_eq!(data[0]["id"], id);
    assert_eq!(data[0]["face_model_version"], "test-v2");
    assert!(data[0]["face_embedding"].as_array().is_some());
}

/// Reject flow: update_face -> reject -> not in face-data
#[tokio::test]
async fn test_face_reject_not_in_face_data() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "FaceRejND").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    let emp = common::create_test_employee(&client, &base_url, &auth, "FaceRejEmp", "FRN01").await;
    let id = emp["id"].as_str().unwrap();

    let embedding: Vec<f64> = (0..1024).map(|i| (i as f64) * 0.003).collect();
    client.put(format!("{base_url}/api/employees/{id}/face"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "face_embedding": embedding,
            "face_model_version": "test-v3"
        }))
        .send().await.unwrap();

    // Reject
    let res = client
        .put(format!("{base_url}/api/employees/{id}/face/reject"))
        .header("Authorization", &auth)
        .send().await.unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["face_approval_status"], "rejected");

    // face-data should NOT include rejected entries
    let res = client
        .get(format!("{base_url}/api/employees/face-data"))
        .header("Authorization", &auth)
        .send().await.unwrap();
    let data: Vec<Value> = res.json().await.unwrap();
    assert_eq!(data.len(), 0, "rejected face should not appear in face-data");
}

/// Approve on non-pending employee returns 404
#[tokio::test]
async fn test_face_approve_non_pending_returns_404() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "FaceAppNP").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    let emp = common::create_test_employee(&client, &base_url, &auth, "NoFaceEmp", "NF01").await;
    let id = emp["id"].as_str().unwrap();

    // No face registered, approval_status is not 'pending'
    let res = client
        .put(format!("{base_url}/api/employees/{id}/face/approve"))
        .header("Authorization", &auth)
        .send().await.unwrap();
    assert_eq!(res.status(), 404);
}

/// Reject on non-pending employee returns 404
#[tokio::test]
async fn test_face_reject_non_pending_returns_404() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "FaceRejNP").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    let emp = common::create_test_employee(&client, &base_url, &auth, "NoFaceEmp2", "NF02").await;
    let id = emp["id"].as_str().unwrap();

    let res = client
        .put(format!("{base_url}/api/employees/{id}/face/reject"))
        .header("Authorization", &auth)
        .send().await.unwrap();
    assert_eq!(res.status(), 404);
}

/// Re-register face after approval resets to pending
#[tokio::test]
async fn test_face_reregister_resets_to_pending() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "FaceReReg").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    let emp = common::create_test_employee(&client, &base_url, &auth, "ReRegEmp", "RR01").await;
    let id = emp["id"].as_str().unwrap();

    let embedding: Vec<f64> = (0..1024).map(|i| (i as f64) * 0.001).collect();

    // Register + approve
    client.put(format!("{base_url}/api/employees/{id}/face"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "face_embedding": embedding,
            "face_model_version": "v1"
        }))
        .send().await.unwrap();
    client.put(format!("{base_url}/api/employees/{id}/face/approve"))
        .header("Authorization", &auth)
        .send().await.unwrap();

    // Re-register with new embedding
    let new_embedding: Vec<f64> = (0..1024).map(|i| (i as f64) * 0.005).collect();
    let res = client
        .put(format!("{base_url}/api/employees/{id}/face"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "face_embedding": new_embedding,
            "face_model_version": "v2"
        }))
        .send().await.unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["face_approval_status"], "pending", "re-register should reset to pending");
    assert_eq!(body["face_model_version"], "v2");

    // face-data should be empty (pending, not approved)
    let res = client
        .get(format!("{base_url}/api/employees/face-data"))
        .header("Authorization", &auth)
        .send().await.unwrap();
    let data: Vec<Value> = res.json().await.unwrap();
    assert_eq!(data.len(), 0);
}

/// update_face on nonexistent employee returns 404
#[tokio::test]
async fn test_update_face_nonexistent_employee() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "FaceNE").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let fake_id = uuid::Uuid::new_v4();
    let embedding: Vec<f64> = (0..1024).map(|i| (i as f64) * 0.001).collect();
    let res = client
        .put(format!("{base_url}/api/employees/{fake_id}/face"))
        .header("Authorization", format!("Bearer {jwt}"))
        .json(&serde_json::json!({
            "face_embedding": embedding,
            "face_model_version": "v1"
        }))
        .send().await.unwrap();
    assert_eq!(res.status(), 404);
}

/// update_face with only face_photo_url (no embedding) should not change approval status
#[tokio::test]
async fn test_update_face_photo_only_no_status_change() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "FacePhoto").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    let emp = common::create_test_employee(&client, &base_url, &auth, "PhotoEmp", "PH01").await;
    let id = emp["id"].as_str().unwrap();

    // Update only photo URL, no embedding
    let res = client
        .put(format!("{base_url}/api/employees/{id}/face"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "face_photo_url": "https://mock/photo-only.jpg"
        }))
        .send().await.unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    // Status should remain null/none (not changed to pending since no embedding provided)
    assert_ne!(body["face_approval_status"], "pending",
        "photo-only update should not set status to pending");
}
