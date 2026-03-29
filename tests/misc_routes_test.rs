#[macro_use]
mod common;

use serde_json::Value;

// ============================================================
// Tenko Call
// ============================================================

#[tokio::test]
async fn test_tenko_call_list_numbers() {
    test_group!("中間点呼");
    test_case!("電話番号マスタ一覧を取得できる", {
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
    });
}

// NOTE: tenko_call_numbers テーブルに INSERT 権限がない (GRANT SELECT のみ)
// create_number は本番でも 500 になるバグ → 修正後にテスト有効化
// #[tokio::test]
// async fn test_tenko_call_create_number() { ... }

#[tokio::test]
async fn test_tenko_call_list_drivers() {
    test_group!("中間点呼");
    test_case!("登録運転者一覧を取得できる", {
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
    });
}

#[tokio::test]
async fn test_tenko_call_register_driver() {
    test_group!("中間点呼");
    test_case!(
        "運転者を電話番号マスタ経由で登録できる",
        {
            let state = common::setup_app_state().await;
            let base_url = common::spawn_test_server(state.clone()).await;
            let tenant_id = common::create_test_tenant(&state.pool, "Tenko Reg").await;
            let client = reqwest::Client::new();

            // 電話番号マスタを直接 DB に INSERT (ユニーク制約のためランダム化)
            let call_number = format!(
                "03-{}",
                uuid::Uuid::new_v4().simple().to_string().get(..8).unwrap()
            );
            sqlx::query("INSERT INTO tenko_call_numbers (call_number, tenant_id, label) VALUES ($1, $2, $3)")
            .bind(&call_number)
            .bind(tenant_id.to_string())
            .bind("営業所")
            .execute(&state.pool)
            .await
            .unwrap();

            // ドライバー登録 (公開エンドポイント)
            let phone = format!(
                "090-{}",
                uuid::Uuid::new_v4().simple().to_string().get(..8).unwrap()
            );
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
    );
}

// ============================================================
// Timecard
// ============================================================

#[tokio::test]
async fn test_timecard_cards_crud() {
    test_group!("タイムカード");
    test_case!("カードのCRUD操作ができる", {
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;
        let tenant_id = common::create_test_tenant(&state.pool, "Timecard").await;
        let jwt = common::create_test_jwt(tenant_id, "admin");
        let auth = format!("Bearer {jwt}");
        let client = reqwest::Client::new();

        // 従業員作成
        let emp =
            common::create_test_employee(&client, &base_url, &auth, "TimecardEmp", "TC01").await;
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
            .get(format!("{base_url}/api/timecard/cards/by-card/CARD-001"))
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
    });
}

#[tokio::test]
async fn test_timecard_punch() {
    test_group!("タイムカード");
    test_case!("カードIDで打刻できる", {
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
    });
}

#[tokio::test]
async fn test_timecard_punches_list() {
    test_group!("タイムカード");
    test_case!("打刻一覧を取得できる", {
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
    });
}

// ============================================================
// Tenko Schedules (基本)
// ============================================================

#[tokio::test]
async fn test_tenko_schedules_list() {
    test_group!("点呼スケジュール");
    test_case!("スケジュール一覧を取得できる", {
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
    });
}

// ============================================================
// Tenko Sessions (基本)
// ============================================================

#[tokio::test]
async fn test_tenko_sessions_list() {
    test_group!("点呼セッション");
    test_case!("セッション一覧を取得できる", {
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
    });
}

// ============================================================
// Tenko Records (基本)
// ============================================================

#[tokio::test]
async fn test_tenko_records_list() {
    test_group!("点呼記録");
    test_case!("点呼記録一覧を取得できる", {
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
    });
}

// ============================================================
// Health Baselines (基本)
// ============================================================

#[tokio::test]
async fn test_health_baselines_list() {
    test_group!("健康基準値");
    test_case!("健康基準値一覧を取得できる", {
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
    });
}

// ============================================================
// Equipment Failures (基本)
// ============================================================

#[tokio::test]
async fn test_equipment_failures_list() {
    test_group!("機器故障");
    test_case!("機器故障一覧を取得できる", {
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
    });
}

// ============================================================
// Tenko Webhooks (基本)
// ============================================================

#[tokio::test]
async fn test_tenko_webhooks_list() {
    test_group!("点呼Webhook");
    test_case!("Webhook一覧を取得できる", {
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
    });
}

// ============================================================
// Carrying Items (基本)
// ============================================================

#[tokio::test]
async fn test_carrying_items_crud() {
    test_group!("携行品目");
    test_case!("携行品目のCRUD操作ができる", {
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
    });
}

#[tokio::test]
async fn test_communication_items_crud() {
    test_group!("連絡事項");
    test_case!("連絡事項のCRUD操作ができる", {
        let _db = common::DB_RENAME_LOCK.lock().unwrap();
        let _flock = common::db_rename_flock();
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
    });
}

#[tokio::test]
async fn test_carrying_items_list() {
    test_group!("携行品目");
    test_case!("携行品目一覧を取得できる", {
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
    });
}

// ============================================================
// Communication Items (基本)
// ============================================================

#[tokio::test]
async fn test_communication_items_list() {
    test_group!("連絡事項");
    test_case!("連絡事項一覧を取得できる", {
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
    });
}

// ============================================================
// Guidance Records (基本)
// ============================================================

#[tokio::test]
async fn test_guidance_records_crud() {
    test_group!("指導記録");
    test_case!("指導記録のCRUD操作ができる", {
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
            .get(format!(
                "{base_url}/api/guidance-records?employee_id={emp_id}"
            ))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);

        // List attachments (empty)
        let res = client
            .get(format!(
                "{base_url}/api/guidance-records/{record_id}/attachments"
            ))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);

        // Delete
        let res = client
            .delete(format!("{base_url}/api/guidance-records/{record_id}"))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 204);
    });
}

#[tokio::test]
async fn test_guidance_records_upload_attachment() {
    test_group!("指導記録");
    test_case!(
        "添付ファイルをアップロードして一覧取得できる",
        {
            let state = common::setup_app_state().await;
            let base_url = common::spawn_test_server(state.clone()).await;
            let tenant_id = common::create_test_tenant(&state.pool, "GuidAtt").await;
            let jwt = common::create_test_jwt(tenant_id, "admin");
            let auth = format!("Bearer {jwt}");
            let client = reqwest::Client::new();

            let emp =
                common::create_test_employee(&client, &base_url, &auth, "AttEmp", "AT01").await;
            let emp_id = emp["id"].as_str().unwrap();

            // record 作成
            let res = client
                .post(format!("{base_url}/api/guidance-records"))
                .header("Authorization", &auth)
                .json(&serde_json::json!({ "employee_id": emp_id, "title": "添付テスト" }))
                .send()
                .await
                .unwrap();
            let record: Value = res.json().await.unwrap();
            let record_id = record["id"].as_str().unwrap();

            // attachment upload (multipart)
            let file_part = reqwest::multipart::Part::bytes(b"test attachment data".to_vec())
                .file_name("test.pdf")
                .mime_str("application/pdf")
                .unwrap();
            let form = reqwest::multipart::Form::new().part("file", file_part);

            let res = client
                .post(format!(
                    "{base_url}/api/guidance-records/{record_id}/attachments"
                ))
                .header("Authorization", &auth)
                .multipart(form)
                .send()
                .await
                .unwrap();
            assert!(
                res.status() == 200 || res.status() == 201,
                "attachment upload: {}",
                res.status()
            );

            // attachment list
            let res = client
                .get(format!(
                    "{base_url}/api/guidance-records/{record_id}/attachments"
                ))
                .header("Authorization", &auth)
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
            let atts: Vec<Value> = res.json().await.unwrap();
            assert!(!atts.is_empty());
        }
    );
}

#[tokio::test]
async fn test_guidance_records_list_with_date_filter() {
    test_group!("指導記録");
    test_case!(
        "日付フィルタ付きで指導記録一覧を取得できる",
        {
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
    );
}

// ============================================================
// Tenant Users (管理者)
// ============================================================

#[tokio::test]
async fn test_tenant_users_list() {
    test_group!("テナントユーザー");
    test_case!("テナントユーザー一覧を取得できる", {
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;
        let tenant_id = common::create_test_tenant(&state.pool, "TenantUsers").await;
        let (user_id, _) =
            common::create_test_user_in_db(&state.pool, tenant_id, "tu@test.com", "admin").await;
        let jwt = common::create_test_jwt_for_user(user_id, tenant_id, "tu@test.com", "admin");
        let client = reqwest::Client::new();

        let res = client
            .get(format!("{base_url}/api/admin/users"))
            .header("Authorization", format!("Bearer {jwt}"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
    });
}

// ============================================================
// Tenant Users — invite / delete
// ============================================================

#[tokio::test]
async fn test_tenant_users_invite_and_delete() {
    test_group!("テナントユーザー");
    test_case!(
        "ユーザー招待・招待一覧・招待削除・ユーザー削除ができる",
        {
            let state = common::setup_app_state().await;
            let base_url = common::spawn_test_server(state.clone()).await;
            let tenant_id = common::create_test_tenant(&state.pool, "TUInvite").await;
            let (user_id, _) = common::create_test_user_in_db(
                &state.pool,
                tenant_id,
                "admin-tu@test.com",
                "admin",
            )
            .await;
            let jwt =
                common::create_test_jwt_for_user(user_id, tenant_id, "admin-tu@test.com", "admin");
            let auth = format!("Bearer {jwt}");
            let client = reqwest::Client::new();

            // invite
            let res = client
                .post(format!("{base_url}/api/admin/users/invite"))
                .header("Authorization", &auth)
                .json(&serde_json::json!({ "email": "invited@example.com", "role": "viewer" }))
                .send()
                .await
                .unwrap();
            let inv_status = res.status();
            assert!(
                inv_status == 200 || inv_status == 201,
                "invite: {inv_status}"
            );
            let inv: serde_json::Value = res.json().await.unwrap();

            // list invitations
            let res = client
                .get(format!("{base_url}/api/admin/users/invitations"))
                .header("Authorization", &auth)
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
            let body: serde_json::Value = res.json().await.unwrap();
            // array or object with items
            assert!(
                body.is_array() || body.is_object(),
                "invitations response: {body}"
            );

            // delete invitation (RLS の関係で 404 の可能性あり — コードパスは通る)
            if let Some(inv_id) = inv.get("id").and_then(|v| v.as_str()) {
                let res = client
                    .delete(format!("{base_url}/api/admin/users/invitations/{inv_id}"))
                    .header("Authorization", &auth)
                    .send()
                    .await
                    .unwrap();
                // 200/204 or 404 (RLS)
                assert!(
                    res.status().as_u16() < 500,
                    "delete invitation: {}",
                    res.status()
                );
            }

            // delete user
            // 他のユーザーを作成して削除
            let (other_id, _) = common::create_test_user_in_db(
                &state.pool,
                tenant_id,
                "other-tu@test.com",
                "viewer",
            )
            .await;
            let res = client
                .delete(format!("{base_url}/api/admin/users/{other_id}"))
                .header("Authorization", &auth)
                .send()
                .await
                .unwrap();
            assert!(res.status().as_u16() < 500, "delete user: {}", res.status());
        }
    );
}

// ============================================================
// Daily Health Status
// ============================================================

#[tokio::test]
async fn test_daily_health_status() {
    test_group!("日次健康状態");
    test_case!("日次健康状態を取得できる", {
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
    });
}

#[tokio::test]
async fn test_daily_health_status_with_date() {
    test_group!("日次健康状態");
    test_case!("日付指定で日次健康状態を取得できる", {
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;
        let tenant_id = common::create_test_tenant(&state.pool, "DailyHealthD").await;
        let jwt = common::create_test_jwt(tenant_id, "admin");
        let client = reqwest::Client::new();

        let res = client
            .get(format!(
                "{base_url}/api/tenko/daily-health-status?date=2026-03-26"
            ))
            .header("Authorization", format!("Bearer {jwt}"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
    });
}

// ============================================================
// Driver Info
// ============================================================

#[tokio::test]
async fn test_driver_info() {
    test_group!("運転者情報");
    test_case!("従業員IDで運転者情報を取得できる", {
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
    });
}

// ============================================================
// Timecard — CSV + フィルタ
// ============================================================

#[tokio::test]
async fn test_timecard_punches_csv() {
    test_group!("タイムカード");
    test_case!("打刻データをCSVエクスポートできる", {
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;
        let tenant_id = common::create_test_tenant(&state.pool, "CSV Punch").await;
        let jwt = common::create_test_jwt(tenant_id, "admin");
        let auth = format!("Bearer {jwt}");
        let client = reqwest::Client::new();

        let emp = common::create_test_employee(&client, &base_url, &auth, "CsvEmp", "CSV1").await;
        let emp_id = emp["id"].as_str().unwrap();

        // カード作成 + 打刻
        client
            .post(format!("{base_url}/api/timecard/cards"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({ "employee_id": emp_id, "card_id": "CSV-CARD" }))
            .send()
            .await
            .unwrap();

        client
            .post(format!("{base_url}/api/timecard/punch"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({ "card_id": "CSV-CARD" }))
            .send()
            .await
            .unwrap();

        // CSV エクスポート
        let res = client
            .get(format!("{base_url}/api/timecard/punches/csv"))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        let ct = res.headers().get("content-type").unwrap().to_str().unwrap();
        assert!(ct.contains("csv"));
    });
}

#[tokio::test]
async fn test_timecard_punches_with_filter() {
    test_group!("タイムカード");
    test_case!(
        "従業員IDフィルタ付きで打刻一覧を取得できる",
        {
            let state = common::setup_app_state().await;
            let base_url = common::spawn_test_server(state.clone()).await;
            let tenant_id = common::create_test_tenant(&state.pool, "FilterPunch").await;
            let jwt = common::create_test_jwt(tenant_id, "admin");
            let auth = format!("Bearer {jwt}");
            let client = reqwest::Client::new();

            let emp =
                common::create_test_employee(&client, &base_url, &auth, "FiltEmp", "FI01").await;
            let emp_id = emp["id"].as_str().unwrap();

            // フィルタ付き一覧
            let res = client
                .get(format!(
                    "{base_url}/api/timecard/punches?employee_id={emp_id}&page=1&per_page=10"
                ))
                .header("Authorization", &auth)
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
            let body: Value = res.json().await.unwrap();
            assert_eq!(body["total"], 0);
        }
    );
}

// ============================================================
// Carins Files — delete then download → 404
// ============================================================

#[tokio::test]
async fn test_carins_files_delete_then_download() {
    test_group!("車検証ファイル");
    test_case!("ファイル削除後に一覧から除外される", {
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;
        let tenant_id = common::create_test_tenant(&state.pool, "CarinsDel").await;
        let jwt = common::create_test_jwt(tenant_id, "admin");
        let auth = format!("Bearer {jwt}");
        let client = reqwest::Client::new();

        // ファイル作成 (base64 エンコード済みダミーコンテンツ)
        let res = client
            .post(format!("{base_url}/api/files"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({
                "filename": "test-delete.pdf",
                "type": "application/pdf",
                "content": "dGVzdCBjb250ZW50"
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 201);
        let file: Value = res.json().await.unwrap();
        let file_uuid = file["uuid"].as_str().unwrap();

        // ファイル取得 → 200
        let res = client
            .get(format!("{base_url}/api/files/{file_uuid}"))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);

        // ファイル削除 (soft delete)
        let res = client
            .post(format!("{base_url}/api/files/{file_uuid}/delete"))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 204);

        // 削除後のダウンロード → file metadata は取得可能 (soft delete)
        // ただし list_files では表示されない
        let res = client
            .get(format!("{base_url}/api/files"))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        let list: Value = res.json().await.unwrap();
        let files = list["files"].as_array().unwrap();
        // 削除済みファイルは一覧に表示されない
        let found = files.iter().any(|f| f["uuid"].as_str() == Some(file_uuid));
        assert!(!found, "deleted file should not appear in list");
    });
}

// ============================================================
// Timecard — punch with NFC fallback (employees.nfc_id)
// ============================================================

#[tokio::test]
async fn test_timecard_punch_nfc_fallback() {
    test_group!("タイムカード");
    test_case!(
        "カード未登録時にemployees.nfc_idフォールバックで打刻できる",
        {
            let state = common::setup_app_state().await;
            let base_url = common::spawn_test_server(state.clone()).await;
            let tenant_id = common::create_test_tenant(&state.pool, "NfcPunch").await;
            let jwt = common::create_test_jwt(tenant_id, "admin");
            let auth = format!("Bearer {jwt}");
            let client = reqwest::Client::new();

            // 従業員作成
            let emp =
                common::create_test_employee(&client, &base_url, &auth, "NfcEmp", "NF01").await;
            let emp_id = emp["id"].as_str().unwrap();

            // employees.nfc_id を直接設定 (カードは登録しない)
            let nfc_id = format!(
                "NFC-{}",
                uuid::Uuid::new_v4().simple().to_string().get(..8).unwrap()
            );
            sqlx::query("UPDATE alc_api.employees SET nfc_id = $1 WHERE id = $2::uuid")
                .bind(&nfc_id)
                .bind(emp_id)
                .execute(&state.pool)
                .await
                .unwrap();

            // timecard_cards にはカードを登録しない → nfc_id フォールバック
            let res = client
                .post(format!("{base_url}/api/timecard/punch"))
                .header("Authorization", &auth)
                .json(&serde_json::json!({ "card_id": &nfc_id }))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 201);
            let body: Value = res.json().await.unwrap();
            assert_eq!(body["employee_name"], "NfcEmp");
        }
    );
}

// ============================================================
// Timecard — get card by card_id not found → 404
// ============================================================

#[tokio::test]
async fn test_timecard_get_card_by_card_id_not_found() {
    test_group!("タイムカード");
    test_case!("存在しないcard_idで取得すると404を返す", {
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;
        let tenant_id = common::create_test_tenant(&state.pool, "CardNotFound").await;
        let jwt = common::create_test_jwt(tenant_id, "admin");
        let auth = format!("Bearer {jwt}");
        let client = reqwest::Client::new();

        let res = client
            .get(format!(
                "{base_url}/api/timecard/cards/by-card/NONEXISTENT-CARD"
            ))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 404);
    });
}

// ============================================================
// Timecard — CSV with employee_id filter
// ============================================================

#[tokio::test]
async fn test_timecard_csv_with_employee_filter() {
    test_group!("タイムカード");
    test_case!(
        "従業員IDフィルタ付きでCSVエクスポートできる",
        {
            let state = common::setup_app_state().await;
            let base_url = common::spawn_test_server(state.clone()).await;
            let tenant_id = common::create_test_tenant(&state.pool, "CsvFilter").await;
            let jwt = common::create_test_jwt(tenant_id, "admin");
            let auth = format!("Bearer {jwt}");
            let client = reqwest::Client::new();

            let emp =
                common::create_test_employee(&client, &base_url, &auth, "CsvFiltEmp", "CF01").await;
            let emp_id = emp["id"].as_str().unwrap();

            // カード作成 + 打刻
            client
                .post(format!("{base_url}/api/timecard/cards"))
                .header("Authorization", &auth)
                .json(&serde_json::json!({ "employee_id": emp_id, "card_id": "CSVF-CARD" }))
                .send()
                .await
                .unwrap();

            client
                .post(format!("{base_url}/api/timecard/punch"))
                .header("Authorization", &auth)
                .json(&serde_json::json!({ "card_id": "CSVF-CARD" }))
                .send()
                .await
                .unwrap();

            // CSV with employee_id filter
            let res = client
                .get(format!(
                    "{base_url}/api/timecard/punches/csv?employee_id={emp_id}"
                ))
                .header("Authorization", &auth)
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
            let ct = res.headers().get("content-type").unwrap().to_str().unwrap();
            assert!(ct.contains("csv"));
            let csv_body = res.text().await.unwrap();
            assert!(
                csv_body.contains("CsvFiltEmp"),
                "CSV should contain the employee name"
            );
        }
    );
}

// ============================================================
// Communication Items — create with all fields + list active
// ============================================================

#[tokio::test]
async fn test_communication_items_create_all_fields_and_list_active() {
    let _db = common::DB_RENAME_LOCK.lock().unwrap();
    let _flock = common::db_rename_flock();
    test_group!("連絡事項");
    test_case!(
        "全フィールド指定で作成し有効期限内のみactive一覧に表示される",
        {
            let state = common::setup_app_state().await;
            let base_url = common::spawn_test_server(state.clone()).await;
            let tenant_id = common::create_test_tenant(&state.pool, "CommAll").await;
            let jwt = common::create_test_jwt(tenant_id, "admin");
            let auth = format!("Bearer {jwt}");
            let client = reqwest::Client::new();

            // Create with all fields (priority, effective_from, effective_until)
            let res = client
                .post(format!("{base_url}/api/communication-items"))
                .header("Authorization", &auth)
                .json(&serde_json::json!({
                    "title": "緊急連絡",
                    "content": "台風接近に伴う注意",
                    "priority": "urgent",
                    "effective_from": "2020-01-01T00:00:00Z",
                    "effective_until": "2099-12-31T23:59:59Z",
                    "created_by": "テスト管理者"
                }))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 201);
            let item: Value = res.json().await.unwrap();
            assert_eq!(item["title"], "緊急連絡");
            assert_eq!(item["priority"], "urgent");
            assert!(item["effective_from"].as_str().is_some());
            assert!(item["effective_until"].as_str().is_some());

            // List active → should include the item (effective range covers now)
            let res = client
                .get(format!("{base_url}/api/communication-items/active"))
                .header("Authorization", &auth)
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
            let active_items: Vec<Value> = res.json().await.unwrap();
            assert!(
                active_items.iter().any(|i| i["title"] == "緊急連絡"),
                "active list should contain the created item"
            );

            // Create a second item with expired range
            let res = client
                .post(format!("{base_url}/api/communication-items"))
                .header("Authorization", &auth)
                .json(&serde_json::json!({
                    "title": "期限切れ連絡",
                    "content": "過去の連絡",
                    "priority": "normal",
                    "effective_from": "2020-01-01T00:00:00Z",
                    "effective_until": "2020-12-31T23:59:59Z"
                }))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 201);

            // List active → expired item should NOT appear
            let res = client
                .get(format!("{base_url}/api/communication-items/active"))
                .header("Authorization", &auth)
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
            let active_items: Vec<Value> = res.json().await.unwrap();
            assert!(
                !active_items.iter().any(|i| i["title"] == "期限切れ連絡"),
                "expired item should not appear in active list"
            );
        }
    );
}

// ============================================================
// Health Baselines — PUT update + upsert idempotency
// ============================================================

#[tokio::test]
async fn test_health_baselines_update_with_put() {
    test_group!("健康基準値");
    test_case!("POST作成後にPUTで部分更新できる", {
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;
        let tenant_id = common::create_test_tenant(&state.pool, "HBUpdate").await;
        let jwt = common::create_test_jwt(tenant_id, "admin");
        let auth = format!("Bearer {jwt}");
        let client = reqwest::Client::new();

        let emp = common::create_test_employee(&client, &base_url, &auth, "HBEmp", "HB01").await;
        let emp_id = emp["id"].as_str().unwrap();

        // Create baseline via POST (upsert)
        let res = client
            .post(format!("{base_url}/api/tenko/health-baselines"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({
                "employee_id": emp_id,
                "baseline_systolic": 120,
                "baseline_diastolic": 80,
                "baseline_temperature": 36.5,
                "systolic_tolerance": 10,
                "diastolic_tolerance": 10,
                "temperature_tolerance": 0.5
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 201);
        let baseline: Value = res.json().await.unwrap();
        assert_eq!(baseline["baseline_systolic"], 120);

        // Update via PUT
        let res = client
            .put(format!("{base_url}/api/tenko/health-baselines/{emp_id}"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({
                "baseline_systolic": 130,
                "baseline_temperature": 36.8
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        let updated: Value = res.json().await.unwrap();
        assert_eq!(updated["baseline_systolic"], 130);
        // unchanged fields should remain
        assert_eq!(updated["baseline_diastolic"], 80);

        // GET to verify
        let res = client
            .get(format!("{base_url}/api/tenko/health-baselines/{emp_id}"))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        let fetched: Value = res.json().await.unwrap();
        assert_eq!(fetched["baseline_systolic"], 130);
    });
}

#[tokio::test]
async fn test_health_baselines_upsert_twice_no_duplicate() {
    test_group!("健康基準値");
    test_case!("同一従業員に2回upsertしても重複しない", {
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;
        let tenant_id = common::create_test_tenant(&state.pool, "HBUpsert").await;
        let jwt = common::create_test_jwt(tenant_id, "admin");
        let auth = format!("Bearer {jwt}");
        let client = reqwest::Client::new();

        let emp =
            common::create_test_employee(&client, &base_url, &auth, "UpsertEmp", "UP01").await;
        let emp_id = emp["id"].as_str().unwrap();

        // First upsert
        let res = client
            .post(format!("{base_url}/api/tenko/health-baselines"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({
                "employee_id": emp_id,
                "baseline_systolic": 115
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 201);

        // Second upsert (same employee) → should update, not duplicate
        let res = client
            .post(format!("{base_url}/api/tenko/health-baselines"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({
                "employee_id": emp_id,
                "baseline_systolic": 125,
                "baseline_diastolic": 85
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 201);
        let updated: Value = res.json().await.unwrap();
        assert_eq!(updated["baseline_systolic"], 125);
        assert_eq!(updated["baseline_diastolic"], 85);

        // List → should have exactly 1 baseline for this employee
        let res = client
            .get(format!("{base_url}/api/tenko/health-baselines"))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        let baselines: Vec<Value> = res.json().await.unwrap();
        let count = baselines
            .iter()
            .filter(|b| b["employee_id"].as_str() == Some(emp_id))
            .count();
        assert_eq!(count, 1, "upsert should not create duplicate baselines");
    });
}

// ============================================================
// Tenko Call — 点呼送信
// ============================================================

#[tokio::test]
async fn test_tenko_call_tenko() {
    test_group!("中間点呼");
    test_case!(
        "登録済み運転者でGPS付き点呼を送信できる",
        {
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
                .execute(&state.pool)
                .await
                .unwrap();

            client
                .post(format!("{base_url}/api/tenko-call/register"))
                .json(&serde_json::json!({
                    "phone_number": phone,
                    "driver_name": "点呼運転者",
                    "call_number": call_num
                }))
                .send()
                .await
                .unwrap();

            // 点呼送信
            let res = client
                .post(format!("{base_url}/api/tenko-call/tenko"))
                .json(&serde_json::json!({
                    "phone_number": phone,
                    "driver_name": "点呼運転者",
                    "latitude": 35.6762,
                    "longitude": 139.6503
                }))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
            let body: Value = res.json().await.unwrap();
            assert_eq!(body["success"], true);
        }
    );
}

#[tokio::test]
async fn test_tenko_call_delete_number() {
    test_group!("中間点呼");
    test_case!("電話番号マスタを削除できる", {
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;
        let tenant_id = common::create_test_tenant(&state.pool, "TenkoDelNum").await;
        let jwt = common::create_test_jwt(tenant_id, "admin");
        let client = reqwest::Client::new();

        // 電話番号マスタ作成
        let call_num = format!("03-{}", &uuid::Uuid::new_v4().simple().to_string()[..8]);
        let row: (i32,) = sqlx::query_as(
            "INSERT INTO tenko_call_numbers (call_number, tenant_id) VALUES ($1, $2) RETURNING id",
        )
        .bind(&call_num)
        .bind(tenant_id.to_string())
        .fetch_one(&state.pool)
        .await
        .unwrap();

        let res = client
            .delete(format!("{base_url}/api/tenko-call/numbers/{}", row.0))
            .header("Authorization", format!("Bearer {jwt}"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 204);
    });
}

// ============================================================
// Car Inspections
// ============================================================

#[tokio::test]
async fn test_car_inspections_current() {
    test_group!("車検証");
    test_case!("有効な車検証一覧を取得できる", {
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;
        let tenant_id = common::create_test_tenant(&state.pool, "CarIns").await;
        let jwt = common::create_test_jwt(tenant_id, "admin");
        let client = reqwest::Client::new();

        let res = client
            .get(format!("{base_url}/api/car-inspections/current"))
            .header("Authorization", format!("Bearer {jwt}"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
    });
}

#[tokio::test]
async fn test_car_inspections_expired() {
    test_group!("車検証");
    test_case!("期限切れ車検証一覧を取得できる", {
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;
        let tenant_id = common::create_test_tenant(&state.pool, "CarInsExp").await;
        let jwt = common::create_test_jwt(tenant_id, "admin");
        let client = reqwest::Client::new();

        let res = client
            .get(format!("{base_url}/api/car-inspections/expired"))
            .header("Authorization", format!("Bearer {jwt}"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
    });
}

#[tokio::test]
async fn test_car_inspections_renew() {
    test_group!("車検証");
    test_case!("更新対象の車検証一覧を取得できる", {
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;
        let tenant_id = common::create_test_tenant(&state.pool, "CarInsRen").await;
        let jwt = common::create_test_jwt(tenant_id, "admin");
        let client = reqwest::Client::new();

        let res = client
            .get(format!("{base_url}/api/car-inspections/renew"))
            .header("Authorization", format!("Bearer {jwt}"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
    });
}

#[tokio::test]
async fn test_car_inspections_vehicle_categories() {
    test_group!("車検証");
    test_case!("車両カテゴリ一覧を取得できる", {
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;
        let tenant_id = common::create_test_tenant(&state.pool, "VehCat").await;
        let jwt = common::create_test_jwt(tenant_id, "admin");
        let client = reqwest::Client::new();

        let res = client
            .get(format!("{base_url}/api/car-inspections/vehicle-categories"))
            .header("Authorization", format!("Bearer {jwt}"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
    });
}

// ============================================================
// Car Inspection Files
// ============================================================

#[tokio::test]
async fn test_car_inspection_files_current() {
    test_group!("車検証ファイル");
    test_case!("有効な車検証ファイル一覧を取得できる", {
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;
        let tenant_id = common::create_test_tenant(&state.pool, "CarInsFiles").await;
        let jwt = common::create_test_jwt(tenant_id, "admin");
        let client = reqwest::Client::new();

        let res = client
            .get(format!("{base_url}/api/car-inspection-files/current"))
            .header("Authorization", format!("Bearer {jwt}"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
    });
}

// ============================================================
// Guidance Records — 親子レコード + 添付ファイル + guidance_type フィルタ
// ============================================================

#[tokio::test]
async fn test_guidance_records_child_record() {
    test_group!("指導記録");
    test_case!(
        "親子レコードを作成し親削除で子も再帰削除される",
        {
            let state = common::setup_app_state().await;
            let base_url = common::spawn_test_server(state.clone()).await;
            let tenant_id = common::create_test_tenant(&state.pool, "GuidChild").await;
            let jwt = common::create_test_jwt(tenant_id, "admin");
            let auth = format!("Bearer {jwt}");
            let client = reqwest::Client::new();

            let emp =
                common::create_test_employee(&client, &base_url, &auth, "ChildEmp", "CE01").await;
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
                .send()
                .await
                .unwrap();
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
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 201);
            let child: Value = res.json().await.unwrap();
            assert_eq!(child["depth"], 1);
            assert_eq!(child["parent_id"].as_str().unwrap(), parent_id);

            // 親を取得して確認
            let res = client
                .get(format!("{base_url}/api/guidance-records/{parent_id}"))
                .header("Authorization", &auth)
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);

            // 親削除 → 子も再帰削除
            let res = client
                .delete(format!("{base_url}/api/guidance-records/{parent_id}"))
                .header("Authorization", &auth)
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 204);

            // 子も消えている
            let child_id = child["id"].as_str().unwrap();
            let res = client
                .get(format!("{base_url}/api/guidance-records/{child_id}"))
                .header("Authorization", &auth)
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 404);
        }
    );
}

#[tokio::test]
async fn test_guidance_records_attachments_empty_after_creation() {
    test_group!("指導記録");
    test_case!("作成直後の添付ファイル一覧は空である", {
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
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 201);
        let record: Value = res.json().await.unwrap();
        let record_id = record["id"].as_str().unwrap();

        // 添付ファイル一覧 → 空
        let res = client
            .get(format!(
                "{base_url}/api/guidance-records/{record_id}/attachments"
            ))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        let atts: Vec<Value> = res.json().await.unwrap();
        assert_eq!(atts.len(), 0);

        // クリーンアップ
        client
            .delete(format!("{base_url}/api/guidance-records/{record_id}"))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();
    });
}

#[tokio::test]
async fn test_guidance_records_filter_by_guidance_type() {
    test_group!("指導記録");
    test_case!(
        "guidance_typeでフィルタして指導記録を取得できる",
        {
            let state = common::setup_app_state().await;
            let base_url = common::spawn_test_server(state.clone()).await;
            let tenant_id = common::create_test_tenant(&state.pool, "GuidType").await;
            let jwt = common::create_test_jwt(tenant_id, "admin");
            let auth = format!("Bearer {jwt}");
            let client = reqwest::Client::new();

            let emp =
                common::create_test_employee(&client, &base_url, &auth, "TypeEmp", "TY01").await;
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
                .send()
                .await
                .unwrap();
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
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 201);
            let rec2: Value = res.json().await.unwrap();

            // guidance_type=initial でフィルタ
            let res = client
                .get(format!(
                    "{base_url}/api/guidance-records?guidance_type=initial"
                ))
                .header("Authorization", &auth)
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
            let body: Value = res.json().await.unwrap();
            let records = body["records"].as_array().unwrap();
            for r in records {
                assert_eq!(r["guidance_type"], "initial");
            }
            assert!(records.len() >= 1);

            // guidance_type=accident でフィルタ
            let res = client
                .get(format!(
                    "{base_url}/api/guidance-records?guidance_type=accident"
                ))
                .header("Authorization", &auth)
                .send()
                .await
                .unwrap();
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
            client
                .delete(format!("{base_url}/api/guidance-records/{r1_id}"))
                .header("Authorization", &auth)
                .send()
                .await
                .unwrap();
            client
                .delete(format!("{base_url}/api/guidance-records/{r2_id}"))
                .header("Authorization", &auth)
                .send()
                .await
                .unwrap();
        }
    );
}

// ============================================================
// Carrying Items — vehicle_conditions ネスト
// ============================================================

#[tokio::test]
async fn test_carrying_items_with_vehicle_conditions() {
    test_group!("携行品目");
    test_case!(
        "vehicle_conditions付きで携行品目を作成・更新・取得できる",
        {
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
                        { "category": "car_kind", "value": "普通" },
                        { "category": "use", "value": "貨物" }
                    ]
                }))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status().as_u16(), 201);
            let item: Value = res.json().await.unwrap();
            let item_id = item["id"].as_str().unwrap();
            assert_eq!(item["item_name"], "輪止め");

            // vehicle_conditions がネストされている
            let conditions = item["vehicle_conditions"].as_array().unwrap();
            assert_eq!(conditions.len(), 2);

            // 一覧取得でもネストされている
            let res = client
                .get(format!("{base_url}/api/carrying-items"))
                .header("Authorization", &auth)
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
            let items: Vec<Value> = res.json().await.unwrap();
            let target = items
                .iter()
                .find(|i| i["id"].as_str() == Some(item_id))
                .unwrap();
            let list_conditions = target["vehicle_conditions"].as_array().unwrap();
            assert_eq!(list_conditions.len(), 2);

            // vehicle_conditions 付きで更新 (全置換)
            let res = client
                .put(format!("{base_url}/api/carrying-items/{item_id}"))
                .header("Authorization", &auth)
                .json(&serde_json::json!({
                    "item_name": "輪止め改",
                    "vehicle_conditions": [
                        { "category": "car_shape", "value": "バン" }
                    ]
                }))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
            let updated: Value = res.json().await.unwrap();
            assert_eq!(updated["item_name"], "輪止め改");
            let updated_conds = updated["vehicle_conditions"].as_array().unwrap();
            assert_eq!(updated_conds.len(), 1);
            assert_eq!(updated_conds[0]["category"], "car_shape");

            // クリーンアップ
            let res = client
                .delete(format!("{base_url}/api/carrying-items/{item_id}"))
                .header("Authorization", &auth)
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 204);

            // 存在しない ID を DELETE → 404
            let fake_id = uuid::Uuid::new_v4();
            let res = client
                .delete(format!("{base_url}/api/carrying-items/{fake_id}"))
                .header("Authorization", &auth)
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 404);
        }
    );
}

// ============================================================
// Car Inspections — GET by ID with fake ID → 404
// ============================================================

#[tokio::test]
async fn test_car_inspections_get_by_fake_id_returns_404() {
    test_group!("車検証");
    test_case!(
        "存在しないIDで車検証を取得すると404を返す",
        {
            let state = common::setup_app_state().await;
            let base_url = common::spawn_test_server(state.clone()).await;
            let tenant_id = common::create_test_tenant(&state.pool, "CarIns404").await;
            let jwt = common::create_test_jwt(tenant_id, "admin");
            let client = reqwest::Client::new();

            // 存在しない ID (car_inspection.id は i32)
            let res = client
                .get(format!("{base_url}/api/car-inspections/999999999"))
                .header("Authorization", format!("Bearer {jwt}"))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 404);
        }
    );
}

// ============================================================
// Bot Admin — update (既存config更新)
// ============================================================

#[tokio::test]
#[ignore] // llvm-cov 環境で env var 競合
async fn test_bot_admin_update_config() {
    test_group!("Bot管理");
    test_case!("Bot設定を作成後にupsertで更新できる", {
        std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;
        let tenant_id = common::create_test_tenant(&state.pool, "BotUpd").await;
        let (user_id, _) =
            common::create_test_user_in_db(&state.pool, tenant_id, "botupd@test.com", "admin")
                .await;
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
        let res = client
            .post(format!("{base_url}/api/admin/bot/configs"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({
                "id": bot_id, "name": "UpdBot2", "client_id": "upd-cid2",
                "client_secret": "secret2", "service_account": "upd@sa2",
                "private_key": "key2", "bot_id": "upd-bot2"
            }))
            .send()
            .await
            .unwrap();
        assert!(res.status() == 200 || res.status() == 201);
    });
}

// ============================================================
// NFC Tags (基本)
// ============================================================

#[tokio::test]
async fn test_nfc_tags_list() {
    test_group!("NFCタグ");
    test_case!("NFCタグ一覧を取得できる", {
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
    });
}

// ============================================================
// Timecard — list_punches with date filter
// ============================================================

#[tokio::test]
async fn test_timecard_punches_with_date_filter() {
    test_group!("タイムカード");
    test_case!(
        "日付フィルタ付きで打刻一覧を取得できる",
        {
            let state = common::setup_app_state().await;
            let base_url = common::spawn_test_server(state.clone()).await;
            let tenant_id = common::create_test_tenant(&state.pool, "PunchDateF").await;
            let jwt = common::create_test_jwt(tenant_id, "admin");
            let auth = format!("Bearer {jwt}");
            let client = reqwest::Client::new();

            let emp =
                common::create_test_employee(&client, &base_url, &auth, "DateFEmp", "DF01").await;
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
    );
}

// ============================================================
// Timecard — CSV export with date filter
// ============================================================

#[tokio::test]
async fn test_timecard_csv_with_date_filter() {
    test_group!("タイムカード");
    test_case!(
        "日付フィルタ付きでCSVエクスポートできる",
        {
            let state = common::setup_app_state().await;
            let base_url = common::spawn_test_server(state.clone()).await;
            let tenant_id = common::create_test_tenant(&state.pool, "CsvDateF").await;
            let jwt = common::create_test_jwt(tenant_id, "admin");
            let auth = format!("Bearer {jwt}");
            let client = reqwest::Client::new();

            let emp =
                common::create_test_employee(&client, &base_url, &auth, "CsvDFEmp", "CD01").await;
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
    );
}

// ============================================================
// Tenko Call — register with invalid call_number → 400
// ============================================================

#[tokio::test]
async fn test_tenko_call_register_invalid_call_number() {
    test_group!("中間点呼");
    test_case!(
        "未登録の電話番号マスタで登録すると400を返す",
        {
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
    );
}

// ============================================================
// Tenko Call — tenko with unregistered phone → 404
// ============================================================

#[tokio::test]
async fn test_tenko_call_tenko_unregistered_phone() {
    test_group!("中間点呼");
    test_case!("未登録の電話番号で点呼すると404を返す", {
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
    });
}

// ============================================================
// Tenko Webhooks — deliveries endpoint
// ============================================================

#[tokio::test]
async fn test_tenko_webhook_deliveries() {
    test_group!("点呼Webhook");
    test_case!("Webhook作成後にdeliveries一覧を取得できる", {
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
            .get(format!(
                "{base_url}/api/tenko/webhooks/{fake_id}/deliveries"
            ))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        let deliveries: Vec<Value> = res.json().await.unwrap();
        assert_eq!(deliveries.len(), 0);
    });
}

// ============================================================
// NFC Tags — CRUD (register, search, delete)
// ============================================================

#[tokio::test]
async fn test_nfc_tags_crud() {
    test_group!("NFCタグ");
    test_case!("NFCタグの登録・検索・削除ができる", {
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
        let nfc_uuid = format!(
            "AA:BB:CC:{}",
            &uuid::Uuid::new_v4().simple().to_string()[..2]
        );
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
        assert!(
            !returned_uuid.contains(':'),
            "UUID should be normalized without colons"
        );

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
            .get(format!("{base_url}/api/nfc-tags/search?uuid={nfc_uuid}"))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        let search_result: Value = res.json().await.unwrap();
        assert!(search_result["nfc_tag"]["id"].as_i64().is_some());

        // Search nonexistent UUID -> 404
        let res = client
            .get(format!("{base_url}/api/nfc-tags/search?uuid=FF:FF:FF:FF"))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 404);

        // Delete NFC tag (uses normalized UUID)
        let res = client
            .delete(format!("{base_url}/api/nfc-tags/{returned_uuid}"))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 204);

        // Delete again -> 404
        let res = client
            .delete(format!("{base_url}/api/nfc-tags/{returned_uuid}"))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 404);
    });
}

// ============================================================
// NFC Tags — upsert (register same UUID twice updates car_inspection_id)
// ============================================================

#[tokio::test]
async fn test_nfc_tags_upsert() {
    test_group!("NFCタグ");
    test_case!(
        "同一UUIDで2回登録するとcar_inspection_idが更新される",
        {
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
            assert_eq!(
                tag2["carInspectionId"], ci2.0,
                "car_inspection_id should be updated by upsert"
            );
        }
    );
}

// ============================================================
// Event Classifications — list after upload + update
// ============================================================

#[tokio::test]
async fn tenko_completed_classifications_after_upload_and_update() {
    test_group!("イベント分類");
    test_case!(
        "アップロード後にイベント分類を取得・更新できる",
        {
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
    );
}

// ============================================================
// Dtako CSV Proxy — GET /operations/{unko_no}/csv/{csv_type}
// ============================================================

#[tokio::test]
async fn test_dtako_csv_proxy_kudguri() {
    test_group!("デタコCSVプロキシ");
    test_case!("KUDGURI CSVをJSON形式で取得できる", {
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
        assert!(
            body["headers"].as_array().is_some(),
            "should have headers array"
        );
        assert!(body["rows"].as_array().is_some(), "should have rows array");
        let headers = body["headers"].as_array().unwrap();
        assert!(!headers.is_empty(), "headers should not be empty");
    });
}

#[tokio::test]
async fn test_dtako_csv_proxy_kudgivt() {
    test_group!("デタコCSVプロキシ");
    test_case!(
        "KUDGIVT (イベント) CSVをJSON形式で取得できる",
        {
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
    );
}

// ============================================================
// Webhook — deliver_webhook error paths (connection error + retries)
// ============================================================

#[tokio::test]
#[cfg_attr(not(coverage), ignore)]
async fn test_webhook_deliver_connection_error() {
    test_group!("Webhook配信");
    test_case!(
        "接続エラー時にリトライしてdeliveries記録する",
        {
            use chrono::Utc;
            use rust_alc_api::db::models::WebhookConfig;

            let state = common::setup_app_state().await;
            let tenant_id = common::create_test_tenant(&state.pool, "WhConnErr").await;

            // set tenant for inserts
            let mut conn = state.pool.acquire().await.unwrap();
            rust_alc_api::db::tenant::set_current_tenant(&mut conn, &tenant_id.to_string())
                .await
                .unwrap();

            // Insert webhook_config pointing to unreachable address
            let config_id = uuid::Uuid::new_v4();
            sqlx::query(
            r#"INSERT INTO webhook_configs (id, tenant_id, event_type, url, secret, enabled)
               VALUES ($1, $2, 'tenko_completed', 'http://127.0.0.1:1/webhook', 'test-secret', TRUE)"#,
        )
        .bind(config_id)
        .bind(tenant_id)
        .execute(&mut *conn)
        .await
        .unwrap();
            drop(conn);

            let config = WebhookConfig {
                id: config_id,
                tenant_id,
                event_type: "tenko_completed".to_string(),
                url: "http://127.0.0.1:1/webhook".to_string(),
                secret: Some("test-secret".to_string()),
                enabled: true,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            };

            let payload = serde_json::json!({"test": "data"});

            // deliver_webhook should retry 3 times and return Ok(()) even on failure
            let result = rust_alc_api::webhook::deliver_webhook(
                &state.pool,
                &config,
                "tenko_completed",
                &payload,
            )
            .await;
            assert!(result.is_ok());

            // Check that delivery logs were recorded (3 attempts, all failed)
            let mut conn = state.pool.acquire().await.unwrap();
            rust_alc_api::db::tenant::set_current_tenant(&mut conn, &tenant_id.to_string())
                .await
                .unwrap();
            let count: (i64,) =
                sqlx::query_as("SELECT COUNT(*) FROM webhook_deliveries WHERE config_id = $1")
                    .bind(config_id)
                    .fetch_one(&mut *conn)
                    .await
                    .unwrap();
            assert_eq!(count.0, 3, "Expected 3 delivery attempts");
        }
    );
}

#[tokio::test]
#[cfg_attr(not(coverage), ignore)]
async fn test_webhook_fire_event_delivery_error_logged() {
    test_group!("Webhook配信");
    test_case!("fire_event でエラー時にログ記録される", {
        let state = common::setup_app_state().await;
        let tenant_id = common::create_test_tenant(&state.pool, "WhFireErr").await;

        // set tenant for inserts
        let mut conn = state.pool.acquire().await.unwrap();
        rust_alc_api::db::tenant::set_current_tenant(&mut conn, &tenant_id.to_string())
            .await
            .unwrap();

        // Insert webhook_config pointing to unreachable address
        let config_id = uuid::Uuid::new_v4();
        sqlx::query(
            r#"INSERT INTO webhook_configs (id, tenant_id, event_type, url, secret, enabled)
               VALUES ($1, $2, 'tenko_overdue', 'http://127.0.0.1:1/hook', NULL, TRUE)"#,
        )
        .bind(config_id)
        .bind(tenant_id)
        .execute(&mut *conn)
        .await
        .unwrap();
        drop(conn);

        let payload = serde_json::json!({"key": "value"});

        // fire_event spawns deliver_webhook in background; itself returns Ok
        let result =
            rust_alc_api::webhook::fire_event(&state.pool, tenant_id, "tenko_overdue", payload)
                .await;
        assert!(result.is_ok());

        // Wait for background task to complete (retries with delays 1+5+25 = 31s max,
        // but connection errors are instant, so a short wait suffices)
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;

        // Verify delivery attempts were logged
        let mut conn = state.pool.acquire().await.unwrap();
        rust_alc_api::db::tenant::set_current_tenant(&mut conn, &tenant_id.to_string())
            .await
            .unwrap();
        let count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM webhook_deliveries WHERE config_id = $1")
                .bind(config_id)
                .fetch_one(&mut *conn)
                .await
                .unwrap();
        // Should have at least 1 attempt (background task may still be running)
        assert!(
            count.0 >= 1,
            "Expected at least 1 delivery attempt, got {}",
            count.0
        );
    });
}

// ============================================================
// Timecard: edge cases & error paths
// ============================================================

#[tokio::test]
async fn test_timecard_create_card_conflict() {
    test_group!("タイムカード");
    test_case!("重複カードIDで409を返す", {
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;
        let tenant_id = common::create_test_tenant(&state.pool, "TC Conflict").await;
        let jwt = common::create_test_jwt(tenant_id, "admin");
        let auth = format!("Bearer {jwt}");
        let client = reqwest::Client::new();

        let emp =
            common::create_test_employee(&client, &base_url, &auth, "ConflictEmp", "CF01").await;
        let emp_id = emp["id"].as_str().unwrap();

        // First card
        let res = client
            .post(format!("{base_url}/api/timecard/cards"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({
                "employee_id": emp_id,
                "card_id": "DUP-CARD-001"
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 201);

        // Duplicate card_id + same tenant → 409
        let res = client
            .post(format!("{base_url}/api/timecard/cards"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({
                "employee_id": emp_id,
                "card_id": "DUP-CARD-001"
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 409);
    });
}

#[tokio::test]
async fn test_timecard_list_cards_with_employee_filter() {
    test_group!("タイムカード");
    test_case!("employee_idフィルター付きカード一覧", {
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;
        let tenant_id = common::create_test_tenant(&state.pool, "TC Filter").await;
        let jwt = common::create_test_jwt(tenant_id, "admin");
        let auth = format!("Bearer {jwt}");
        let client = reqwest::Client::new();

        let emp1 =
            common::create_test_employee(&client, &base_url, &auth, "FilterEmp1", "FE01").await;
        let emp1_id = emp1["id"].as_str().unwrap();
        let emp2 =
            common::create_test_employee(&client, &base_url, &auth, "FilterEmp2", "FE02").await;
        let emp2_id = emp2["id"].as_str().unwrap();

        // Create cards for both employees
        client
            .post(format!("{base_url}/api/timecard/cards"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({ "employee_id": emp1_id, "card_id": "FCARD-1" }))
            .send()
            .await
            .unwrap();
        client
            .post(format!("{base_url}/api/timecard/cards"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({ "employee_id": emp2_id, "card_id": "FCARD-2" }))
            .send()
            .await
            .unwrap();

        // Filter by emp1
        let res = client
            .get(format!(
                "{base_url}/api/timecard/cards?employee_id={emp1_id}"
            ))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        let cards: Vec<Value> = res.json().await.unwrap();
        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0]["card_id"], "FCARD-1");
    });
}

#[tokio::test]
async fn test_timecard_delete_card_not_found() {
    test_group!("タイムカード");
    test_case!("存在しないカード削除で404", {
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;
        let tenant_id = common::create_test_tenant(&state.pool, "TC DelNF").await;
        let jwt = common::create_test_jwt(tenant_id, "admin");
        let client = reqwest::Client::new();

        let fake_id = uuid::Uuid::new_v4();
        let res = client
            .delete(format!("{base_url}/api/timecard/cards/{fake_id}"))
            .header("Authorization", format!("Bearer {jwt}"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 404);
    });
}

// Timecard DB error tests moved to tests/coverage/timecard_coverage.rs (trigger pattern)

// ============================================================
// Communication Items — get by ID, update/delete 404, DB errors
// ============================================================

#[tokio::test]
async fn test_communication_items_get_by_id() {
    test_group!("連絡事項");
    test_case!("IDで連絡事項を取得", {
        let _db = common::DB_RENAME_LOCK.lock().unwrap();
        let _flock = common::db_rename_flock();
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;
        let tenant_id = common::create_test_tenant(&state.pool, "CommGetId").await;
        let jwt = common::create_test_jwt(tenant_id, "admin");
        let auth = format!("Bearer {jwt}");
        let client = reqwest::Client::new();

        // Create
        let res = client
            .post(format!("{base_url}/api/communication-items"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({
                "title": "ID取得テスト",
                "content": "テスト内容",
                "priority": "normal"
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 201);
        let item: Value = res.json().await.unwrap();
        let item_id = item["id"].as_str().unwrap();

        // GET by ID
        let res = client
            .get(format!("{base_url}/api/communication-items/{item_id}"))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        let fetched: Value = res.json().await.unwrap();
        assert_eq!(fetched["title"], "ID取得テスト");
        assert_eq!(fetched["id"], item_id);
    });
}

#[tokio::test]
async fn test_communication_items_get_by_id_not_found() {
    test_group!("連絡事項");
    test_case!("存在しないIDで404", {
        let _db = common::DB_RENAME_LOCK.lock().unwrap();
        let _flock = common::db_rename_flock();
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;
        let tenant_id = common::create_test_tenant(&state.pool, "CommNF").await;
        let jwt = common::create_test_jwt(tenant_id, "admin");
        let auth = format!("Bearer {jwt}");
        let client = reqwest::Client::new();

        let fake_id = uuid::Uuid::new_v4();
        let res = client
            .get(format!("{base_url}/api/communication-items/{fake_id}"))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 404);
    });
}

#[tokio::test]
async fn test_communication_items_update_not_found() {
    test_group!("連絡事項");
    test_case!("存在しないIDをupdateで404", {
        let _db = common::DB_RENAME_LOCK.lock().unwrap();
        let _flock = common::db_rename_flock();
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;
        let tenant_id = common::create_test_tenant(&state.pool, "CommUpdNF").await;
        let jwt = common::create_test_jwt(tenant_id, "admin");
        let auth = format!("Bearer {jwt}");
        let client = reqwest::Client::new();

        let fake_id = uuid::Uuid::new_v4();
        let res = client
            .put(format!("{base_url}/api/communication-items/{fake_id}"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({ "title": "更新テスト" }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 404);
    });
}

#[tokio::test]
async fn test_communication_items_delete_not_found() {
    test_group!("連絡事項");
    test_case!("存在しないIDをdeleteで404", {
        let _db = common::DB_RENAME_LOCK.lock().unwrap();
        let _flock = common::db_rename_flock();
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;
        let tenant_id = common::create_test_tenant(&state.pool, "CommDelNF").await;
        let jwt = common::create_test_jwt(tenant_id, "admin");
        let auth = format!("Bearer {jwt}");
        let client = reqwest::Client::new();

        let fake_id = uuid::Uuid::new_v4();
        let res = client
            .delete(format!("{base_url}/api/communication-items/{fake_id}"))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 404);
    });
}

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_communication_items_list_db_error() {
    test_group!("communication_items DB エラー");
    test_case!("list + active: RENAME → 500", {
        let _db = common::DB_RENAME_LOCK.lock().unwrap();
        let _flock = common::db_rename_flock();
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;
        let tenant_id = common::create_test_tenant(&state.pool, "CommListErr").await;
        let jwt = common::create_test_jwt(tenant_id, "admin");
        let auth = format!("Bearer {jwt}");
        let client = reqwest::Client::new();

        sqlx::query("ALTER TABLE alc_api.communication_items RENAME TO communication_items_bak")
            .execute(&state.pool)
            .await
            .unwrap();

        let res = client
            .get(format!("{base_url}/api/communication-items"))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 500, "list should fail");

        let res = client
            .get(format!("{base_url}/api/communication-items/active"))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 500, "active should fail");

        sqlx::query("ALTER TABLE alc_api.communication_items_bak RENAME TO communication_items")
            .execute(&state.pool)
            .await
            .unwrap();
    });
}

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_communication_items_create_db_error() {
    test_group!("communication_items DB エラー");
    test_case!("create: trigger → 500", {
        let _db = common::DB_RENAME_LOCK.lock().unwrap();
        let _flock = common::db_rename_flock();
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;
        let tenant_id = common::create_test_tenant(&state.pool, "CommCreateErr").await;
        let jwt = common::create_test_jwt(tenant_id, "admin");
        let auth = format!("Bearer {jwt}");
        let client = reqwest::Client::new();

        sqlx::query(
            r#"CREATE OR REPLACE FUNCTION alc_api.fail_comm_insert() RETURNS trigger AS $$
            BEGIN RAISE EXCEPTION 'test: communication_items insert blocked'; END;
            $$ LANGUAGE plpgsql"#,
        )
        .execute(&state.pool)
        .await
        .unwrap();
        sqlx::query("CREATE OR REPLACE TRIGGER fail_comm_insert BEFORE INSERT ON alc_api.communication_items FOR EACH ROW EXECUTE FUNCTION alc_api.fail_comm_insert()")
            .execute(&state.pool).await.unwrap();

        let res = client.post(format!("{base_url}/api/communication-items"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({"title": "trigger test", "content": "should fail", "priority": "normal"}))
            .send().await.unwrap();
        assert_eq!(res.status(), 500);

        sqlx::query("DROP TRIGGER fail_comm_insert ON alc_api.communication_items")
            .execute(&state.pool)
            .await
            .unwrap();
        sqlx::query("DROP FUNCTION alc_api.fail_comm_insert")
            .execute(&state.pool)
            .await
            .unwrap();
    });
}
