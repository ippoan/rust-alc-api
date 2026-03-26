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
