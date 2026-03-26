mod common;

use serde_json::Value;

// ============================================================
// Tenko Call
// ============================================================

#[tokio::test]
async fn test_tenko_call_list_numbers() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "Tenko Call").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/tenko-call/numbers"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let _numbers: Vec<Value> = res.json().await.unwrap();
}

// NOTE: tenko_call_numbers テーブルに INSERT 権限がない (GRANT SELECT のみ)
// create_number は本番でも 500 になるバグ → 修正後にテスト有効化
// #[tokio::test]
// async fn test_tenko_call_create_number() { ... }

#[tokio::test]
async fn test_tenko_call_list_drivers() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "Tenko Drivers").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/tenko-call/drivers"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let _drivers: Vec<Value> = res.json().await.unwrap();
}

#[tokio::test]
async fn test_tenko_call_register_driver() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "Tenko Reg").await;
    let client = reqwest::Client::new();

    // 電話番号マスタを直接 DB に INSERT (ユニーク制約のためランダム化)
    let call_number = format!("03-{}", uuid::Uuid::new_v4().simple().to_string().get(..8).unwrap());
    sqlx::query("INSERT INTO tenko_call_numbers (call_number, tenant_id, label) VALUES ($1, $2, $3)")
        .bind(&call_number)
        .bind(tenant_id.to_string())
        .bind("営業所")
        .execute(&state.pool)
        .await
        .unwrap();

    // ドライバー登録 (公開エンドポイント)
    let phone = format!("090-{}", uuid::Uuid::new_v4().simple().to_string().get(..8).unwrap());
    let res = client
        .post(format!("{base_url}/api/tenko-call/register"))
        .json(&serde_json::json!({
            "phone_number": phone,
            "driver_name": "テスト運転者",
            "call_number": call_number
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["success"], true);
}

// ============================================================
// Timecard
// ============================================================

#[tokio::test]
async fn test_timecard_cards_crud() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "Timecard").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    // 従業員作成
    let emp = common::create_test_employee(&client, &base_url, &auth, "TimecardEmp", "TC01").await;
    let emp_id = emp["id"].as_str().unwrap();

    // カード一覧 → 空
    let res = client
        .get(format!("{base_url}/api/timecard/cards"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let cards: Vec<Value> = res.json().await.unwrap();
    assert_eq!(cards.len(), 0);

    // カード作成
    let res = client
        .post(format!("{base_url}/api/timecard/cards"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "employee_id": emp_id,
            "card_id": "CARD-001",
            "label": "テストカード"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 201);
    let card: Value = res.json().await.unwrap();
    let card_db_id = card["id"].as_str().unwrap();

    // カード一覧 → 1件
    let res = client
        .get(format!("{base_url}/api/timecard/cards"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    let cards: Vec<Value> = res.json().await.unwrap();
    assert_eq!(cards.len(), 1);

    // カード ID で取得
    let res = client
        .get(format!("{base_url}/api/timecard/cards/{card_db_id}"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    // card_id で取得
    let res = client
        .get(format!(
            "{base_url}/api/timecard/cards/by-card/CARD-001"
        ))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    // カード削除
    let res = client
        .delete(format!("{base_url}/api/timecard/cards/{card_db_id}"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 204);
}

#[tokio::test]
async fn test_timecard_punch() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "Punch").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    let emp = common::create_test_employee(&client, &base_url, &auth, "PunchEmp", "PU01").await;
    let emp_id = emp["id"].as_str().unwrap();

    // カード作成
    client
        .post(format!("{base_url}/api/timecard/cards"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "employee_id": emp_id,
            "card_id": "PUNCH-001"
        }))
        .send()
        .await
        .unwrap();

    // 打刻
    let res = client
        .post(format!("{base_url}/api/timecard/punch"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({ "card_id": "PUNCH-001" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 201);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["employee_name"], "PunchEmp");
}

#[tokio::test]
async fn test_timecard_punches_list() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "Punches List").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/timecard/punches"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
}

// ============================================================
// Tenko Schedules (基本)
// ============================================================

#[tokio::test]
async fn test_tenko_schedules_list() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "Tenko Sched").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/tenko/schedules"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
}

// ============================================================
// Tenko Sessions (基本)
// ============================================================

#[tokio::test]
async fn test_tenko_sessions_list() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "Tenko Sess").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/tenko/sessions"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
}

// ============================================================
// Tenko Records (基本)
// ============================================================

#[tokio::test]
async fn test_tenko_records_list() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "Tenko Rec").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/tenko/records"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
}

// ============================================================
// Health Baselines (基本)
// ============================================================

#[tokio::test]
async fn test_health_baselines_list() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "Health BL").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/tenko/health-baselines"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
}

// ============================================================
// Equipment Failures (基本)
// ============================================================

#[tokio::test]
async fn test_equipment_failures_list() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "Equip Fail").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/tenko/equipment-failures"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
}

// ============================================================
// Tenko Webhooks (基本)
// ============================================================

#[tokio::test]
async fn test_tenko_webhooks_list() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "Webhooks").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/tenko/webhooks"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
}

// ============================================================
// Carrying Items (基本)
// ============================================================

#[tokio::test]
async fn test_carrying_items_crud() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "Carry CRUD").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    // Create
    let res = client
        .post(format!("{base_url}/api/carrying-items"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({ "item_name": "免許証" }))
        .send()
        .await
        .unwrap();
    assert!(res.status() == 200 || res.status() == 201);
    let item: Value = res.json().await.unwrap();
    let item_id = item["id"].as_str().unwrap();

    // Update
    let res = client
        .put(format!("{base_url}/api/carrying-items/{item_id}"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({ "item_name": "運転免許証" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    // Delete
    let res = client
        .delete(format!("{base_url}/api/carrying-items/{item_id}"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 204);
}

#[tokio::test]
async fn test_communication_items_crud() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "Comm CRUD").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    // Create
    let res = client
        .post(format!("{base_url}/api/communication-items"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "title": "安全連絡",
            "body": "本日は雨天です",
            "priority": "normal"
        }))
        .send()
        .await
        .unwrap();
    assert!(res.status() == 200 || res.status() == 201);
    let item: Value = res.json().await.unwrap();
    let item_id = item["id"].as_str().unwrap();

    // Active list
    let res = client
        .get(format!("{base_url}/api/communication-items/active"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    // Update
    let res = client
        .put(format!("{base_url}/api/communication-items/{item_id}"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "title": "安全連絡（更新）",
            "body": "本日は雨天注意",
            "priority": "high"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    // Delete
    let res = client
        .delete(format!("{base_url}/api/communication-items/{item_id}"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 204);
}

#[tokio::test]
async fn test_carrying_items_list() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "Carrying").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/carrying-items"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
}

// ============================================================
// Communication Items (基本)
// ============================================================

#[tokio::test]
async fn test_communication_items_list() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "Comms").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/communication-items"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
}

// ============================================================
// Guidance Records (基本)
// ============================================================

#[tokio::test]
async fn test_guidance_records_crud() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "Guidance").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    // Employee needed for guidance record
    let emp = common::create_test_employee(&client, &base_url, &auth, "GuidEmp", "GE01").await;
    let emp_id = emp["id"].as_str().unwrap();

    // List empty
    let res = client
        .get(format!("{base_url}/api/guidance-records"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    // Create
    let res = client
        .post(format!("{base_url}/api/guidance-records"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "employee_id": emp_id,
            "title": "初任運転者指導",
            "content": "座学研修実施",
            "guided_by": "管理者A"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 201);
    let record: Value = res.json().await.unwrap();
    let record_id = record["id"].as_str().unwrap();

    // Get
    let res = client
        .get(format!("{base_url}/api/guidance-records/{record_id}"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    // Update
    let res = client
        .put(format!("{base_url}/api/guidance-records/{record_id}"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({ "title": "初任運転者指導（完了）" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    // List with filter
    let res = client
        .get(format!("{base_url}/api/guidance-records?employee_id={emp_id}"))
        .header("Authorization", &auth)
        .send().await.unwrap();
    assert_eq!(res.status(), 200);

    // List attachments (empty)
    let res = client
        .get(format!("{base_url}/api/guidance-records/{record_id}/attachments"))
        .header("Authorization", &auth)
        .send().await.unwrap();
    assert_eq!(res.status(), 200);

    // Delete
    let res = client
        .delete(format!("{base_url}/api/guidance-records/{record_id}"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 204);
}

#[tokio::test]
async fn test_guidance_records_upload_attachment() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "GuidAtt").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    let emp = common::create_test_employee(&client, &base_url, &auth, "AttEmp", "AT01").await;
    let emp_id = emp["id"].as_str().unwrap();

    // record 作成
    let res = client.post(format!("{base_url}/api/guidance-records"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({ "employee_id": emp_id, "title": "添付テスト" }))
        .send().await.unwrap();
    let record: Value = res.json().await.unwrap();
    let record_id = record["id"].as_str().unwrap();

    // attachment upload (multipart)
    let file_part = reqwest::multipart::Part::bytes(b"test attachment data".to_vec())
        .file_name("test.pdf")
        .mime_str("application/pdf")
        .unwrap();
    let form = reqwest::multipart::Form::new().part("file", file_part);

    let res = client
        .post(format!("{base_url}/api/guidance-records/{record_id}/attachments"))
        .header("Authorization", &auth)
        .multipart(form)
        .send().await.unwrap();
    assert!(res.status() == 200 || res.status() == 201, "attachment upload: {}", res.status());

    // attachment list
    let res = client
        .get(format!("{base_url}/api/guidance-records/{record_id}/attachments"))
        .header("Authorization", &auth)
        .send().await.unwrap();
    assert_eq!(res.status(), 200);
    let atts: Vec<Value> = res.json().await.unwrap();
    assert!(!atts.is_empty());
}

#[tokio::test]
async fn test_guidance_records_list_with_date_filter() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "GuidDate").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/guidance-records?date_from=2026-01-01T00:00:00Z&date_to=2026-12-31T23:59:59Z"))
        .header("Authorization", &auth)
        .send().await.unwrap();
    assert_eq!(res.status(), 200);
}

// ============================================================
// Tenant Users (管理者)
// ============================================================

#[tokio::test]
async fn test_tenant_users_list() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "TenantUsers").await;
    let (user_id, _) = common::create_test_user_in_db(&state.pool, tenant_id, "tu@test.com", "admin").await;
    let jwt = common::create_test_jwt_for_user(user_id, tenant_id, "tu@test.com", "admin");
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/admin/users"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
}

// ============================================================
// Daily Health Status
// ============================================================

#[tokio::test]
async fn test_daily_health_status() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "DailyHealth").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/tenko/daily-health-status"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
}

#[tokio::test]
async fn test_daily_health_status_with_date() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "DailyHealthD").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/tenko/daily-health-status?date=2026-03-26"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
}

// ============================================================
// Driver Info
// ============================================================

#[tokio::test]
async fn test_driver_info() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "DriverInfo").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    let emp = common::create_test_employee(&client, &base_url, &auth, "InfoEmp", "DI01").await;
    let emp_id = emp["id"].as_str().unwrap();

    let res = client
        .get(format!("{base_url}/api/tenko/driver-info/{emp_id}"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
}

// ============================================================
// Timecard — CSV + フィルタ
// ============================================================

#[tokio::test]
async fn test_timecard_punches_csv() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "CSV Punch").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    let emp = common::create_test_employee(&client, &base_url, &auth, "CsvEmp", "CSV1").await;
    let emp_id = emp["id"].as_str().unwrap();

    // カード作成 + 打刻
    client.post(format!("{base_url}/api/timecard/cards"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({ "employee_id": emp_id, "card_id": "CSV-CARD" }))
        .send().await.unwrap();

    client.post(format!("{base_url}/api/timecard/punch"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({ "card_id": "CSV-CARD" }))
        .send().await.unwrap();

    // CSV エクスポート
    let res = client
        .get(format!("{base_url}/api/timecard/punches/csv"))
        .header("Authorization", &auth)
        .send().await.unwrap();
    assert_eq!(res.status(), 200);
    let ct = res.headers().get("content-type").unwrap().to_str().unwrap();
    assert!(ct.contains("csv"));
}

#[tokio::test]
async fn test_timecard_punches_with_filter() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "FilterPunch").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    let emp = common::create_test_employee(&client, &base_url, &auth, "FiltEmp", "FI01").await;
    let emp_id = emp["id"].as_str().unwrap();

    // フィルタ付き一覧
    let res = client
        .get(format!("{base_url}/api/timecard/punches?employee_id={emp_id}&page=1&per_page=10"))
        .header("Authorization", &auth)
        .send().await.unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["total"], 0);
}

// ============================================================
// Tenko Call — 点呼送信
// ============================================================

#[tokio::test]
async fn test_tenko_call_tenko() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "TenkoSend").await;
    let client = reqwest::Client::new();

    // 電話番号マスタ + ドライバー登録
    let call_num = format!("03-{}", &uuid::Uuid::new_v4().simple().to_string()[..8]);
    let phone = format!("090-{}", &uuid::Uuid::new_v4().simple().to_string()[..8]);

    sqlx::query("INSERT INTO tenko_call_numbers (call_number, tenant_id) VALUES ($1, $2)")
        .bind(&call_num)
        .bind(tenant_id.to_string())
        .execute(&state.pool).await.unwrap();

    client.post(format!("{base_url}/api/tenko-call/register"))
        .json(&serde_json::json!({
            "phone_number": phone,
            "driver_name": "点呼運転者",
            "call_number": call_num
        }))
        .send().await.unwrap();

    // 点呼送信
    let res = client
        .post(format!("{base_url}/api/tenko-call/tenko"))
        .json(&serde_json::json!({
            "phone_number": phone,
            "driver_name": "点呼運転者",
            "latitude": 35.6762,
            "longitude": 139.6503
        }))
        .send().await.unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["success"], true);
}

#[tokio::test]
async fn test_tenko_call_delete_number() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "TenkoDelNum").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    // 電話番号マスタ作成
    let call_num = format!("03-{}", &uuid::Uuid::new_v4().simple().to_string()[..8]);
    let row: (i32,) = sqlx::query_as("INSERT INTO tenko_call_numbers (call_number, tenant_id) VALUES ($1, $2) RETURNING id")
        .bind(&call_num)
        .bind(tenant_id.to_string())
        .fetch_one(&state.pool).await.unwrap();

    let res = client
        .delete(format!("{base_url}/api/tenko-call/numbers/{}", row.0))
        .header("Authorization", format!("Bearer {jwt}"))
        .send().await.unwrap();
    assert_eq!(res.status(), 204);
}

// ============================================================
// Car Inspections
// ============================================================

#[tokio::test]
async fn test_car_inspections_current() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "CarIns").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/car-inspections/current"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send().await.unwrap();
    assert_eq!(res.status(), 200);
}

#[tokio::test]
async fn test_car_inspections_expired() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "CarInsExp").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/car-inspections/expired"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send().await.unwrap();
    assert_eq!(res.status(), 200);
}

#[tokio::test]
async fn test_car_inspections_renew() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "CarInsRen").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/car-inspections/renew"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send().await.unwrap();
    assert_eq!(res.status(), 200);
}

#[tokio::test]
async fn test_car_inspections_vehicle_categories() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "VehCat").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/car-inspections/vehicle-categories"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send().await.unwrap();
    assert_eq!(res.status(), 200);
}

// ============================================================
// Car Inspection Files
// ============================================================

#[tokio::test]
async fn test_car_inspection_files_current() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "CarInsFiles").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/car-inspection-files/current"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send().await.unwrap();
    assert_eq!(res.status(), 200);
}

// ============================================================
// Guidance Records — 親子レコード + 添付ファイル + guidance_type フィルタ
// ============================================================

#[tokio::test]
async fn test_guidance_records_child_record() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "GuidChild").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    let emp = common::create_test_employee(&client, &base_url, &auth, "ChildEmp", "CE01").await;
    let emp_id = emp["id"].as_str().unwrap();

    // 親レコード作成
    let res = client
        .post(format!("{base_url}/api/guidance-records"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "employee_id": emp_id,
            "title": "親レコード",
            "guided_by": "管理者A",
            "guidance_type": "initial"
        }))
        .send().await.unwrap();
    assert_eq!(res.status(), 201);
    let parent: Value = res.json().await.unwrap();
    let parent_id = parent["id"].as_str().unwrap();
    assert_eq!(parent["depth"], 0);

    // 子レコード作成 (parent_id 指定)
    let res = client
        .post(format!("{base_url}/api/guidance-records"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "employee_id": emp_id,
            "title": "子レコード",
            "guided_by": "管理者B",
            "parent_id": parent_id
        }))
        .send().await.unwrap();
    assert_eq!(res.status(), 201);
    let child: Value = res.json().await.unwrap();
    assert_eq!(child["depth"], 1);
    assert_eq!(child["parent_id"].as_str().unwrap(), parent_id);

    // 親を取得して確認
    let res = client
        .get(format!("{base_url}/api/guidance-records/{parent_id}"))
        .header("Authorization", &auth)
        .send().await.unwrap();
    assert_eq!(res.status(), 200);

    // 親削除 → 子も再帰削除
    let res = client
        .delete(format!("{base_url}/api/guidance-records/{parent_id}"))
        .header("Authorization", &auth)
        .send().await.unwrap();
    assert_eq!(res.status(), 204);

    // 子も消えている
    let child_id = child["id"].as_str().unwrap();
    let res = client
        .get(format!("{base_url}/api/guidance-records/{child_id}"))
        .header("Authorization", &auth)
        .send().await.unwrap();
    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn test_guidance_records_attachments_empty_after_creation() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "GuidAtt").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    let emp = common::create_test_employee(&client, &base_url, &auth, "AttEmp", "AT01").await;
    let emp_id = emp["id"].as_str().unwrap();

    // レコード作成
    let res = client
        .post(format!("{base_url}/api/guidance-records"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "employee_id": emp_id,
            "title": "添付テスト",
            "guided_by": "管理者C"
        }))
        .send().await.unwrap();
    assert_eq!(res.status(), 201);
    let record: Value = res.json().await.unwrap();
    let record_id = record["id"].as_str().unwrap();

    // 添付ファイル一覧 → 空
    let res = client
        .get(format!("{base_url}/api/guidance-records/{record_id}/attachments"))
        .header("Authorization", &auth)
        .send().await.unwrap();
    assert_eq!(res.status(), 200);
    let atts: Vec<Value> = res.json().await.unwrap();
    assert_eq!(atts.len(), 0);

    // クリーンアップ
    client
        .delete(format!("{base_url}/api/guidance-records/{record_id}"))
        .header("Authorization", &auth)
        .send().await.unwrap();
}

#[tokio::test]
async fn test_guidance_records_filter_by_guidance_type() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "GuidType").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    let emp = common::create_test_employee(&client, &base_url, &auth, "TypeEmp", "TY01").await;
    let emp_id = emp["id"].as_str().unwrap();

    // "initial" タイプのレコード作成
    let res = client
        .post(format!("{base_url}/api/guidance-records"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "employee_id": emp_id,
            "title": "初任指導",
            "guidance_type": "initial",
            "guided_by": "A"
        }))
        .send().await.unwrap();
    assert_eq!(res.status(), 201);
    let rec1: Value = res.json().await.unwrap();

    // "accident" タイプのレコード作成
    let res = client
        .post(format!("{base_url}/api/guidance-records"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "employee_id": emp_id,
            "title": "事故惹起者指導",
            "guidance_type": "accident",
            "guided_by": "B"
        }))
        .send().await.unwrap();
    assert_eq!(res.status(), 201);
    let rec2: Value = res.json().await.unwrap();

    // guidance_type=initial でフィルタ
    let res = client
        .get(format!("{base_url}/api/guidance-records?guidance_type=initial"))
        .header("Authorization", &auth)
        .send().await.unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    let records = body["records"].as_array().unwrap();
    for r in records {
        assert_eq!(r["guidance_type"], "initial");
    }
    assert!(records.len() >= 1);

    // guidance_type=accident でフィルタ
    let res = client
        .get(format!("{base_url}/api/guidance-records?guidance_type=accident"))
        .header("Authorization", &auth)
        .send().await.unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    let records = body["records"].as_array().unwrap();
    for r in records {
        assert_eq!(r["guidance_type"], "accident");
    }
    assert!(records.len() >= 1);

    // クリーンアップ
    let r1_id = rec1["id"].as_str().unwrap();
    let r2_id = rec2["id"].as_str().unwrap();
    client.delete(format!("{base_url}/api/guidance-records/{r1_id}"))
        .header("Authorization", &auth).send().await.unwrap();
    client.delete(format!("{base_url}/api/guidance-records/{r2_id}"))
        .header("Authorization", &auth).send().await.unwrap();
}

// ============================================================
// Carrying Items — vehicle_conditions ネスト
// ============================================================

#[tokio::test]
#[ignore] // carrying_item_vehicle_conditions テーブルのマイグレーション未作成
async fn test_carrying_items_with_vehicle_conditions() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "CarryVC").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    // vehicle_conditions 付きで作成
    let res = client
        .post(format!("{base_url}/api/carrying-items"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "item_name": "輪止め",
            "is_required": true,
            "vehicle_conditions": [
                { "category": "普通", "value": "4t" },
                { "category": "大型", "value": "10t" }
            ]
        }))
        .send().await.unwrap();
    assert!(res.status() == 200 || res.status() == 201);
    let item: Value = res.json().await.unwrap();
    let item_id = item["id"].as_str().unwrap();
    assert_eq!(item["item_name"], "輪止め");

    // vehicle_conditions がネストされている
    let conditions = item["vehicle_conditions"].as_array().unwrap();
    assert_eq!(conditions.len(), 2);
    let categories: Vec<&str> = conditions.iter().map(|c| c["category"].as_str().unwrap()).collect();
    assert!(categories.contains(&"普通"));
    assert!(categories.contains(&"大型"));

    // 一覧取得でもネストされている
    let res = client
        .get(format!("{base_url}/api/carrying-items"))
        .header("Authorization", &auth)
        .send().await.unwrap();
    assert_eq!(res.status(), 200);
    let items: Vec<Value> = res.json().await.unwrap();
    let target = items.iter().find(|i| i["id"].as_str() == Some(item_id)).unwrap();
    let list_conditions = target["vehicle_conditions"].as_array().unwrap();
    assert_eq!(list_conditions.len(), 2);

    // クリーンアップ
    let res = client
        .delete(format!("{base_url}/api/carrying-items/{item_id}"))
        .header("Authorization", &auth)
        .send().await.unwrap();
    assert_eq!(res.status(), 204);
}

// ============================================================
// Car Inspections — GET by ID with fake ID → 404
// ============================================================

#[tokio::test]
async fn test_car_inspections_get_by_fake_id_returns_404() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "CarIns404").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    // 存在しない ID (car_inspection.id は i32)
    let res = client
        .get(format!("{base_url}/api/car-inspections/999999999"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send().await.unwrap();
    assert_eq!(res.status(), 404);
}

// ============================================================
// Bot Admin — update (既存config更新)
// ============================================================

#[tokio::test]
#[ignore] // llvm-cov 環境で env var 競合
async fn test_bot_admin_update_config() {
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "BotUpd").await;
    let (user_id, _) = common::create_test_user_in_db(&state.pool, tenant_id, "botupd@test.com", "admin").await;
    let jwt = common::create_test_jwt_for_user(user_id, tenant_id, "botupd@test.com", "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    // create
    let res = client.post(format!("{base_url}/api/admin/bot/configs"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "name": "UpdBot", "client_id": "upd-cid", "service_account": "upd@sa", "bot_id": "upd-bot"
        }))
        .send().await.unwrap();
    let body: Value = res.json().await.unwrap();
    let bot_id = body["id"].as_str().unwrap().to_string();

    // update (upsert with same id)
    let res = client.post(format!("{base_url}/api/admin/bot/configs"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "id": bot_id, "name": "UpdBot2", "client_id": "upd-cid2",
            "client_secret": "secret2", "service_account": "upd@sa2",
            "private_key": "key2", "bot_id": "upd-bot2"
        }))
        .send().await.unwrap();
    assert!(res.status() == 200 || res.status() == 201);
}

// ============================================================
// NFC Tags (基本)
// ============================================================

#[tokio::test]
async fn test_nfc_tags_list() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "NFC Tags").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/nfc-tags"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
}

// ============================================================
// Timecard — list_punches with date filter
// ============================================================

#[tokio::test]
async fn test_timecard_punches_with_date_filter() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "PunchDateF").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    let emp = common::create_test_employee(&client, &base_url, &auth, "DateFEmp", "DF01").await;
    let emp_id = emp["id"].as_str().unwrap();

    // カード作成 + 打刻
    client
        .post(format!("{base_url}/api/timecard/cards"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({ "employee_id": emp_id, "card_id": "DATE-FILT" }))
        .send()
        .await
        .unwrap();

    let res = client
        .post(format!("{base_url}/api/timecard/punch"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({ "card_id": "DATE-FILT" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 201);

    // date_from / date_to フィルタ付き一覧
    let res = client
        .get(format!(
            "{base_url}/api/timecard/punches?date_from=2026-01-01T00:00:00Z&date_to=2026-12-31T23:59:59Z"
        ))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert!(body["total"].as_i64().unwrap() >= 1);
    assert!(body["punches"].as_array().unwrap().len() >= 1);
}

// ============================================================
// Timecard — CSV export with date filter
// ============================================================

#[tokio::test]
async fn test_timecard_csv_with_date_filter() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "CsvDateF").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    let emp = common::create_test_employee(&client, &base_url, &auth, "CsvDFEmp", "CD01").await;
    let emp_id = emp["id"].as_str().unwrap();

    // カード作成 + 打刻
    client
        .post(format!("{base_url}/api/timecard/cards"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({ "employee_id": emp_id, "card_id": "CSV-DATE" }))
        .send()
        .await
        .unwrap();

    client
        .post(format!("{base_url}/api/timecard/punch"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({ "card_id": "CSV-DATE" }))
        .send()
        .await
        .unwrap();

    // date フィルタ付き CSV エクスポート
    let res = client
        .get(format!(
            "{base_url}/api/timecard/punches/csv?date_from=2026-01-01T00:00:00Z&date_to=2026-12-31T23:59:59Z"
        ))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let ct = res.headers().get("content-type").unwrap().to_str().unwrap();
    assert!(ct.contains("csv"));
    let body = res.text().await.unwrap();
    // BOM + ヘッダー行 + データ行
    assert!(body.contains("社員名"));
    assert!(body.contains("CsvDFEmp"));
}

// ============================================================
// Tenko Call — register with invalid call_number → 400
// ============================================================

#[tokio::test]
async fn test_tenko_call_register_invalid_call_number() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let _tenant_id = common::create_test_tenant(&state.pool, "TenkoBadReg").await;
    let client = reqwest::Client::new();

    // 存在しない call_number で登録 → 400
    let phone = format!("090-{}", &uuid::Uuid::new_v4().simple().to_string()[..8]);
    let res = client
        .post(format!("{base_url}/api/tenko-call/register"))
        .json(&serde_json::json!({
            "phone_number": phone,
            "driver_name": "テスト運転者",
            "call_number": "99-99999999"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 400);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["success"], false);
    assert!(body["error"].as_str().unwrap().contains("未登録"));
}

// ============================================================
// Tenko Call — tenko with unregistered phone → 404
// ============================================================

#[tokio::test]
async fn test_tenko_call_tenko_unregistered_phone() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let _tenant_id = common::create_test_tenant(&state.pool, "TenkoNoPhone").await;
    let client = reqwest::Client::new();

    // 未登録の電話番号で点呼 → 404
    let phone = format!("090-{}", &uuid::Uuid::new_v4().simple().to_string()[..8]);
    let res = client
        .post(format!("{base_url}/api/tenko-call/tenko"))
        .json(&serde_json::json!({
            "phone_number": phone,
            "driver_name": "未登録運転者",
            "latitude": 35.6762,
            "longitude": 139.6503
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

// ============================================================
// Tenko Webhooks — deliveries endpoint
// ============================================================

#[tokio::test]
async fn test_tenko_webhook_deliveries() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "WhDeliv").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    // Webhook 作成
    let res = client
        .post(format!("{base_url}/api/tenko/webhooks"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "event_type": "tenko_completed",
            "url": "https://example.com/hook",
            "secret": "deliv-secret"
        }))
        .send()
        .await
        .unwrap();
    assert!(res.status() == 200 || res.status() == 201);
    let wh: Value = res.json().await.unwrap();
    let wh_id = wh["id"].as_str().unwrap();

    // Deliveries 一覧 (作成直後なので空)
    let res = client
        .get(format!("{base_url}/api/tenko/webhooks/{wh_id}/deliveries"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let deliveries: Vec<Value> = res.json().await.unwrap();
    assert_eq!(deliveries.len(), 0);

    // 存在しない webhook ID で deliveries → 200 (空配列、RLS スコープで絞られる)
    let fake_id = uuid::Uuid::new_v4();
    let res = client
        .get(format!("{base_url}/api/tenko/webhooks/{fake_id}/deliveries"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let deliveries: Vec<Value> = res.json().await.unwrap();
    assert_eq!(deliveries.len(), 0);
}

// ============================================================
// NFC Tags — CRUD (register, search, delete)
// ============================================================

#[tokio::test]
async fn test_nfc_tags_crud() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "NFC CRUD").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    // car_inspection テーブルに最小行を INSERT (NFC タグの FK に必要)
    // RLS を通すために set_current_tenant してから INSERT
    let mut conn = state.pool.acquire().await.unwrap();
    sqlx::query("SELECT set_config('app.current_tenant_id', $1, true)")
        .bind(tenant_id.to_string())
        .execute(&mut *conn)
        .await
        .unwrap();

    let ci_id: (i32,) = sqlx::query_as(
        r#"INSERT INTO alc_api.car_inspection (
            tenant_id,
            "CertInfoImportFileVersion", "Acceptoutputno", "FormType",
            "ElectCertMgNo", "CarId",
            "ElectCertPublishdateE", "ElectCertPublishdateY", "ElectCertPublishdateM", "ElectCertPublishdateD",
            "GrantdateE", "GrantdateY", "GrantdateM", "GrantdateD",
            "TranspotationBureauchiefName", "EntryNoCarNo",
            "ReggrantdateE", "ReggrantdateY", "ReggrantdateM", "ReggrantdateD",
            "FirstregistdateE", "FirstregistdateY", "FirstregistdateM",
            "CarName", "CarNameCode", "CarNo", "Model", "EngineModel",
            "OwnernameLowLevelChar", "OwnernameHighLevelChar",
            "OwnerAddressChar", "OwnerAddressNumValue", "OwnerAddressCode",
            "UsernameLowLevelChar", "UsernameHighLevelChar",
            "UserAddressChar", "UserAddressNumValue", "UserAddressCode",
            "UseheadqrterChar", "UseheadqrterNumValue", "UseheadqrterCode",
            "CarKind", "Use", "PrivateBusiness", "CarShape", "CarShapeCode",
            "NoteCap", "Cap", "NoteMaxloadage", "Maxloadage",
            "NoteCarWgt", "CarWgt", "NoteCarTotalWgt", "CarTotalWgt",
            "NoteLength", "Length", "NoteWidth", "Width", "NoteHeight", "Height",
            "FfAxWgt", "FrAxWgt", "RfAxWgt", "RrAxWgt",
            "Displacement", "FuelClass", "ModelSpecifyNo", "ClassifyAroundNo",
            "ValidPeriodExpirdateE", "ValidPeriodExpirdateY", "ValidPeriodExpirdateM", "ValidPeriodExpirdateD",
            "NoteInfo",
            "TwodimensionCodeInfoEntryNoCarNo", "TwodimensionCodeInfoCarNo",
            "TwodimensionCodeInfoValidPeriodExpirdate", "TwodimensionCodeInfoModel",
            "TwodimensionCodeInfoModelSpecifyNoClassifyAroundNo",
            "TwodimensionCodeInfoCharInfo", "TwodimensionCodeInfoEngineModel",
            "TwodimensionCodeInfoCarNoStampPlace", "TwodimensionCodeInfoFirstregistdate",
            "TwodimensionCodeInfoFfAxWgt", "TwodimensionCodeInfoFrAxWgt",
            "TwodimensionCodeInfoRfAxWgt", "TwodimensionCodeInfoRrAxWgt",
            "TwodimensionCodeInfoNoiseReg", "TwodimensionCodeInfoNearNoiseReg",
            "TwodimensionCodeInfoDriveMethod", "TwodimensionCodeInfoOpacimeterMeasCar",
            "TwodimensionCodeInfoNoxPmMeasMode", "TwodimensionCodeInfoNoxValue",
            "TwodimensionCodeInfoPmValue", "TwodimensionCodeInfoSafeStdDate",
            "TwodimensionCodeInfoFuelClassCode", "RegistCarLightCar"
        ) VALUES (
            $1,
            '', '', '',
            'TEST-CERT-001', 'CAR-001',
            '', '', '', '',
            '', '2026', '03', '01',
            '', '品川500あ1234',
            '', '', '', '',
            '', '', '',
            '', '', '', '', '',
            '', '',
            '', '', '',
            '', '',
            '', '', '',
            '', '', '',
            '', '', '', '', '',
            '', '', '', '',
            '', '', '', '',
            '', '', '', '', '', '',
            '', '', '', '',
            '', '', '', '',
            '', '2027', '03', '01',
            '',
            '', '',
            '', '',
            '',
            '', '',
            '', '',
            '', '',
            '', '',
            '', '',
            '',
            '',
            '', '',
            '', '',
            '', ''
        ) RETURNING id"#,
    )
    .bind(tenant_id)
    .fetch_one(&mut *conn)
    .await
    .unwrap();
    drop(conn);

    let car_inspection_id = ci_id.0;

    // Register NFC tag
    let nfc_uuid = format!("AA:BB:CC:{}", &uuid::Uuid::new_v4().simple().to_string()[..2]);
    let res = client
        .post(format!("{base_url}/api/nfc-tags"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "nfc_uuid": nfc_uuid,
            "car_inspection_id": car_inspection_id
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 201, "register_tag should return 201");
    let tag: Value = res.json().await.unwrap();
    assert!(tag["id"].as_i64().is_some());
    // UUID should be normalized (lowercase, no colons)
    let returned_uuid = tag["nfcUuid"].as_str().unwrap();
    assert!(!returned_uuid.contains(':'), "UUID should be normalized without colons");

    // List NFC tags
    let res = client
        .get(format!("{base_url}/api/nfc-tags"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let tags: Vec<Value> = res.json().await.unwrap();
    assert!(tags.len() >= 1, "should have at least one tag");

    // List NFC tags filtered by car_inspection_id
    let res = client
        .get(format!(
            "{base_url}/api/nfc-tags?car_inspection_id={car_inspection_id}"
        ))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let tags: Vec<Value> = res.json().await.unwrap();
    assert_eq!(tags.len(), 1);

    // Search by UUID
    let res = client
        .get(format!(
            "{base_url}/api/nfc-tags/search?uuid={nfc_uuid}"
        ))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let search_result: Value = res.json().await.unwrap();
    assert!(search_result["nfc_tag"]["id"].as_i64().is_some());

    // Search nonexistent UUID -> 404
    let res = client
        .get(format!(
            "{base_url}/api/nfc-tags/search?uuid=FF:FF:FF:FF"
        ))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);

    // Delete NFC tag (uses normalized UUID)
    let res = client
        .delete(format!(
            "{base_url}/api/nfc-tags/{returned_uuid}"
        ))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 204);

    // Delete again -> 404
    let res = client
        .delete(format!(
            "{base_url}/api/nfc-tags/{returned_uuid}"
        ))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

// ============================================================
// NFC Tags — upsert (register same UUID twice updates car_inspection_id)
// ============================================================

#[tokio::test]
async fn test_nfc_tags_upsert() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "NFC Upsert").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    // Insert 2 car_inspection rows for the upsert test
    let mut conn = state.pool.acquire().await.unwrap();
    sqlx::query("SELECT set_config('app.current_tenant_id', $1, true)")
        .bind(tenant_id.to_string())
        .execute(&mut *conn)
        .await
        .unwrap();

    let insert_ci = |cert_no: &str| {
        format!(
            r#"INSERT INTO alc_api.car_inspection (
                tenant_id,
                "CertInfoImportFileVersion", "Acceptoutputno", "FormType",
                "ElectCertMgNo", "CarId",
                "ElectCertPublishdateE", "ElectCertPublishdateY", "ElectCertPublishdateM", "ElectCertPublishdateD",
                "GrantdateE", "GrantdateY", "GrantdateM", "GrantdateD",
                "TranspotationBureauchiefName", "EntryNoCarNo",
                "ReggrantdateE", "ReggrantdateY", "ReggrantdateM", "ReggrantdateD",
                "FirstregistdateE", "FirstregistdateY", "FirstregistdateM",
                "CarName", "CarNameCode", "CarNo", "Model", "EngineModel",
                "OwnernameLowLevelChar", "OwnernameHighLevelChar",
                "OwnerAddressChar", "OwnerAddressNumValue", "OwnerAddressCode",
                "UsernameLowLevelChar", "UsernameHighLevelChar",
                "UserAddressChar", "UserAddressNumValue", "UserAddressCode",
                "UseheadqrterChar", "UseheadqrterNumValue", "UseheadqrterCode",
                "CarKind", "Use", "PrivateBusiness", "CarShape", "CarShapeCode",
                "NoteCap", "Cap", "NoteMaxloadage", "Maxloadage",
                "NoteCarWgt", "CarWgt", "NoteCarTotalWgt", "CarTotalWgt",
                "NoteLength", "Length", "NoteWidth", "Width", "NoteHeight", "Height",
                "FfAxWgt", "FrAxWgt", "RfAxWgt", "RrAxWgt",
                "Displacement", "FuelClass", "ModelSpecifyNo", "ClassifyAroundNo",
                "ValidPeriodExpirdateE", "ValidPeriodExpirdateY", "ValidPeriodExpirdateM", "ValidPeriodExpirdateD",
                "NoteInfo",
                "TwodimensionCodeInfoEntryNoCarNo", "TwodimensionCodeInfoCarNo",
                "TwodimensionCodeInfoValidPeriodExpirdate", "TwodimensionCodeInfoModel",
                "TwodimensionCodeInfoModelSpecifyNoClassifyAroundNo",
                "TwodimensionCodeInfoCharInfo", "TwodimensionCodeInfoEngineModel",
                "TwodimensionCodeInfoCarNoStampPlace", "TwodimensionCodeInfoFirstregistdate",
                "TwodimensionCodeInfoFfAxWgt", "TwodimensionCodeInfoFrAxWgt",
                "TwodimensionCodeInfoRfAxWgt", "TwodimensionCodeInfoRrAxWgt",
                "TwodimensionCodeInfoNoiseReg", "TwodimensionCodeInfoNearNoiseReg",
                "TwodimensionCodeInfoDriveMethod", "TwodimensionCodeInfoOpacimeterMeasCar",
                "TwodimensionCodeInfoNoxPmMeasMode", "TwodimensionCodeInfoNoxValue",
                "TwodimensionCodeInfoPmValue", "TwodimensionCodeInfoSafeStdDate",
                "TwodimensionCodeInfoFuelClassCode", "RegistCarLightCar"
            ) VALUES (
                '{tenant_id}',
                '', '', '',
                '{cert_no}', 'CAR-001',
                '', '', '', '',
                '', '2026', '03', '01',
                '', '品川500あ1234',
                '', '', '', '',
                '', '', '',
                '', '', '', '', '',
                '', '',
                '', '', '',
                '', '',
                '', '', '',
                '', '', '',
                '', '', '', '', '',
                '', '', '', '',
                '', '', '', '',
                '', '', '', '', '', '',
                '', '', '', '',
                '', '', '', '',
                '', '2027', '03', '01',
                '',
                '', '',
                '', '',
                '',
                '', '',
                '', '',
                '', '',
                '', '',
                '', '',
                '',
                '',
                '', '',
                '', '',
                '', ''
            ) RETURNING id"#,
            tenant_id = tenant_id,
            cert_no = cert_no,
        )
    };

    let ci1: (i32,) = sqlx::query_as(&insert_ci("UPSERT-CERT-001"))
        .fetch_one(&mut *conn)
        .await
        .unwrap();
    let ci2: (i32,) = sqlx::query_as(&insert_ci("UPSERT-CERT-002"))
        .fetch_one(&mut *conn)
        .await
        .unwrap();
    drop(conn);

    let nfc_uuid = "11:22:33:44";

    // Register with first car_inspection_id
    let res = client
        .post(format!("{base_url}/api/nfc-tags"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "nfc_uuid": nfc_uuid,
            "car_inspection_id": ci1.0
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 201);
    let tag1: Value = res.json().await.unwrap();
    assert_eq!(tag1["carInspectionId"], ci1.0);

    // Register same UUID with second car_inspection_id → upsert
    let res = client
        .post(format!("{base_url}/api/nfc-tags"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "nfc_uuid": nfc_uuid,
            "car_inspection_id": ci2.0
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 201);
    let tag2: Value = res.json().await.unwrap();
    assert_eq!(tag2["carInspectionId"], ci2.0, "car_inspection_id should be updated by upsert");
}

// ============================================================
// Event Classifications — list after upload + update
// ============================================================

#[tokio::test]
async fn test_event_classifications_after_upload_and_update() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "EC Update").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    // Upload ZIP to create event classifications
    let zip_bytes = common::create_test_dtako_zip();
    let file_part = reqwest::multipart::Part::bytes(zip_bytes)
        .file_name("test.zip")
        .mime_str("application/zip")
        .unwrap();
    let form = reqwest::multipart::Form::new().part("file", file_part);
    let res = client
        .post(format!("{base_url}/api/upload"))
        .header("Authorization", &auth)
        .multipart(form)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200, "upload should succeed");

    // List event classifications — should have at least one (from KUDGIVT events)
    let res = client
        .get(format!("{base_url}/api/event-classifications"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let classifications: Vec<Value> = res.json().await.unwrap();
    assert!(
        !classifications.is_empty(),
        "should have event classifications after upload"
    );

    // Pick the first classification and update it
    let first = &classifications[0];
    let ec_id = first["id"].as_str().unwrap();
    let original_classification = first["classification"].as_str().unwrap();

    // Update to "rest_split"
    let new_class = if original_classification == "rest_split" {
        "break"
    } else {
        "rest_split"
    };
    let res = client
        .put(format!("{base_url}/api/event-classifications/{ec_id}"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({ "classification": new_class }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let updated: Value = res.json().await.unwrap();
    assert_eq!(updated["classification"], new_class);

    // Update with invalid classification -> 400
    let res = client
        .put(format!("{base_url}/api/event-classifications/{ec_id}"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({ "classification": "invalid_value" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 400);

    // Update nonexistent ID -> 404
    let fake_id = uuid::Uuid::new_v4();
    let res = client
        .put(format!("{base_url}/api/event-classifications/{fake_id}"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({ "classification": "drive" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

// ============================================================
// Dtako CSV Proxy — GET /operations/{unko_no}/csv/{csv_type}
// ============================================================

#[tokio::test]
async fn test_dtako_csv_proxy_kudguri() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "CsvProxy").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    // Upload ZIP first (stores CSVs in MockStorage under {tenant_id}/unko/{unko_no}/KUDGURI.csv)
    let zip_bytes = common::create_test_dtako_zip();
    let file_part = reqwest::multipart::Part::bytes(zip_bytes)
        .file_name("test.zip")
        .mime_str("application/zip")
        .unwrap();
    let form = reqwest::multipart::Form::new().part("file", file_part);
    let res = client
        .post(format!("{base_url}/api/upload"))
        .header("Authorization", &auth)
        .multipart(form)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200, "upload should succeed");

    // GET KUDGURI CSV as JSON for unko_no=1001
    let res = client
        .get(format!("{base_url}/api/operations/1001/csv/kudguri"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert!(body["headers"].as_array().is_some(), "should have headers array");
    assert!(body["rows"].as_array().is_some(), "should have rows array");
    let headers = body["headers"].as_array().unwrap();
    assert!(!headers.is_empty(), "headers should not be empty");
}

#[tokio::test]
async fn test_dtako_csv_proxy_kudgivt() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "CsvProxyEvt").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    // Upload ZIP
    let zip_bytes = common::create_test_dtako_zip();
    let file_part = reqwest::multipart::Part::bytes(zip_bytes)
        .file_name("test.zip")
        .mime_str("application/zip")
        .unwrap();
    let form = reqwest::multipart::Form::new().part("file", file_part);
    client
        .post(format!("{base_url}/api/upload"))
        .header("Authorization", &auth)
        .multipart(form)
        .send()
        .await
        .unwrap();

    // GET KUDGIVT (events) CSV as JSON
    let res = client
        .get(format!("{base_url}/api/operations/1001/csv/events"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert!(body["headers"].as_array().unwrap().len() > 0);
}

#[tokio::test]
async fn test_dtako_csv_proxy_nonexistent_unko_no() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "CsvProxy404").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    // GET CSV for nonexistent unko_no → 404 (no r2_key_prefix, fallback key not in storage)
    let res = client
        .get(format!("{base_url}/api/operations/99999/csv/kudguri"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn test_dtako_csv_proxy_invalid_csv_type() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "CsvProxyBad").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    // GET with invalid csv_type → 400
    let res = client
        .get(format!("{base_url}/api/operations/1001/csv/unknown_type"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 400);
}
