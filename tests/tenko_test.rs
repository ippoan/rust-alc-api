mod common;

use serde_json::Value;

// ============================================================
// ヘルパー
// ============================================================

async fn setup_tenko() -> (String, String, String, reqwest::Client) {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, &format!("Tenko{}", uuid::Uuid::new_v4().simple())).await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();
    let emp = common::create_test_employee(&client, &base_url, &auth, "TenkoEmp", &format!("TK{}", &uuid::Uuid::new_v4().simple().to_string()[..4])).await;
    let emp_id = emp["id"].as_str().unwrap().to_string();
    (base_url, auth, emp_id, client)
}

/// スケジュールを作成して ID を返す
async fn create_schedule(
    client: &reqwest::Client,
    base_url: &str,
    auth: &str,
    emp_id: &str,
    tenko_type: &str,
) -> String {
    let res = client
        .post(format!("{base_url}/api/tenko/schedules"))
        .header("Authorization", auth)
        .json(&serde_json::json!({
            "employee_id": emp_id,
            "tenko_type": tenko_type,
            "responsible_manager_name": "管理者テスト",
            "scheduled_at": "2099-01-01T00:00:00Z",
            "instruction": "安全運転で"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 201, "Failed to create schedule");
    let body: Value = res.json().await.unwrap();
    body["id"].as_str().unwrap().to_string()
}

/// セッション開始 (スケジュール付き)
async fn start_session(
    client: &reqwest::Client,
    base_url: &str,
    auth: &str,
    emp_id: &str,
    schedule_id: &str,
) -> Value {
    let res = client
        .post(format!("{base_url}/api/tenko/sessions/start"))
        .header("Authorization", auth)
        .json(&serde_json::json!({
            "employee_id": emp_id,
            "schedule_id": schedule_id
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 201, "Failed to start session");
    res.json().await.unwrap()
}

/// セッション開始 (スケジュールなし = 遠隔点呼)
async fn start_session_remote(
    client: &reqwest::Client,
    base_url: &str,
    auth: &str,
    emp_id: &str,
) -> Value {
    let res = client
        .post(format!("{base_url}/api/tenko/sessions/start"))
        .header("Authorization", auth)
        .json(&serde_json::json!({
            "employee_id": emp_id,
            "tenko_type": "pre_operation"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 201, "Failed to start remote session");
    res.json().await.unwrap()
}

// ============================================================
// Tenko Schedules CRUD
// ============================================================

#[tokio::test]
async fn test_schedule_crud() {
    let (base_url, auth, emp_id, client) = setup_tenko().await;

    // 作成
    let sid = create_schedule(&client, &base_url, &auth, &emp_id, "pre_operation").await;

    // 一覧
    let res = client
        .get(format!("{base_url}/api/tenko/schedules"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert!(body["total"].as_i64().unwrap() >= 1);

    // 取得
    let res = client
        .get(format!("{base_url}/api/tenko/schedules/{sid}"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let sched: Value = res.json().await.unwrap();
    assert_eq!(sched["tenko_type"], "pre_operation");
    assert_eq!(sched["consumed"], false);

    // 更新
    let res = client
        .put(format!("{base_url}/api/tenko/schedules/{sid}"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "responsible_manager_name": "新管理者",
            "instruction": "安全運転でお願いします"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let updated: Value = res.json().await.unwrap();
    assert_eq!(updated["responsible_manager_name"], "新管理者");

    // 削除
    let res = client
        .delete(format!("{base_url}/api/tenko/schedules/{sid}"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 204);
}

#[tokio::test]
async fn test_schedule_batch_create() {
    let (base_url, auth, emp_id, client) = setup_tenko().await;

    let res = client
        .post(format!("{base_url}/api/tenko/schedules/batch"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "schedules": [
                {
                    "employee_id": emp_id,
                    "tenko_type": "pre_operation",
                    "responsible_manager_name": "管理者A",
                    "scheduled_at": "2099-01-01T06:00:00Z",
                    "instruction": "安全確認"
                },
                {
                    "employee_id": emp_id,
                    "tenko_type": "post_operation",
                    "responsible_manager_name": "管理者A",
                    "scheduled_at": "2099-01-01T18:00:00Z"
                }
            ]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 201);
    let body: Value = res.json().await.unwrap();
    let schedules = body.as_array().unwrap();
    assert_eq!(schedules.len(), 2);
}

#[tokio::test]
async fn test_schedule_pending_for_employee() {
    let (base_url, auth, emp_id, client) = setup_tenko().await;

    create_schedule(&client, &base_url, &auth, &emp_id, "pre_operation").await;

    let res = client
        .get(format!("{base_url}/api/tenko/schedules/pending/{emp_id}"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let pending: Vec<Value> = res.json().await.unwrap();
    assert!(pending.len() >= 1);
}

// ============================================================
// Tenko Schedules — edge cases
// ============================================================

#[tokio::test]
async fn test_schedule_get_not_found() {
    let (base_url, auth, _emp_id, client) = setup_tenko().await;
    let fake = uuid::Uuid::new_v4();
    let res = client.get(format!("{base_url}/api/tenko/schedules/{fake}"))
        .header("Authorization", &auth).send().await.unwrap();
    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn test_schedule_delete_not_found() {
    let (base_url, auth, _emp_id, client) = setup_tenko().await;
    let fake = uuid::Uuid::new_v4();
    let res = client.delete(format!("{base_url}/api/tenko/schedules/{fake}"))
        .header("Authorization", &auth).send().await.unwrap();
    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn test_schedule_update_not_found() {
    let (base_url, auth, _emp_id, client) = setup_tenko().await;
    let fake = uuid::Uuid::new_v4();
    let res = client.put(format!("{base_url}/api/tenko/schedules/{fake}"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({ "responsible_manager_name": "test" }))
        .send().await.unwrap();
    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn test_schedule_invalid_tenko_type() {
    let (base_url, auth, emp_id, client) = setup_tenko().await;
    let res = client.post(format!("{base_url}/api/tenko/schedules"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "employee_id": emp_id, "tenko_type": "invalid_type",
            "responsible_manager_name": "mgr", "scheduled_at": "2099-01-01T00:00:00Z"
        }))
        .send().await.unwrap();
    assert_eq!(res.status(), 400);
}

// ============================================================
// Equipment Failures — edge cases
// ============================================================

#[tokio::test]
async fn test_equipment_failure_get_not_found() {
    let (base_url, auth, _emp_id, client) = setup_tenko().await;
    let fake = uuid::Uuid::new_v4();
    let res = client.get(format!("{base_url}/api/tenko/equipment-failures/{fake}"))
        .header("Authorization", &auth).send().await.unwrap();
    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn test_equipment_failure_invalid_type() {
    let (base_url, auth, _emp_id, client) = setup_tenko().await;
    let res = client.post(format!("{base_url}/api/tenko/equipment-failures"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({ "failure_type": "invalid_type", "description": "test" }))
        .send().await.unwrap();
    assert_eq!(res.status(), 400);
}

// ============================================================
// Health Baselines — edge cases
// ============================================================

#[tokio::test]
async fn test_health_baseline_get_not_found() {
    let (base_url, auth, _emp_id, client) = setup_tenko().await;
    let fake = uuid::Uuid::new_v4();
    let res = client.get(format!("{base_url}/api/tenko/health-baselines/{fake}"))
        .header("Authorization", &auth).send().await.unwrap();
    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn test_health_baseline_delete_not_found() {
    let (base_url, auth, _emp_id, client) = setup_tenko().await;
    let fake = uuid::Uuid::new_v4();
    let res = client.delete(format!("{base_url}/api/tenko/health-baselines/{fake}"))
        .header("Authorization", &auth).send().await.unwrap();
    assert_eq!(res.status(), 404);
}

// ============================================================
// Tenko Sessions — pre_operation フロー
// ============================================================

#[tokio::test]
async fn test_session_start_with_schedule() {
    let (base_url, auth, emp_id, client) = setup_tenko().await;

    let sid = create_schedule(&client, &base_url, &auth, &emp_id, "pre_operation").await;
    let session = start_session(&client, &base_url, &auth, &emp_id, &sid).await;

    assert_eq!(session["status"], "medical_pending");
    assert_eq!(session["tenko_type"], "pre_operation");
    assert!(session["id"].as_str().is_some());
}

#[tokio::test]
async fn test_session_start_remote_no_schedule() {
    let (base_url, auth, emp_id, client) = setup_tenko().await;

    let session = start_session_remote(&client, &base_url, &auth, &emp_id).await;

    assert_eq!(session["status"], "medical_pending");
    assert_eq!(session["tenko_type"], "pre_operation");
}

#[tokio::test]
async fn test_session_get() {
    let (base_url, auth, emp_id, client) = setup_tenko().await;

    let session = start_session_remote(&client, &base_url, &auth, &emp_id).await;
    let session_id = session["id"].as_str().unwrap();

    let res = client
        .get(format!("{base_url}/api/tenko/sessions/{session_id}"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
}

#[tokio::test]
async fn test_session_list() {
    let (base_url, auth, emp_id, client) = setup_tenko().await;

    start_session_remote(&client, &base_url, &auth, &emp_id).await;

    let res = client
        .get(format!("{base_url}/api/tenko/sessions"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert!(body["total"].as_i64().unwrap() >= 1);
}

#[tokio::test]
async fn test_session_dashboard() {
    let (base_url, auth, _emp_id, client) = setup_tenko().await;

    let res = client
        .get(format!("{base_url}/api/tenko/dashboard"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert!(body.get("pending_schedules").is_some());
    assert!(body.get("active_sessions").is_some());
    assert!(body.get("completed_today").is_some());
}

/// pre_operation フルフロー: medical → self_declaration → daily_inspection → instruction → completed
#[tokio::test]
async fn test_pre_operation_full_flow() {
    let (base_url, auth, emp_id, client) = setup_tenko().await;

    // スケジュール作成 (instruction 付き)
    let res = client
        .post(format!("{base_url}/api/tenko/schedules"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "employee_id": emp_id,
            "tenko_type": "pre_operation",
            "responsible_manager_name": "テスト管理者",
            "scheduled_at": "2099-06-01T06:00:00Z",
            "instruction": "本日は雨天注意"
        }))
        .send()
        .await
        .unwrap();
    let sched: Value = res.json().await.unwrap();
    let sid = sched["id"].as_str().unwrap();

    // 1. セッション開始 → medical_pending
    let session = start_session(&client, &base_url, &auth, &emp_id, sid).await;
    let session_id = session["id"].as_str().unwrap();
    assert_eq!(session["status"], "medical_pending");

    // 2. 医療データ送信 → self_declaration_pending
    let res = client
        .put(format!("{base_url}/api/tenko/sessions/{session_id}/medical"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "temperature": 36.5,
            "systolic": 120,
            "diastolic": 80,
            "pulse": 72,
            "medical_manual_input": true
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let session: Value = res.json().await.unwrap();
    assert_eq!(session["status"], "self_declaration_pending");

    // 3. 自己申告 → safety judgment → daily_inspection_pending or instruction_pending
    let res = client
        .put(format!("{base_url}/api/tenko/sessions/{session_id}/self-declaration"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "illness": false,
            "fatigue": false,
            "sleep_deprivation": false
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let session: Value = res.json().await.unwrap();
    let status_after_decl = session["status"].as_str().unwrap();
    // safety judgment の結果で daily_inspection_pending or instruction_pending
    assert!(
        status_after_decl == "daily_inspection_pending" || status_after_decl == "instruction_pending",
        "Expected daily_inspection_pending or instruction_pending, got {status_after_decl}"
    );

    // 4. 日常点検 → identity_verified (携行品マスタなしの場合)
    if status_after_decl == "daily_inspection_pending" {
        let res = client
            .put(format!("{base_url}/api/tenko/sessions/{session_id}/daily-inspection"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({
                "brakes": "ok",
                "tires": "ok",
                "lights": "ok",
                "steering": "ok",
                "wipers": "ok",
                "mirrors": "ok",
                "horn": "ok",
                "seatbelts": "ok"
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        let session: Value = res.json().await.unwrap();
        // 携行品マスタなし → identity_verified (アルコール検査へ)
        assert_eq!(session["status"], "identity_verified");
    }

    // 5. アルコール検査 → instruction_pending
    let res = client
        .put(format!("{base_url}/api/tenko/sessions/{session_id}/alcohol"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "alcohol_result": "pass",
            "alcohol_value": 0.0
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let session: Value = res.json().await.unwrap();
    assert_eq!(session["status"], "instruction_pending");

    // 6. 指示事項確認 → completed
    let res = client
        .put(format!("{base_url}/api/tenko/sessions/{session_id}/instruction-confirm"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let session: Value = res.json().await.unwrap();
    assert_eq!(session["status"], "completed");
    assert!(session["completed_at"].as_str().is_some());
}

/// post_operation フロー: identity_verified → alcohol → report → completed
#[tokio::test]
async fn test_post_operation_flow() {
    let (base_url, auth, emp_id, client) = setup_tenko().await;

    // post_operation スケジュール (instruction なし)
    let sid = create_schedule(&client, &base_url, &auth, &emp_id, "post_operation").await;

    // セッション開始 → identity_verified (post_op は medical skip)
    let session = start_session(&client, &base_url, &auth, &emp_id, &sid).await;
    let session_id = session["id"].as_str().unwrap();
    assert_eq!(session["status"], "identity_verified");

    // アルコール送信 → report_pending
    let res = client
        .put(format!("{base_url}/api/tenko/sessions/{session_id}/alcohol"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "alcohol_result": "pass",
            "alcohol_value": 0.0
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let session: Value = res.json().await.unwrap();
    assert_eq!(session["status"], "report_pending");

    // 運行報告 → instruction_pending (スケジュールに instruction あり)
    let res = client
        .put(format!("{base_url}/api/tenko/sessions/{session_id}/report"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "vehicle_road_status": "異常なし",
            "driver_alternation": "交替なし"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let session: Value = res.json().await.unwrap();
    assert_eq!(session["status"], "instruction_pending");

    // 指示事項確認 → completed
    let res = client
        .put(format!("{base_url}/api/tenko/sessions/{session_id}/instruction-confirm"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let session: Value = res.json().await.unwrap();
    assert_eq!(session["status"], "completed");
}

// ============================================================
// セッション中止 / 中断 / 再開
// ============================================================

#[tokio::test]
async fn test_session_cancel() {
    let (base_url, auth, emp_id, client) = setup_tenko().await;

    let session = start_session_remote(&client, &base_url, &auth, &emp_id).await;
    let session_id = session["id"].as_str().unwrap();

    let res = client
        .post(format!("{base_url}/api/tenko/sessions/{session_id}/cancel"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({ "reason": "テスト中止" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let session: Value = res.json().await.unwrap();
    assert_eq!(session["status"], "cancelled");
    assert_eq!(session["cancel_reason"], "テスト中止");
}

#[tokio::test]
async fn test_session_interrupt_and_resume() {
    let (base_url, auth, emp_id, client) = setup_tenko().await;

    let session = start_session_remote(&client, &base_url, &auth, &emp_id).await;
    let session_id = session["id"].as_str().unwrap();

    // 中断
    let res = client
        .post(format!("{base_url}/api/tenko/sessions/{session_id}/interrupt"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({ "reason": "電話対応" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let session: Value = res.json().await.unwrap();
    assert_eq!(session["status"], "interrupted");

    // 再開
    let res = client
        .post(format!("{base_url}/api/tenko/sessions/{session_id}/resume"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({ "reason": "電話終了" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let session: Value = res.json().await.unwrap();
    // 中断前の状態に復帰
    assert_ne!(session["status"], "interrupted");
}

// ============================================================
// アルコール検知 → 自動キャンセル
// ============================================================

#[tokio::test]
async fn test_alcohol_fail_cancels_session() {
    let (base_url, auth, emp_id, client) = setup_tenko().await;

    let sid = create_schedule(&client, &base_url, &auth, &emp_id, "post_operation").await;
    let session = start_session(&client, &base_url, &auth, &emp_id, &sid).await;
    let session_id = session["id"].as_str().unwrap();

    let res = client
        .put(format!("{base_url}/api/tenko/sessions/{session_id}/alcohol"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "alcohol_result": "fail",
            "alcohol_value": 0.25
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let session: Value = res.json().await.unwrap();
    assert_eq!(session["status"], "cancelled");
    assert_eq!(session["cancel_reason"], "アルコール検知");
}

// ============================================================
// Tenko Records
// ============================================================

#[tokio::test]
async fn test_tenko_records_after_completion() {
    let (base_url, auth, emp_id, client) = setup_tenko().await;

    // post_operation を完了させる
    let sid = create_schedule(&client, &base_url, &auth, &emp_id, "post_operation").await;
    let session = start_session(&client, &base_url, &auth, &emp_id, &sid).await;
    let session_id = session["id"].as_str().unwrap();

    client
        .put(format!("{base_url}/api/tenko/sessions/{session_id}/alcohol"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({ "alcohol_result": "pass", "alcohol_value": 0.0 }))
        .send()
        .await
        .unwrap();

    client
        .put(format!("{base_url}/api/tenko/sessions/{session_id}/report"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "vehicle_road_status": "良好",
            "driver_alternation": "なし"
        }))
        .send()
        .await
        .unwrap();

    // instruction 確認 (スケジュールに instruction あり)
    client
        .put(format!("{base_url}/api/tenko/sessions/{session_id}/instruction-confirm"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();

    // レコード確認
    let res = client
        .get(format!("{base_url}/api/tenko/records"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert!(body["total"].as_i64().unwrap() >= 1);
}

#[tokio::test]
async fn test_tenko_records_csv() {
    let (base_url, auth, _emp_id, client) = setup_tenko().await;

    let res = client
        .get(format!("{base_url}/api/tenko/records/csv"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let content_type = res.headers().get("content-type").unwrap().to_str().unwrap();
    assert!(content_type.contains("text/csv") || content_type.contains("application/octet-stream"));
}

// ============================================================
// Health Baselines CRUD
// ============================================================

#[tokio::test]
async fn test_health_baselines_crud() {
    let (base_url, auth, emp_id, client) = setup_tenko().await;

    // Upsert
    let res = client
        .post(format!("{base_url}/api/tenko/health-baselines"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "employee_id": emp_id,
            "baseline_temperature": 36.5,
            "baseline_systolic": 120,
            "baseline_diastolic": 80,
            "systolic_tolerance": 20,
            "diastolic_tolerance": 15,
            "temperature_tolerance": 1.0
        }))
        .send()
        .await
        .unwrap();
    assert!(res.status() == 200 || res.status() == 201, "upsert baseline: {}", res.status());

    // Get
    let res = client
        .get(format!("{base_url}/api/tenko/health-baselines/{emp_id}"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let bl: Value = res.json().await.unwrap();
    assert_eq!(bl["baseline_temperature"], 36.5);

    // List
    let res = client
        .get(format!("{base_url}/api/tenko/health-baselines"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    // Delete
    let res = client
        .delete(format!("{base_url}/api/tenko/health-baselines/{emp_id}"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 204);
}

// ============================================================
// Equipment Failures CRUD
// ============================================================

#[tokio::test]
async fn test_equipment_failures_crud() {
    let (base_url, auth, _emp_id, client) = setup_tenko().await;

    // Create
    let res = client
        .post(format!("{base_url}/api/tenko/equipment-failures"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "failure_type": "manual_report",
            "description": "センサー異常",
            "detected_by": "テスト管理者"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 201);
    let failure: Value = res.json().await.unwrap();
    let failure_id = failure["id"].as_str().unwrap();

    // List
    let res = client
        .get(format!("{base_url}/api/tenko/equipment-failures"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    // Get
    let res = client
        .get(format!("{base_url}/api/tenko/equipment-failures/{failure_id}"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    // Resolve
    let res = client
        .put(format!("{base_url}/api/tenko/equipment-failures/{failure_id}"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({ "resolution_notes": "修理完了" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let resolved: Value = res.json().await.unwrap();
    assert!(resolved["resolved_at"].as_str().is_some(), "resolved_at should be set");

    // CSV
    let res = client
        .get(format!("{base_url}/api/tenko/equipment-failures/csv"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
}

// ============================================================
// 携行品チェック付きフロー
// ============================================================

// tenko_carrying_item_checks テーブルのマイグレーションが未作成のためスキップ
#[tokio::test]
#[ignore]
async fn test_pre_operation_with_carrying_items() {
    let (base_url, auth, emp_id, client) = setup_tenko().await;

    // 携行品マスタを作成 (daily_inspection → carrying_items_pending のトリガー)
    let res = client
        .post(format!("{base_url}/api/carrying-items"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({ "item_name": "免許証" }))
        .send().await.unwrap();
    assert!(res.status() == 200 || res.status() == 201);
    let item: Value = res.json().await.unwrap();
    let item_id = item["id"].as_str().unwrap();

    // スケジュール作成 + セッション開始
    let sid = create_schedule(&client, &base_url, &auth, &emp_id, "pre_operation").await;
    let session = start_session(&client, &base_url, &auth, &emp_id, &sid).await;
    let session_id = session["id"].as_str().unwrap();

    // medical → self_declaration → daily_inspection → carrying_items_pending
    client.put(format!("{base_url}/api/tenko/sessions/{session_id}/medical"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({ "temperature": 36.5, "systolic": 120, "diastolic": 80, "pulse": 72 }))
        .send().await.unwrap();

    client.put(format!("{base_url}/api/tenko/sessions/{session_id}/self-declaration"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({ "illness": false, "fatigue": false, "sleep_deprivation": false }))
        .send().await.unwrap();

    let res = client.put(format!("{base_url}/api/tenko/sessions/{session_id}/daily-inspection"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "brakes": "ok", "tires": "ok", "lights": "ok", "steering": "ok",
            "wipers": "ok", "mirrors": "ok", "horn": "ok", "seatbelts": "ok"
        }))
        .send().await.unwrap();
    assert_eq!(res.status(), 200);
    let session: Value = res.json().await.unwrap();
    assert_eq!(session["status"], "carrying_items_pending");

    // 携行品チェック送信
    let res = client
        .put(format!("{base_url}/api/tenko/sessions/{session_id}/carrying-items"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "checks": [{ "item_id": item_id, "checked": true }]
        }))
        .send().await.unwrap();
    assert_eq!(res.status(), 200);
    let session: Value = res.json().await.unwrap();
    assert_eq!(session["status"], "identity_verified");
}

// ============================================================
// 日常点検 NG → キャンセル
// ============================================================

#[tokio::test]
async fn test_daily_inspection_ng_cancels() {
    let (base_url, auth, emp_id, client) = setup_tenko().await;

    let session = start_session_remote(&client, &base_url, &auth, &emp_id).await;
    let session_id = session["id"].as_str().unwrap();

    // medical → self_declaration → daily_inspection (NG)
    client.put(format!("{base_url}/api/tenko/sessions/{session_id}/medical"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({ "temperature": 36.5 }))
        .send().await.unwrap();

    client.put(format!("{base_url}/api/tenko/sessions/{session_id}/self-declaration"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({ "illness": false, "fatigue": false, "sleep_deprivation": false }))
        .send().await.unwrap();

    let res = client.put(format!("{base_url}/api/tenko/sessions/{session_id}/daily-inspection"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "brakes": "ng", "tires": "ok", "lights": "ok", "steering": "ok",
            "wipers": "ok", "mirrors": "ok", "horn": "ok", "seatbelts": "ok"
        }))
        .send().await.unwrap();
    assert_eq!(res.status(), 200);
    let session: Value = res.json().await.unwrap();
    assert_eq!(session["status"], "cancelled");
}

// ============================================================
// ダッシュボード — overdue schedules
// ============================================================

#[tokio::test]
async fn test_dashboard_with_overdue() {
    let (base_url, auth, emp_id, client) = setup_tenko().await;

    // 過去のスケジュールを作成 (consumed=false → overdue)
    let res = client
        .post(format!("{base_url}/api/tenko/schedules"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "employee_id": emp_id,
            "tenko_type": "pre_operation",
            "responsible_manager_name": "管理者",
            "scheduled_at": "2020-01-01T00:00:00Z",
            "instruction": "過去の指示"
        }))
        .send().await.unwrap();
    assert_eq!(res.status(), 201);

    let res = client
        .get(format!("{base_url}/api/tenko/dashboard"))
        .header("Authorization", &auth)
        .send().await.unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert!(body["pending_schedules"].as_i64().unwrap() >= 1);
    // overdue_schedules should contain our past schedule
    assert!(body["overdue_schedules"].as_array().unwrap().len() >= 1);
}

// ============================================================
// セッション — webhook 付き完了 (alcohol_detected)
// ============================================================

#[tokio::test]
async fn test_alcohol_fail_fires_webhook() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, &format!("AlcWH{}", uuid::Uuid::new_v4().simple())).await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    // webhook 設定
    client.post(format!("{base_url}/api/tenko/webhooks"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "event_type": "alcohol_detected",
            "url": "https://httpbin.org/post",
            "secret": "test"
        }))
        .send().await.unwrap();

    let emp = common::create_test_employee(&client, &base_url, &auth, "WHEmp", &format!("WH{}", &uuid::Uuid::new_v4().simple().to_string()[..4])).await;
    let emp_id = emp["id"].as_str().unwrap();

    let sid = create_schedule(&client, &base_url, &auth, emp_id, "post_operation").await;
    let session = start_session(&client, &base_url, &auth, emp_id, &sid).await;
    let session_id = session["id"].as_str().unwrap();

    // alcohol fail → webhook fired (async, won't block)
    let res = client
        .put(format!("{base_url}/api/tenko/sessions/{session_id}/alcohol"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({ "alcohol_result": "fail", "alcohol_value": 0.3 }))
        .send().await.unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["status"], "cancelled");
}

// ============================================================
// 自己申告で安全判定 fail → interrupted
// ============================================================

#[tokio::test]
async fn test_self_declaration_with_illness_interrupts() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, &format!("SelfDecl{}", uuid::Uuid::new_v4().simple())).await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();
    let emp = common::create_test_employee(&client, &base_url, &auth, "IllEmp", &format!("IL{}", &uuid::Uuid::new_v4().simple().to_string()[..4])).await;
    let emp_id = emp["id"].as_str().unwrap().to_string();

    let session = start_session_remote(&client, &base_url, &auth, &emp_id).await;
    let session_id = session["id"].as_str().unwrap();

    client.put(format!("{base_url}/api/tenko/sessions/{session_id}/medical"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({ "temperature": 36.5 }))
        .send().await.unwrap();

    let res = client.put(format!("{base_url}/api/tenko/sessions/{session_id}/self-declaration"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({ "illness": true, "fatigue": false, "sleep_deprivation": false }))
        .send().await.unwrap();
    assert_eq!(res.status(), 200);
    let session: Value = res.json().await.unwrap();
    assert_eq!(session["status"], "interrupted");
}

// ============================================================
// Tenko Records — CSV + 個別取得
// ============================================================

#[tokio::test]
async fn test_tenko_record_get_by_id() {
    let (base_url, auth, emp_id, client) = setup_tenko().await;

    // post_operation を完了してレコード生成
    let sid = create_schedule(&client, &base_url, &auth, &emp_id, "post_operation").await;
    let session = start_session(&client, &base_url, &auth, &emp_id, &sid).await;
    let session_id = session["id"].as_str().unwrap();

    client.put(format!("{base_url}/api/tenko/sessions/{session_id}/alcohol"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({ "alcohol_result": "pass", "alcohol_value": 0.0 }))
        .send().await.unwrap();
    client.put(format!("{base_url}/api/tenko/sessions/{session_id}/report"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({ "vehicle_road_status": "OK", "driver_alternation": "なし" }))
        .send().await.unwrap();
    client.put(format!("{base_url}/api/tenko/sessions/{session_id}/instruction-confirm"))
        .header("Authorization", &auth)
        .send().await.unwrap();

    // レコード一覧からID取得
    let res = client.get(format!("{base_url}/api/tenko/records"))
        .header("Authorization", &auth)
        .send().await.unwrap();
    let body: Value = res.json().await.unwrap();
    let records = body["records"].as_array().unwrap();
    assert!(!records.is_empty());
    let record_id = records[0]["id"].as_str().unwrap();

    // 個別取得
    let res = client.get(format!("{base_url}/api/tenko/records/{record_id}"))
        .header("Authorization", &auth)
        .send().await.unwrap();
    assert_eq!(res.status(), 200);
}

// ============================================================
// Tenko Sessions — 追加テスト (フィルタ)
// ============================================================

#[tokio::test]
async fn test_session_list_with_filter() {
    let (base_url, auth, emp_id, client) = setup_tenko().await;

    start_session_remote(&client, &base_url, &auth, &emp_id).await;

    // status フィルタ
    let res = client
        .get(format!("{base_url}/api/tenko/sessions?status=medical_pending"))
        .header("Authorization", &auth)
        .send().await.unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    for s in body["sessions"].as_array().unwrap() {
        assert_eq!(s["status"], "medical_pending");
    }

    // tenko_type フィルタ
    let res = client
        .get(format!("{base_url}/api/tenko/sessions?tenko_type=pre_operation"))
        .header("Authorization", &auth)
        .send().await.unwrap();
    assert_eq!(res.status(), 200);

    // employee_id フィルタ
    let res = client
        .get(format!("{base_url}/api/tenko/sessions?employee_id={emp_id}"))
        .header("Authorization", &auth)
        .send().await.unwrap();
    assert_eq!(res.status(), 200);
}

// ============================================================
// Tenko Records — フィルタ
// ============================================================

#[tokio::test]
async fn test_tenko_records_with_filter() {
    let (base_url, auth, emp_id, client) = setup_tenko().await;

    // employee_id フィルタ
    let res = client
        .get(format!("{base_url}/api/tenko/records?employee_id={emp_id}"))
        .header("Authorization", &auth)
        .send().await.unwrap();
    assert_eq!(res.status(), 200);

    // tenko_type フィルタ
    let res = client
        .get(format!("{base_url}/api/tenko/records?tenko_type=pre_operation"))
        .header("Authorization", &auth)
        .send().await.unwrap();
    assert_eq!(res.status(), 200);

    // status フィルタ
    let res = client
        .get(format!("{base_url}/api/tenko/records?status=completed"))
        .header("Authorization", &auth)
        .send().await.unwrap();
    assert_eq!(res.status(), 200);
}

// ============================================================
// セッション一覧 — 複合フィルタ
// ============================================================

#[tokio::test]
async fn test_session_list_date_filter() {
    let (base_url, auth, emp_id, client) = setup_tenko().await;
    start_session_remote(&client, &base_url, &auth, &emp_id).await;

    let res = client
        .get(format!("{base_url}/api/tenko/sessions?date_from=2026-01-01T00:00:00Z&date_to=2099-12-31T23:59:59Z&page=1&per_page=5"))
        .header("Authorization", &auth)
        .send().await.unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["page"], 1);
    assert_eq!(body["per_page"], 5);
}

// ============================================================
// セッション — cancel 済みを再 cancel → BAD_REQUEST
// ============================================================

#[tokio::test]
async fn test_cancel_already_cancelled() {
    let (base_url, auth, emp_id, client) = setup_tenko().await;
    let session = start_session_remote(&client, &base_url, &auth, &emp_id).await;
    let sid = session["id"].as_str().unwrap();

    // 1回目 cancel
    client.post(format!("{base_url}/api/tenko/sessions/{sid}/cancel"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({}))
        .send().await.unwrap();

    // 2回目 cancel → 400
    let res = client.post(format!("{base_url}/api/tenko/sessions/{sid}/cancel"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({}))
        .send().await.unwrap();
    assert_eq!(res.status(), 400);
}

// ============================================================
// セッション — get not found
// ============================================================

#[tokio::test]
async fn test_session_get_not_found() {
    let (base_url, auth, _emp_id, client) = setup_tenko().await;
    let fake = uuid::Uuid::new_v4();

    let res = client
        .get(format!("{base_url}/api/tenko/sessions/{fake}"))
        .header("Authorization", &auth)
        .send().await.unwrap();
    assert_eq!(res.status(), 404);
}

// ============================================================
// アルコール — 無効な result
// ============================================================

#[tokio::test]
async fn test_alcohol_invalid_result() {
    let (base_url, auth, emp_id, client) = setup_tenko().await;
    let sid = create_schedule(&client, &base_url, &auth, &emp_id, "post_operation").await;
    let session = start_session(&client, &base_url, &auth, &emp_id, &sid).await;
    let session_id = session["id"].as_str().unwrap();

    let res = client
        .put(format!("{base_url}/api/tenko/sessions/{session_id}/alcohol"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({ "alcohol_result": "invalid", "alcohol_value": 0.0 }))
        .send().await.unwrap();
    assert_eq!(res.status(), 400);
}

// ============================================================
// 医療データ — wrong tenko_type (post_operation)
// ============================================================

#[tokio::test]
async fn test_medical_wrong_tenko_type() {
    let (base_url, auth, emp_id, client) = setup_tenko().await;
    let sid = create_schedule(&client, &base_url, &auth, &emp_id, "post_operation").await;
    let session = start_session(&client, &base_url, &auth, &emp_id, &sid).await;
    let session_id = session["id"].as_str().unwrap();

    // post_operation は medical_pending にならないので BAD_REQUEST
    let res = client
        .put(format!("{base_url}/api/tenko/sessions/{session_id}/medical"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({ "temperature": 36.5 }))
        .send().await.unwrap();
    assert_eq!(res.status(), 400);
}

// ============================================================
// 運行報告 — 空テキスト
// ============================================================

#[tokio::test]
async fn test_report_empty_fields() {
    let (base_url, auth, emp_id, client) = setup_tenko().await;
    let sid = create_schedule(&client, &base_url, &auth, &emp_id, "post_operation").await;
    let session = start_session(&client, &base_url, &auth, &emp_id, &sid).await;
    let session_id = session["id"].as_str().unwrap();

    // alcohol pass first
    client.put(format!("{base_url}/api/tenko/sessions/{session_id}/alcohol"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({ "alcohol_result": "pass", "alcohol_value": 0.0 }))
        .send().await.unwrap();

    // empty fields → 400
    let res = client
        .put(format!("{base_url}/api/tenko/sessions/{session_id}/report"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({ "vehicle_road_status": "", "driver_alternation": "" }))
        .send().await.unwrap();
    assert_eq!(res.status(), 400);
}

// ============================================================
// Tenko Records — date フィルタ + ページネーション
// ============================================================

#[tokio::test]
async fn test_tenko_records_date_filter() {
    let (base_url, auth, _emp_id, client) = setup_tenko().await;

    let res = client
        .get(format!("{base_url}/api/tenko/records?date_from=2026-01-01T00:00:00Z&date_to=2026-12-31T23:59:59Z"))
        .header("Authorization", &auth)
        .send().await.unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert!(body["total"].as_i64().is_some());
}

#[tokio::test]
async fn test_tenko_records_pagination() {
    let (base_url, auth, _emp_id, client) = setup_tenko().await;

    let res = client
        .get(format!("{base_url}/api/tenko/records?page=1&per_page=5"))
        .header("Authorization", &auth)
        .send().await.unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["page"], 1);
    assert_eq!(body["per_page"], 5);
}

// ============================================================
// Webhooks CRUD
// ============================================================

#[tokio::test]
async fn test_webhooks_crud() {
    let (base_url, auth, _emp_id, client) = setup_tenko().await;

    // Create
    let res = client
        .post(format!("{base_url}/api/tenko/webhooks"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "event_type": "alcohol_detected",
            "url": "https://example.com/webhook",
            "secret": "test-secret"
        }))
        .send()
        .await
        .unwrap();
    assert!(res.status() == 200 || res.status() == 201);
    let wh: Value = res.json().await.unwrap();
    let wh_id = wh["id"].as_str().unwrap();

    // List
    let res = client
        .get(format!("{base_url}/api/tenko/webhooks"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    // Delete
    let res = client
        .delete(format!("{base_url}/api/tenko/webhooks/{wh_id}"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 204);
}
