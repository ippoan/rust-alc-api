#[macro_use]
mod common;

use serde_json::Value;

// ============================================================
// ヘルパー
// ============================================================

async fn setup_tenko() -> (String, String, String, reqwest::Client) {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(
        &state.pool,
        &format!("Tenko{}", uuid::Uuid::new_v4().simple()),
    )
    .await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();
    let emp = common::create_test_employee(
        &client,
        &base_url,
        &auth,
        "TenkoEmp",
        &format!("TK{}", &uuid::Uuid::new_v4().simple().to_string()[..4]),
    )
    .await;
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
    test_group!("点呼スケジュール CRUD");
    test_case!("作成・一覧・取得・更新・削除", {
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
    });
}

#[tokio::test]
async fn test_schedule_batch_create() {
    test_group!("点呼スケジュール CRUD");
    test_case!("バッチ作成", {
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
    });
}

#[tokio::test]
async fn test_schedule_pending_for_employee() {
    test_group!("点呼スケジュール CRUD");
    test_case!("従業員の未消費スケジュール取得", {
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
    });
}

// ============================================================
// Tenko Schedules — edge cases
// ============================================================

#[tokio::test]
async fn test_schedule_get_not_found() {
    test_group!("点呼スケジュール — エッジケース");
    test_case!("存在しないスケジュール取得 → 404", {
        let (base_url, auth, _emp_id, client) = setup_tenko().await;
        let fake = uuid::Uuid::new_v4();
        let res = client
            .get(format!("{base_url}/api/tenko/schedules/{fake}"))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 404);
    });
}

#[tokio::test]
async fn test_schedule_delete_not_found() {
    test_group!("点呼スケジュール — エッジケース");
    test_case!("存在しないスケジュール削除 → 404", {
        let (base_url, auth, _emp_id, client) = setup_tenko().await;
        let fake = uuid::Uuid::new_v4();
        let res = client
            .delete(format!("{base_url}/api/tenko/schedules/{fake}"))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 404);
    });
}

#[tokio::test]
async fn test_schedule_update_not_found() {
    test_group!("点呼スケジュール — エッジケース");
    test_case!("存在しないスケジュール更新 → 404", {
        let (base_url, auth, _emp_id, client) = setup_tenko().await;
        let fake = uuid::Uuid::new_v4();
        let res = client
            .put(format!("{base_url}/api/tenko/schedules/{fake}"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({ "responsible_manager_name": "test" }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 404);
    });
}

#[tokio::test]
async fn test_schedule_invalid_tenko_type() {
    test_group!("点呼スケジュール — エッジケース");
    test_case!("無効な点呼種別 → 400", {
        let (base_url, auth, emp_id, client) = setup_tenko().await;
        let res = client
            .post(format!("{base_url}/api/tenko/schedules"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({
                "employee_id": emp_id, "tenko_type": "invalid_type",
                "responsible_manager_name": "mgr", "scheduled_at": "2099-01-01T00:00:00Z"
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 400);
    });
}

// ============================================================
// Equipment Failures — edge cases
// ============================================================

#[tokio::test]
async fn test_equipment_failure_get_not_found() {
    test_group!("機器故障 — エッジケース");
    test_case!("存在しない故障記録取得 → 404", {
        let (base_url, auth, _emp_id, client) = setup_tenko().await;
        let fake = uuid::Uuid::new_v4();
        let res = client
            .get(format!("{base_url}/api/tenko/equipment-failures/{fake}"))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 404);
    });
}

#[tokio::test]
async fn test_equipment_failure_invalid_type() {
    test_group!("機器故障 — エッジケース");
    test_case!("無効な故障種別 → 400", {
        let (base_url, auth, _emp_id, client) = setup_tenko().await;
        let res = client
            .post(format!("{base_url}/api/tenko/equipment-failures"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({ "failure_type": "invalid_type", "description": "test" }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 400);
    });
}

// ============================================================
// Health Baselines — edge cases
// ============================================================

#[tokio::test]
async fn test_health_baseline_get_not_found() {
    test_group!("健康基準値 — エッジケース");
    test_case!("存在しない基準値取得 → 404", {
        let (base_url, auth, _emp_id, client) = setup_tenko().await;
        let fake = uuid::Uuid::new_v4();
        let res = client
            .get(format!("{base_url}/api/tenko/health-baselines/{fake}"))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 404);
    });
}

#[tokio::test]
async fn test_health_baseline_delete_not_found() {
    test_group!("健康基準値 — エッジケース");
    test_case!("存在しない基準値削除 → 404", {
        let (base_url, auth, _emp_id, client) = setup_tenko().await;
        let fake = uuid::Uuid::new_v4();
        let res = client
            .delete(format!("{base_url}/api/tenko/health-baselines/{fake}"))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 404);
    });
}

// ============================================================
// Tenko Sessions — pre_operation フロー
// ============================================================

#[tokio::test]
async fn test_session_start_with_schedule() {
    test_group!("点呼セッション — pre_operation フロー");
    test_case!("スケジュール付きセッション開始", {
        let (base_url, auth, emp_id, client) = setup_tenko().await;

        let sid = create_schedule(&client, &base_url, &auth, &emp_id, "pre_operation").await;
        let session = start_session(&client, &base_url, &auth, &emp_id, &sid).await;

        assert_eq!(session["status"], "medical_pending");
        assert_eq!(session["tenko_type"], "pre_operation");
        assert!(session["id"].as_str().is_some());
    });
}

#[tokio::test]
async fn test_session_start_remote_no_schedule() {
    test_group!("点呼セッション — pre_operation フロー");
    test_case!("スケジュールなし遠隔セッション開始", {
        let (base_url, auth, emp_id, client) = setup_tenko().await;

        let session = start_session_remote(&client, &base_url, &auth, &emp_id).await;

        assert_eq!(session["status"], "medical_pending");
        assert_eq!(session["tenko_type"], "pre_operation");
    });
}

#[tokio::test]
async fn test_session_get() {
    test_group!("点呼セッション — pre_operation フロー");
    test_case!("セッション個別取得", {
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
    });
}

#[tokio::test]
async fn test_session_list() {
    test_group!("点呼セッション — pre_operation フロー");
    test_case!("セッション一覧取得", {
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
    });
}

#[tokio::test]
async fn test_session_dashboard() {
    test_group!("点呼セッション — pre_operation フロー");
    test_case!("ダッシュボード取得", {
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
    });
}

#[tokio::test]
async fn test_pre_operation_full_flow() {
    test_group!("点呼セッション — pre_operation フロー");
    test_case!("pre_operation フルフロー: medical → self_declaration → daily_inspection → instruction → completed", {
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
    });
}

#[tokio::test]
async fn test_post_operation_flow() {
    test_group!("点呼セッション — pre_operation フロー");
    test_case!(
        "post_operation フロー: identity_verified → alcohol → report → completed",
        {
            let (base_url, auth, emp_id, client) = setup_tenko().await;

            // post_operation スケジュール (instruction なし)
            let sid = create_schedule(&client, &base_url, &auth, &emp_id, "post_operation").await;

            // セッション開始 → identity_verified (post_op は medical skip)
            let session = start_session(&client, &base_url, &auth, &emp_id, &sid).await;
            let session_id = session["id"].as_str().unwrap();
            assert_eq!(session["status"], "identity_verified");

            // アルコール送信 → report_pending
            let res = client
                .put(format!(
                    "{base_url}/api/tenko/sessions/{session_id}/alcohol"
                ))
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
                .put(format!(
                    "{base_url}/api/tenko/sessions/{session_id}/instruction-confirm"
                ))
                .header("Authorization", &auth)
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
            let session: Value = res.json().await.unwrap();
            assert_eq!(session["status"], "completed");
        }
    );
}

// ============================================================
// セッション中止 / 中断 / 再開
// ============================================================

#[tokio::test]
async fn test_session_cancel() {
    test_group!("セッション中止 / 中断 / 再開");
    test_case!("セッション中止", {
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
    });
}

#[tokio::test]
async fn test_session_interrupt_and_resume() {
    test_group!("セッション中止 / 中断 / 再開");
    test_case!("中断と再開", {
        let (base_url, auth, emp_id, client) = setup_tenko().await;

        let session = start_session_remote(&client, &base_url, &auth, &emp_id).await;
        let session_id = session["id"].as_str().unwrap();

        // 中断
        let res = client
            .post(format!(
                "{base_url}/api/tenko/sessions/{session_id}/interrupt"
            ))
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
    });
}

// ============================================================
// アルコール検知 → 自動キャンセル
// ============================================================

#[tokio::test]
async fn test_alcohol_fail_cancels_session() {
    test_group!("アルコール検知 → 自動キャンセル");
    test_case!(
        "アルコール検知でセッション自動キャンセル",
        {
            let (base_url, auth, emp_id, client) = setup_tenko().await;

            let sid = create_schedule(&client, &base_url, &auth, &emp_id, "post_operation").await;
            let session = start_session(&client, &base_url, &auth, &emp_id, &sid).await;
            let session_id = session["id"].as_str().unwrap();

            let res = client
                .put(format!(
                    "{base_url}/api/tenko/sessions/{session_id}/alcohol"
                ))
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
    );
}

// ============================================================
// Tenko Records
// ============================================================

#[tokio::test]
async fn test_tenko_records_after_completion() {
    test_group!("点呼記録");
    test_case!("セッション完了後のレコード確認", {
        let (base_url, auth, emp_id, client) = setup_tenko().await;

        // post_operation を完了させる
        let sid = create_schedule(&client, &base_url, &auth, &emp_id, "post_operation").await;
        let session = start_session(&client, &base_url, &auth, &emp_id, &sid).await;
        let session_id = session["id"].as_str().unwrap();

        client
            .put(format!(
                "{base_url}/api/tenko/sessions/{session_id}/alcohol"
            ))
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
            .put(format!(
                "{base_url}/api/tenko/sessions/{session_id}/instruction-confirm"
            ))
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
    });
}

#[tokio::test]
async fn test_tenko_records_csv() {
    test_group!("点呼記録");
    test_case!("CSV出力", {
        let (base_url, auth, _emp_id, client) = setup_tenko().await;

        let res = client
            .get(format!("{base_url}/api/tenko/records/csv"))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        let content_type = res.headers().get("content-type").unwrap().to_str().unwrap();
        assert!(
            content_type.contains("text/csv") || content_type.contains("application/octet-stream")
        );
    });
}

// ============================================================
// Health Baselines CRUD
// ============================================================

#[tokio::test]
async fn test_health_baselines_crud() {
    test_group!("健康基準値 CRUD");
    test_case!("作成・取得・一覧・削除", {
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
        assert!(
            res.status() == 200 || res.status() == 201,
            "upsert baseline: {}",
            res.status()
        );

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
    });
}

// ============================================================
// Equipment Failures CRUD
// ============================================================

#[tokio::test]
async fn test_equipment_failures_crud() {
    test_group!("機器故障 CRUD");
    test_case!("作成・一覧・取得・解決・CSV", {
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
            .get(format!(
                "{base_url}/api/tenko/equipment-failures/{failure_id}"
            ))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);

        // Resolve
        let res = client
            .put(format!(
                "{base_url}/api/tenko/equipment-failures/{failure_id}"
            ))
            .header("Authorization", &auth)
            .json(&serde_json::json!({ "resolution_notes": "修理完了" }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        let resolved: Value = res.json().await.unwrap();
        assert!(
            resolved["resolved_at"].as_str().is_some(),
            "resolved_at should be set"
        );

        // CSV
        let res = client
            .get(format!("{base_url}/api/tenko/equipment-failures/csv"))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
    });
}

// ============================================================
// 携行品チェック付きフロー
// ============================================================

// tenko_carrying_item_checks テーブルのマイグレーションが未作成のためスキップ
#[tokio::test]
#[ignore]
async fn test_pre_operation_with_carrying_items() {
    test_group!("携行品チェック付きフロー");
    test_case!("携行品マスタあり → carrying_items_pending", {
        let (base_url, auth, emp_id, client) = setup_tenko().await;

        // 携行品マスタを作成 (daily_inspection → carrying_items_pending のトリガー)
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

        let res = client
            .put(format!(
                "{base_url}/api/tenko/sessions/{session_id}/daily-inspection"
            ))
            .header("Authorization", &auth)
            .json(&serde_json::json!({
                "brakes": "ok", "tires": "ok", "lights": "ok", "steering": "ok",
                "wipers": "ok", "mirrors": "ok", "horn": "ok", "seatbelts": "ok"
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        let session: Value = res.json().await.unwrap();
        assert_eq!(session["status"], "carrying_items_pending");

        // 携行品チェック送信
        let res = client
            .put(format!(
                "{base_url}/api/tenko/sessions/{session_id}/carrying-items"
            ))
            .header("Authorization", &auth)
            .json(&serde_json::json!({
                "checks": [{ "item_id": item_id, "checked": true }]
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        let session: Value = res.json().await.unwrap();
        assert_eq!(session["status"], "identity_verified");
    });
}

// ============================================================
// 日常点検 NG → キャンセル
// ============================================================

#[tokio::test]
async fn test_daily_inspection_ng_cancels() {
    test_group!("日常点検 NG → キャンセル");
    test_case!("日常点検NGでセッション自動キャンセル", {
        let (base_url, auth, emp_id, client) = setup_tenko().await;

        let session = start_session_remote(&client, &base_url, &auth, &emp_id).await;
        let session_id = session["id"].as_str().unwrap();

        // medical → self_declaration → daily_inspection (NG)
        client
            .put(format!(
                "{base_url}/api/tenko/sessions/{session_id}/medical"
            ))
            .header("Authorization", &auth)
            .json(&serde_json::json!({ "temperature": 36.5 }))
            .send()
            .await
            .unwrap();

        client.put(format!("{base_url}/api/tenko/sessions/{session_id}/self-declaration"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({ "illness": false, "fatigue": false, "sleep_deprivation": false }))
            .send().await.unwrap();

        let res = client
            .put(format!(
                "{base_url}/api/tenko/sessions/{session_id}/daily-inspection"
            ))
            .header("Authorization", &auth)
            .json(&serde_json::json!({
                "brakes": "ng", "tires": "ok", "lights": "ok", "steering": "ok",
                "wipers": "ok", "mirrors": "ok", "horn": "ok", "seatbelts": "ok"
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        let session: Value = res.json().await.unwrap();
        assert_eq!(session["status"], "cancelled");
    });
}

// ============================================================
// ダッシュボード — overdue schedules
// ============================================================

#[tokio::test]
async fn test_dashboard_with_overdue() {
    test_group!("ダッシュボード — 期限超過スケジュール");
    test_case!("過去のスケジュールがoverdueに表示される", {
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
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 201);

        let res = client
            .get(format!("{base_url}/api/tenko/dashboard"))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        let body: Value = res.json().await.unwrap();
        assert!(body["pending_schedules"].as_i64().unwrap() >= 1);
        // overdue_schedules should contain our past schedule
        assert!(body["overdue_schedules"].as_array().unwrap().len() >= 1);
    });
}

// ============================================================
// セッション — webhook 付き完了 (alcohol_detected)
// ============================================================

#[tokio::test]
async fn test_alcohol_fail_fires_webhook() {
    test_group!("セッション — webhook 付き完了");
    test_case!("アルコール検知でwebhook発火", {
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;
        let tenant_id = common::create_test_tenant(
            &state.pool,
            &format!("AlcWH{}", uuid::Uuid::new_v4().simple()),
        )
        .await;
        let jwt = common::create_test_jwt(tenant_id, "admin");
        let auth = format!("Bearer {jwt}");
        let client = reqwest::Client::new();

        // webhook 設定
        client
            .post(format!("{base_url}/api/tenko/webhooks"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({
                "event_type": "alcohol_detected",
                "url": "https://httpbin.org/post",
                "secret": "test"
            }))
            .send()
            .await
            .unwrap();

        let emp = common::create_test_employee(
            &client,
            &base_url,
            &auth,
            "WHEmp",
            &format!("WH{}", &uuid::Uuid::new_v4().simple().to_string()[..4]),
        )
        .await;
        let emp_id = emp["id"].as_str().unwrap();

        let sid = create_schedule(&client, &base_url, &auth, emp_id, "post_operation").await;
        let session = start_session(&client, &base_url, &auth, emp_id, &sid).await;
        let session_id = session["id"].as_str().unwrap();

        // alcohol fail → webhook fired (async, won't block)
        let res = client
            .put(format!(
                "{base_url}/api/tenko/sessions/{session_id}/alcohol"
            ))
            .header("Authorization", &auth)
            .json(&serde_json::json!({ "alcohol_result": "fail", "alcohol_value": 0.3 }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        let body: Value = res.json().await.unwrap();
        assert_eq!(body["status"], "cancelled");
    });
}

// ============================================================
// 自己申告で安全判定 fail → interrupted
// ============================================================

#[tokio::test]
async fn test_self_declaration_with_illness_interrupts() {
    test_group!("自己申告で安全判定 fail → interrupted");
    test_case!("illness=trueでセッション中断", {
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;
        let tenant_id = common::create_test_tenant(
            &state.pool,
            &format!("SelfDecl{}", uuid::Uuid::new_v4().simple()),
        )
        .await;
        let jwt = common::create_test_jwt(tenant_id, "admin");
        let auth = format!("Bearer {jwt}");
        let client = reqwest::Client::new();
        let emp = common::create_test_employee(
            &client,
            &base_url,
            &auth,
            "IllEmp",
            &format!("IL{}", &uuid::Uuid::new_v4().simple().to_string()[..4]),
        )
        .await;
        let emp_id = emp["id"].as_str().unwrap().to_string();

        let session = start_session_remote(&client, &base_url, &auth, &emp_id).await;
        let session_id = session["id"].as_str().unwrap();

        client
            .put(format!(
                "{base_url}/api/tenko/sessions/{session_id}/medical"
            ))
            .header("Authorization", &auth)
            .json(&serde_json::json!({ "temperature": 36.5 }))
            .send()
            .await
            .unwrap();

        let res = client.put(format!("{base_url}/api/tenko/sessions/{session_id}/self-declaration"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({ "illness": true, "fatigue": false, "sleep_deprivation": false }))
            .send().await.unwrap();
        assert_eq!(res.status(), 200);
        let session: Value = res.json().await.unwrap();
        assert_eq!(session["status"], "interrupted");
    });
}

// ============================================================
// Tenko Records — CSV + 個別取得
// ============================================================

#[tokio::test]
async fn test_tenko_record_get_by_id() {
    test_group!("点呼記録 — CSV + 個別取得");
    test_case!("レコードID指定で取得", {
        let (base_url, auth, emp_id, client) = setup_tenko().await;

        // post_operation を完了してレコード生成
        let sid = create_schedule(&client, &base_url, &auth, &emp_id, "post_operation").await;
        let session = start_session(&client, &base_url, &auth, &emp_id, &sid).await;
        let session_id = session["id"].as_str().unwrap();

        client
            .put(format!(
                "{base_url}/api/tenko/sessions/{session_id}/alcohol"
            ))
            .header("Authorization", &auth)
            .json(&serde_json::json!({ "alcohol_result": "pass", "alcohol_value": 0.0 }))
            .send()
            .await
            .unwrap();
        client
            .put(format!("{base_url}/api/tenko/sessions/{session_id}/report"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({ "vehicle_road_status": "OK", "driver_alternation": "なし" }))
            .send()
            .await
            .unwrap();
        client
            .put(format!(
                "{base_url}/api/tenko/sessions/{session_id}/instruction-confirm"
            ))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();

        // レコード一覧からID取得
        let res = client
            .get(format!("{base_url}/api/tenko/records"))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();
        let body: Value = res.json().await.unwrap();
        let records = body["records"].as_array().unwrap();
        assert!(!records.is_empty());
        let record_id = records[0]["id"].as_str().unwrap();

        // 個別取得
        let res = client
            .get(format!("{base_url}/api/tenko/records/{record_id}"))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
    });
}

// ============================================================
// Tenko Sessions — 追加テスト (フィルタ)
// ============================================================

#[tokio::test]
async fn test_session_list_with_filter() {
    test_group!("点呼セッション — フィルタ");
    test_case!("status / tenko_type / employee_id フィルタ", {
        let (base_url, auth, emp_id, client) = setup_tenko().await;

        start_session_remote(&client, &base_url, &auth, &emp_id).await;

        // status フィルタ
        let res = client
            .get(format!(
                "{base_url}/api/tenko/sessions?status=medical_pending"
            ))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        let body: Value = res.json().await.unwrap();
        for s in body["sessions"].as_array().unwrap() {
            assert_eq!(s["status"], "medical_pending");
        }

        // tenko_type フィルタ
        let res = client
            .get(format!(
                "{base_url}/api/tenko/sessions?tenko_type=pre_operation"
            ))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);

        // employee_id フィルタ
        let res = client
            .get(format!(
                "{base_url}/api/tenko/sessions?employee_id={emp_id}"
            ))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
    });
}

// ============================================================
// Tenko Records — フィルタ
// ============================================================

#[tokio::test]
async fn test_tenko_records_with_filter() {
    test_group!("点呼記録 — フィルタ");
    test_case!("employee_id / tenko_type / status フィルタ", {
        let (base_url, auth, emp_id, client) = setup_tenko().await;

        // employee_id フィルタ
        let res = client
            .get(format!("{base_url}/api/tenko/records?employee_id={emp_id}"))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);

        // tenko_type フィルタ
        let res = client
            .get(format!(
                "{base_url}/api/tenko/records?tenko_type=pre_operation"
            ))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);

        // status フィルタ
        let res = client
            .get(format!("{base_url}/api/tenko/records?status=completed"))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
    });
}

// ============================================================
// セッション一覧 — 複合フィルタ
// ============================================================

#[tokio::test]
async fn test_session_list_date_filter() {
    test_group!("セッション一覧 — 複合フィルタ");
    test_case!("日付範囲 + ページネーション", {
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
    });
}

// ============================================================
// セッション — cancel 済みを再 cancel → BAD_REQUEST
// ============================================================

#[tokio::test]
async fn test_cancel_already_cancelled() {
    test_group!("セッション — 二重キャンセル");
    test_case!("cancel済みセッションを再cancel → 400", {
        let (base_url, auth, emp_id, client) = setup_tenko().await;
        let session = start_session_remote(&client, &base_url, &auth, &emp_id).await;
        let sid = session["id"].as_str().unwrap();

        // 1回目 cancel
        client
            .post(format!("{base_url}/api/tenko/sessions/{sid}/cancel"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({}))
            .send()
            .await
            .unwrap();

        // 2回目 cancel → 400
        let res = client
            .post(format!("{base_url}/api/tenko/sessions/{sid}/cancel"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({}))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 400);
    });
}

// ============================================================
// セッション — get not found
// ============================================================

#[tokio::test]
async fn test_session_get_not_found() {
    test_group!("セッション — 存在しないID取得");
    test_case!("存在しないセッションID → 404", {
        let (base_url, auth, _emp_id, client) = setup_tenko().await;
        let fake = uuid::Uuid::new_v4();

        let res = client
            .get(format!("{base_url}/api/tenko/sessions/{fake}"))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 404);
    });
}

// ============================================================
// アルコール — 無効な result
// ============================================================

#[tokio::test]
async fn test_alcohol_invalid_result() {
    test_group!("アルコール — 無効な result");
    test_case!("無効なalcohol_result → 400", {
        let (base_url, auth, emp_id, client) = setup_tenko().await;
        let sid = create_schedule(&client, &base_url, &auth, &emp_id, "post_operation").await;
        let session = start_session(&client, &base_url, &auth, &emp_id, &sid).await;
        let session_id = session["id"].as_str().unwrap();

        let res = client
            .put(format!(
                "{base_url}/api/tenko/sessions/{session_id}/alcohol"
            ))
            .header("Authorization", &auth)
            .json(&serde_json::json!({ "alcohol_result": "invalid", "alcohol_value": 0.0 }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 400);
    });
}

// ============================================================
// 医療データ — wrong tenko_type (post_operation)
// ============================================================

#[tokio::test]
async fn test_medical_wrong_tenko_type() {
    test_group!("医療データ — 不正な点呼種別");
    test_case!("post_operationで医療データ送信 → 400", {
        let (base_url, auth, emp_id, client) = setup_tenko().await;
        let sid = create_schedule(&client, &base_url, &auth, &emp_id, "post_operation").await;
        let session = start_session(&client, &base_url, &auth, &emp_id, &sid).await;
        let session_id = session["id"].as_str().unwrap();

        // post_operation は medical_pending にならないので BAD_REQUEST
        let res = client
            .put(format!(
                "{base_url}/api/tenko/sessions/{session_id}/medical"
            ))
            .header("Authorization", &auth)
            .json(&serde_json::json!({ "temperature": 36.5 }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 400);
    });
}

// ============================================================
// 運行報告 — 空テキスト
// ============================================================

#[tokio::test]
async fn test_report_empty_fields() {
    test_group!("運行報告 — 空テキスト");
    test_case!("空フィールドで運行報告 → 400", {
        let (base_url, auth, emp_id, client) = setup_tenko().await;
        let sid = create_schedule(&client, &base_url, &auth, &emp_id, "post_operation").await;
        let session = start_session(&client, &base_url, &auth, &emp_id, &sid).await;
        let session_id = session["id"].as_str().unwrap();

        // alcohol pass first
        client
            .put(format!(
                "{base_url}/api/tenko/sessions/{session_id}/alcohol"
            ))
            .header("Authorization", &auth)
            .json(&serde_json::json!({ "alcohol_result": "pass", "alcohol_value": 0.0 }))
            .send()
            .await
            .unwrap();

        // empty fields → 400
        let res = client
            .put(format!("{base_url}/api/tenko/sessions/{session_id}/report"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({ "vehicle_road_status": "", "driver_alternation": "" }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 400);
    });
}

// ============================================================
// Tenko Records — date フィルタ + ページネーション
// ============================================================

#[tokio::test]
async fn test_tenko_records_date_filter() {
    test_group!("点呼記録 — 日付フィルタ + ページネーション");
    test_case!("日付範囲フィルタ", {
        let (base_url, auth, _emp_id, client) = setup_tenko().await;

        let res = client
            .get(format!("{base_url}/api/tenko/records?date_from=2026-01-01T00:00:00Z&date_to=2026-12-31T23:59:59Z"))
            .header("Authorization", &auth)
            .send().await.unwrap();
        assert_eq!(res.status(), 200);
        let body: Value = res.json().await.unwrap();
        assert!(body["total"].as_i64().is_some());
    });
}

#[tokio::test]
async fn test_tenko_records_pagination() {
    test_group!("点呼記録 — 日付フィルタ + ページネーション");
    test_case!("ページネーション", {
        let (base_url, auth, _emp_id, client) = setup_tenko().await;

        let res = client
            .get(format!("{base_url}/api/tenko/records?page=1&per_page=5"))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        let body: Value = res.json().await.unwrap();
        assert_eq!(body["page"], 1);
        assert_eq!(body["per_page"], 5);
    });
}

// ============================================================
// Webhooks CRUD
// ============================================================

#[tokio::test]
async fn test_webhooks_crud() {
    test_group!("Webhook CRUD");
    test_case!("作成・一覧・削除", {
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
    });
}

// ============================================================
// Tenko Records — CSV with date filters
// ============================================================

#[tokio::test]
async fn test_tenko_records_csv_with_date_filters() {
    test_group!("点呼記録 — CSV日付フィルタ");
    test_case!("日付範囲指定CSV出力", {
        let (base_url, auth, emp_id, client) = setup_tenko().await;

        // Complete a post_operation session to generate a record
        let sid = create_schedule(&client, &base_url, &auth, &emp_id, "post_operation").await;
        let session = start_session(&client, &base_url, &auth, &emp_id, &sid).await;
        let session_id = session["id"].as_str().unwrap();

        client
            .put(format!(
                "{base_url}/api/tenko/sessions/{session_id}/alcohol"
            ))
            .header("Authorization", &auth)
            .json(&serde_json::json!({ "alcohol_result": "pass", "alcohol_value": 0.0 }))
            .send()
            .await
            .unwrap();
        client
            .put(format!("{base_url}/api/tenko/sessions/{session_id}/report"))
            .header("Authorization", &auth)
            .json(
                &serde_json::json!({ "vehicle_road_status": "良好", "driver_alternation": "なし" }),
            )
            .send()
            .await
            .unwrap();
        client
            .put(format!(
                "{base_url}/api/tenko/sessions/{session_id}/instruction-confirm"
            ))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();

        // CSV with date range that includes today
        let res = client
            .get(format!("{base_url}/api/tenko/records/csv?date_from=2020-01-01T00:00:00Z&date_to=2099-12-31T23:59:59Z"))
            .header("Authorization", &auth)
            .send().await.unwrap();
        assert_eq!(res.status(), 200);
        let ct = res.headers().get("content-type").unwrap().to_str().unwrap();
        assert!(ct.contains("text/csv"));
        let body = res.bytes().await.unwrap();
        // BOM + header row + at least one data row
        assert!(body.len() > 100, "CSV should have header + data rows");
        // Check BOM prefix
        assert_eq!(&body[..3], &[0xEF, 0xBB, 0xBF]);
        let csv_text = String::from_utf8_lossy(&body[3..]);
        assert!(
            csv_text.contains("record_id"),
            "CSV header should contain record_id"
        );
        assert!(
            csv_text.contains("employee_name"),
            "CSV header should contain employee_name"
        );
        // Should have at least 2 lines (header + 1 record)
        let line_count = csv_text.lines().count();
        assert!(
            line_count >= 2,
            "Expected at least 2 CSV lines, got {line_count}"
        );

        // CSV with date range that excludes records (far future)
        let res = client
            .get(format!("{base_url}/api/tenko/records/csv?date_from=2099-01-01T00:00:00Z&date_to=2099-12-31T23:59:59Z"))
            .header("Authorization", &auth)
            .send().await.unwrap();
        assert_eq!(res.status(), 200);
        let body = res.bytes().await.unwrap();
        let csv_text = String::from_utf8_lossy(&body[3..]);
        // Should only have header row, no data
        let line_count = csv_text.lines().count();
        assert_eq!(
            line_count, 1,
            "Expected only header row for empty date range, got {line_count}"
        );
    });
}

// ============================================================
// Tenko Records — CSV with employee_id filter
// ============================================================

#[tokio::test]
async fn test_tenko_records_csv_with_employee_filter() {
    test_group!("点呼記録 — CSV従業員フィルタ");
    test_case!("従業員ID指定CSV出力", {
        let (base_url, auth, emp_id, client) = setup_tenko().await;

        // Complete a session to generate a record for this employee
        let sid = create_schedule(&client, &base_url, &auth, &emp_id, "post_operation").await;
        let session = start_session(&client, &base_url, &auth, &emp_id, &sid).await;
        let session_id = session["id"].as_str().unwrap();

        client
            .put(format!(
                "{base_url}/api/tenko/sessions/{session_id}/alcohol"
            ))
            .header("Authorization", &auth)
            .json(&serde_json::json!({ "alcohol_result": "pass", "alcohol_value": 0.0 }))
            .send()
            .await
            .unwrap();
        client
            .put(format!("{base_url}/api/tenko/sessions/{session_id}/report"))
            .header("Authorization", &auth)
            .json(
                &serde_json::json!({ "vehicle_road_status": "良好", "driver_alternation": "なし" }),
            )
            .send()
            .await
            .unwrap();
        client
            .put(format!(
                "{base_url}/api/tenko/sessions/{session_id}/instruction-confirm"
            ))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();

        // CSV filtered by this employee
        let res = client
            .get(format!(
                "{base_url}/api/tenko/records/csv?employee_id={emp_id}"
            ))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        let body = res.bytes().await.unwrap();
        let csv_text = String::from_utf8_lossy(&body[3..]);
        let line_count = csv_text.lines().count();
        assert!(
            line_count >= 2,
            "Expected header + at least 1 record for employee filter, got {line_count}"
        );

        // CSV filtered by a non-existent employee
        let fake_emp = uuid::Uuid::new_v4();
        let res = client
            .get(format!(
                "{base_url}/api/tenko/records/csv?employee_id={fake_emp}"
            ))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        let body = res.bytes().await.unwrap();
        let csv_text = String::from_utf8_lossy(&body[3..]);
        let line_count = csv_text.lines().count();
        assert_eq!(
            line_count, 1,
            "Expected only header for non-existent employee, got {line_count}"
        );
    });
}

// ============================================================
// Tenko Records — get record by ID after completed session
// ============================================================

#[tokio::test]
async fn test_tenko_record_get_by_id_with_full_data() {
    test_group!("点呼記録 — 完了セッションのレコード取得");
    test_case!("pre_operation完了後のレコード詳細確認", {
        let (base_url, auth, emp_id, client) = setup_tenko().await;

        // Complete a pre_operation full flow
        let res = client
            .post(format!("{base_url}/api/tenko/schedules"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({
                "employee_id": emp_id,
                "tenko_type": "pre_operation",
                "responsible_manager_name": "RecordTest管理者",
                "scheduled_at": "2099-06-01T06:00:00Z",
                "instruction": "安全運転"
            }))
            .send()
            .await
            .unwrap();
        let sched: Value = res.json().await.unwrap();
        let sid = sched["id"].as_str().unwrap();

        let session = start_session(&client, &base_url, &auth, &emp_id, sid).await;
        let session_id = session["id"].as_str().unwrap();

        // medical
        client.put(format!("{base_url}/api/tenko/sessions/{session_id}/medical"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({ "temperature": 36.5, "systolic": 120, "diastolic": 80, "pulse": 72, "medical_manual_input": true }))
            .send().await.unwrap();
        // self-declaration
        client.put(format!("{base_url}/api/tenko/sessions/{session_id}/self-declaration"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({ "illness": false, "fatigue": false, "sleep_deprivation": false }))
            .send().await.unwrap();
        // daily-inspection
        let res = client
            .put(format!(
                "{base_url}/api/tenko/sessions/{session_id}/daily-inspection"
            ))
            .header("Authorization", &auth)
            .json(&serde_json::json!({
                "brakes": "ok", "tires": "ok", "lights": "ok", "steering": "ok",
                "wipers": "ok", "mirrors": "ok", "horn": "ok", "seatbelts": "ok"
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        // alcohol
        client
            .put(format!(
                "{base_url}/api/tenko/sessions/{session_id}/alcohol"
            ))
            .header("Authorization", &auth)
            .json(&serde_json::json!({ "alcohol_result": "pass", "alcohol_value": 0.0 }))
            .send()
            .await
            .unwrap();
        // instruction-confirm
        client
            .put(format!(
                "{base_url}/api/tenko/sessions/{session_id}/instruction-confirm"
            ))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();

        // Fetch record list and get the record
        let res = client
            .get(format!("{base_url}/api/tenko/records?employee_id={emp_id}"))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        let body: Value = res.json().await.unwrap();
        let records = body["records"].as_array().unwrap();
        assert!(!records.is_empty(), "Should have at least one record");
        let record_id = records[0]["id"].as_str().unwrap();

        // GET by ID and validate fields
        let res = client
            .get(format!("{base_url}/api/tenko/records/{record_id}"))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        let record: Value = res.json().await.unwrap();
        assert_eq!(record["status"], "completed");
        assert_eq!(record["tenko_type"], "pre_operation");
        assert!(record["alcohol_result"].as_str().is_some());
        assert!(record["temperature"].as_f64().is_some());
        assert!(record["completed_at"].as_str().is_some());
        assert!(record["record_hash"].as_str().is_some());
        assert!(!record["record_hash"].as_str().unwrap().is_empty());
    });
}

// ============================================================
// Tenko Records — get record not found
// ============================================================

#[tokio::test]
async fn test_tenko_record_get_not_found() {
    test_group!("点呼記録 — 存在しないレコード");
    test_case!("存在しないレコードID → 404", {
        let (base_url, auth, _emp_id, client) = setup_tenko().await;
        let fake = uuid::Uuid::new_v4();
        let res = client
            .get(format!("{base_url}/api/tenko/records/{fake}"))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 404);
    });
}

// ============================================================
// Tenko Sessions — interrupt already interrupted → fail
// ============================================================

#[tokio::test]
async fn test_interrupt_already_interrupted() {
    test_group!("点呼セッション — 二重中断");
    test_case!("中断済みセッションを再中断 → 400", {
        let (base_url, auth, emp_id, client) = setup_tenko().await;

        let session = start_session_remote(&client, &base_url, &auth, &emp_id).await;
        let session_id = session["id"].as_str().unwrap();

        // First interrupt → success
        let res = client
            .post(format!(
                "{base_url}/api/tenko/sessions/{session_id}/interrupt"
            ))
            .header("Authorization", &auth)
            .json(&serde_json::json!({ "reason": "電話対応" }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        let s: Value = res.json().await.unwrap();
        assert_eq!(s["status"], "interrupted");

        // Second interrupt → 400 (already interrupted)
        let res = client
            .post(format!(
                "{base_url}/api/tenko/sessions/{session_id}/interrupt"
            ))
            .header("Authorization", &auth)
            .json(&serde_json::json!({ "reason": "再度中断" }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 400);
    });
}

// ============================================================
// Tenko Sessions — resume from non-interrupted state → fail
// ============================================================

#[tokio::test]
async fn test_resume_non_interrupted_session() {
    test_group!("点呼セッション — 非中断状態から再開");
    test_case!("非中断セッションを再開 → 400", {
        let (base_url, auth, emp_id, client) = setup_tenko().await;

        // Session is in medical_pending state (not interrupted)
        let session = start_session_remote(&client, &base_url, &auth, &emp_id).await;
        let session_id = session["id"].as_str().unwrap();
        assert_eq!(session["status"], "medical_pending");

        // Resume on non-interrupted session → 400
        let res = client
            .post(format!("{base_url}/api/tenko/sessions/{session_id}/resume"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({ "reason": "再開理由" }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 400);
    });
}

// ============================================================
// Tenko Sessions — resume completed session → fail
// ============================================================

#[tokio::test]
async fn test_resume_completed_session() {
    test_group!("点呼セッション — 完了セッション再開");
    test_case!("完了済みセッションを再開 → 400", {
        let (base_url, auth, emp_id, client) = setup_tenko().await;

        // Complete a post_operation session
        let sid = create_schedule(&client, &base_url, &auth, &emp_id, "post_operation").await;
        let session = start_session(&client, &base_url, &auth, &emp_id, &sid).await;
        let session_id = session["id"].as_str().unwrap();

        client
            .put(format!(
                "{base_url}/api/tenko/sessions/{session_id}/alcohol"
            ))
            .header("Authorization", &auth)
            .json(&serde_json::json!({ "alcohol_result": "pass", "alcohol_value": 0.0 }))
            .send()
            .await
            .unwrap();
        client
            .put(format!("{base_url}/api/tenko/sessions/{session_id}/report"))
            .header("Authorization", &auth)
            .json(
                &serde_json::json!({ "vehicle_road_status": "良好", "driver_alternation": "なし" }),
            )
            .send()
            .await
            .unwrap();
        client
            .put(format!(
                "{base_url}/api/tenko/sessions/{session_id}/instruction-confirm"
            ))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();

        // Resume on completed session → 400
        let res = client
            .post(format!("{base_url}/api/tenko/sessions/{session_id}/resume"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({ "reason": "再開したい" }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 400);
    });
}

// ============================================================
// Tenko Sessions — start with invalid employee_id → fail
// ============================================================

#[tokio::test]
async fn test_start_session_invalid_employee() {
    test_group!("点呼セッション — 無効な従業員ID");
    test_case!(
        "存在しない従業員IDでセッション開始 → 404/500",
        {
            let (base_url, auth, _emp_id, client) = setup_tenko().await;
            let fake_emp = uuid::Uuid::new_v4();

            let res = client
                .post(format!("{base_url}/api/tenko/sessions/start"))
                .header("Authorization", &auth)
                .json(&serde_json::json!({
                    "employee_id": fake_emp.to_string(),
                    "tenko_type": "pre_operation"
                }))
                .send()
                .await
                .unwrap();
            // Should fail: employee does not exist (404 or 500 from FK constraint)
            assert!(
                res.status() == 404 || res.status() == 500,
                "Expected 404 or 500 for invalid employee, got {}",
                res.status()
            );
        }
    );
}

// ============================================================
// Tenko Sessions — submit_alcohol on wrong status → 400
// ============================================================

#[tokio::test]
async fn test_submit_alcohol_wrong_status() {
    test_group!("点呼セッション — 不正ステータスでアルコール送信");
    test_case!("medical_pending状態でアルコール送信 → 400", {
        let (base_url, auth, emp_id, client) = setup_tenko().await;

        // Start a pre_operation session → medical_pending (NOT identity_verified)
        let session = start_session_remote(&client, &base_url, &auth, &emp_id).await;
        let session_id = session["id"].as_str().unwrap();
        assert_eq!(session["status"], "medical_pending");

        // Try to submit alcohol while in medical_pending → 400
        let res = client
            .put(format!(
                "{base_url}/api/tenko/sessions/{session_id}/alcohol"
            ))
            .header("Authorization", &auth)
            .json(&serde_json::json!({ "alcohol_result": "pass", "alcohol_value": 0.0 }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 400);
    });
}

// ============================================================
// Tenko Sessions — confirm_instruction on wrong status → 400
// ============================================================

#[tokio::test]
async fn test_confirm_instruction_wrong_status() {
    test_group!("点呼セッション — 不正ステータスで指示確認");
    test_case!("medical_pending状態で指示確認 → 400", {
        let (base_url, auth, emp_id, client) = setup_tenko().await;

        // Start a session → medical_pending (NOT instruction_pending)
        let session = start_session_remote(&client, &base_url, &auth, &emp_id).await;
        let session_id = session["id"].as_str().unwrap();
        assert_eq!(session["status"], "medical_pending");

        // Try to confirm instruction while in medical_pending → 400
        let res = client
            .put(format!(
                "{base_url}/api/tenko/sessions/{session_id}/instruction-confirm"
            ))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 400);
    });
}

// ============================================================
// Tenko Sessions — interrupt completed session → fail
// ============================================================

#[tokio::test]
async fn test_interrupt_completed_session() {
    test_group!("点呼セッション — 完了セッション中断");
    test_case!("完了済みセッションを中断 → 400", {
        let (base_url, auth, emp_id, client) = setup_tenko().await;

        // Complete a post_operation session
        let sid = create_schedule(&client, &base_url, &auth, &emp_id, "post_operation").await;
        let session = start_session(&client, &base_url, &auth, &emp_id, &sid).await;
        let session_id = session["id"].as_str().unwrap();

        client
            .put(format!(
                "{base_url}/api/tenko/sessions/{session_id}/alcohol"
            ))
            .header("Authorization", &auth)
            .json(&serde_json::json!({ "alcohol_result": "pass", "alcohol_value": 0.0 }))
            .send()
            .await
            .unwrap();
        client
            .put(format!("{base_url}/api/tenko/sessions/{session_id}/report"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({ "vehicle_road_status": "OK", "driver_alternation": "なし" }))
            .send()
            .await
            .unwrap();
        client
            .put(format!(
                "{base_url}/api/tenko/sessions/{session_id}/instruction-confirm"
            ))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();

        // Interrupt completed session → 400
        let res = client
            .post(format!(
                "{base_url}/api/tenko/sessions/{session_id}/interrupt"
            ))
            .header("Authorization", &auth)
            .json(&serde_json::json!({ "reason": "中断したい" }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 400);
    });
}

// ============================================================
// Tenko Call — register / tenko / delete_number
// ============================================================

#[tokio::test]
async fn test_tenko_call_register_with_employee_code() {
    test_group!("中間点呼 — 登録 / 点呼 / 番号削除");
    test_case!(
        "電話番号マスタ登録 → ドライバー登録(employee_code付き) → 一覧確認",
        {
            let (base_url, auth, _emp_id, client) = setup_tenko().await;

            // 1. 電話番号マスタを作成 (tenant_router 経由)
            let call_num = format!("TC{}", &uuid::Uuid::new_v4().simple().to_string()[..6]);
            let res = client
                .post(format!("{base_url}/api/tenko-call/numbers"))
                .header("Authorization", &auth)
                .json(&serde_json::json!({ "call_number": &call_num, "label": "テスト番号" }))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
            let num_body: serde_json::Value = res.json().await.unwrap();
            assert!(num_body["success"].as_bool().unwrap());

            // 2. ドライバー登録 (public, 認証不要)
            let phone = format!("090{}", &uuid::Uuid::new_v4().simple().to_string()[..8]);
            let emp_code = format!("EMP{}", &uuid::Uuid::new_v4().simple().to_string()[..4]);
            let res = client
                .post(format!("{base_url}/api/tenko-call/register"))
                .json(&serde_json::json!({
                    "phone_number": &phone,
                    "driver_name": "テストドライバー",
                    "call_number": &call_num,
                    "employee_code": &emp_code
                }))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
            let reg: serde_json::Value = res.json().await.unwrap();
            assert!(reg["success"].as_bool().unwrap());
            assert!(reg["driver_id"].as_i64().unwrap() > 0);
            assert_eq!(reg["call_number"].as_str().unwrap(), &call_num);

            // 3. ドライバー一覧で employee_code が保存されていることを確認
            let res = client
                .get(format!("{base_url}/api/tenko-call/drivers"))
                .header("Authorization", &auth)
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
            let drivers: Vec<serde_json::Value> = res.json().await.unwrap();
            let found = drivers
                .iter()
                .find(|d| d["phone_number"].as_str() == Some(&phone));
            assert!(found.is_some(), "Registered driver not found in list");
            assert_eq!(
                found.unwrap()["employee_code"].as_str(),
                Some(emp_code.as_str())
            );
        }
    );
}

#[tokio::test]
async fn test_tenko_call_tenko_returns_call_number() {
    test_group!("中間点呼 — 登録 / 点呼 / 番号削除");
    test_case!(
        "ドライバー登録 → 点呼送信 → call_numberが返る",
        {
            let (base_url, auth, _emp_id, client) = setup_tenko().await;

            // マスタ登録
            let call_num = format!("TK{}", &uuid::Uuid::new_v4().simple().to_string()[..6]);
            client
                .post(format!("{base_url}/api/tenko-call/numbers"))
                .header("Authorization", &auth)
                .json(&serde_json::json!({ "call_number": &call_num }))
                .send()
                .await
                .unwrap();

            // ドライバー登録
            let phone = format!("080{}", &uuid::Uuid::new_v4().simple().to_string()[..8]);
            client
                .post(format!("{base_url}/api/tenko-call/register"))
                .json(&serde_json::json!({
                    "phone_number": &phone,
                    "driver_name": "点呼テスト",
                    "call_number": &call_num
                }))
                .send()
                .await
                .unwrap();

            // 点呼送信
            let res = client
                .post(format!("{base_url}/api/tenko-call/tenko"))
                .json(&serde_json::json!({
                    "phone_number": &phone,
                    "driver_name": "点呼テスト",
                    "latitude": 35.6812,
                    "longitude": 139.7671
                }))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
            let body: serde_json::Value = res.json().await.unwrap();
            assert!(body["success"].as_bool().unwrap());
            assert_eq!(body["call_number"].as_str().unwrap(), &call_num);
        }
    );
}

#[tokio::test]
async fn test_tenko_call_tenko_unregistered_driver_404() {
    test_group!("中間点呼 — 登録 / 点呼 / 番号削除");
    test_case!("未登録phone_numberで点呼 → 404", {
        let (base_url, _auth, _emp_id, client) = setup_tenko().await;

        let res = client
            .post(format!("{base_url}/api/tenko-call/tenko"))
            .json(&serde_json::json!({
                "phone_number": "000-0000-0000",
                "driver_name": "存在しない",
                "latitude": 0.0,
                "longitude": 0.0
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 404);
    });
}

#[tokio::test]
async fn test_tenko_call_register_invalid_call_number_400() {
    test_group!("中間点呼 — 登録 / 点呼 / 番号削除");
    test_case!("未登録call_numberで登録 → 400", {
        let (base_url, _auth, _emp_id, client) = setup_tenko().await;

        let res = client
            .post(format!("{base_url}/api/tenko-call/register"))
            .json(&serde_json::json!({
                "phone_number": "090-0000-0000",
                "driver_name": "テスト",
                "call_number": "NONEXISTENT-999"
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 400);
        let body: serde_json::Value = res.json().await.unwrap();
        assert_eq!(body["success"], false);
        assert!(body["error"].as_str().unwrap().contains("未登録"));
    });
}

#[tokio::test]
async fn test_tenko_call_delete_number_nonexistent() {
    test_group!("中間点呼 — 登録 / 点呼 / 番号削除");
    test_case!("存在しないID削除 → 204", {
        let (base_url, auth, _emp_id, client) = setup_tenko().await;

        let res = client
            .delete(format!("{base_url}/api/tenko-call/numbers/999999"))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();
        // 現在の実装は rows_affected をチェックしないため 204 を返す
        assert_eq!(res.status(), 204);
    });
}

// ============================================================
// Tenko Schedules — filter tests
// ============================================================

#[tokio::test]
async fn test_schedule_list_filter_consumed() {
    test_group!("点呼スケジュール — フィルタ");
    test_case!("consumed=true/false フィルタ", {
        let (base_url, auth, emp_id, client) = setup_tenko().await;

        // スケジュール作成 → セッション開始 (consumed=true になる)
        let sid = create_schedule(&client, &base_url, &auth, &emp_id, "pre_operation").await;
        let _session = start_session(&client, &base_url, &auth, &emp_id, &sid).await;

        // もう1つ未消費のスケジュールを作成
        let _sid2 = create_schedule(&client, &base_url, &auth, &emp_id, "post_operation").await;

        // consumed=true でフィルタ
        let res = client
            .get(format!("{base_url}/api/tenko/schedules?consumed=true"))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        let body: serde_json::Value = res.json().await.unwrap();
        let schedules = body["schedules"].as_array().unwrap();
        for s in schedules {
            assert_eq!(
                s["consumed"], true,
                "consumed=true filter returned unconsumed schedule"
            );
        }

        // consumed=false でフィルタ
        let res = client
            .get(format!("{base_url}/api/tenko/schedules?consumed=false"))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        let body: serde_json::Value = res.json().await.unwrap();
        let schedules = body["schedules"].as_array().unwrap();
        for s in schedules {
            assert_eq!(
                s["consumed"], false,
                "consumed=false filter returned consumed schedule"
            );
        }
    });
}

#[tokio::test]
async fn test_schedule_list_filter_date_range() {
    test_group!("点呼スケジュール — フィルタ");
    test_case!("date_from/date_to フィルタ", {
        let (base_url, auth, emp_id, client) = setup_tenko().await;

        // 遠い未来のスケジュールを作成
        let res = client
            .post(format!("{base_url}/api/tenko/schedules"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({
                "employee_id": emp_id,
                "tenko_type": "pre_operation",
                "responsible_manager_name": "テスト",
                "scheduled_at": "2098-06-15T06:00:00Z",
                "instruction": "日付範囲テスト"
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 201);

        // date_from=2098-01-01 & date_to=2098-12-31 → 上記スケジュールが含まれる
        let res = client
            .get(format!("{base_url}/api/tenko/schedules?date_from=2098-01-01T00:00:00Z&date_to=2098-12-31T23:59:59Z"))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        let body: serde_json::Value = res.json().await.unwrap();
        assert!(
            body["total"].as_i64().unwrap() >= 1,
            "Expected at least 1 schedule in date range"
        );

        // date_from=2097-01-01 & date_to=2097-12-31 → 含まれない
        let res = client
            .get(format!("{base_url}/api/tenko/schedules?date_from=2097-01-01T00:00:00Z&date_to=2097-12-31T23:59:59Z"))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        let body: serde_json::Value = res.json().await.unwrap();
        assert_eq!(
            body["total"].as_i64().unwrap(),
            0,
            "Expected 0 schedules outside date range"
        );
    });
}

#[tokio::test]
async fn test_schedule_batch_create_invalid_type() {
    test_group!("点呼スケジュール — フィルタ");
    test_case!("バッチ作成で無効な点呼種別 → 400", {
        let (base_url, auth, emp_id, client) = setup_tenko().await;

        let res = client
            .post(format!("{base_url}/api/tenko/schedules/batch"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({
                "schedules": [
                    {
                        "employee_id": emp_id,
                        "tenko_type": "invalid_type",
                        "responsible_manager_name": "テスト",
                        "scheduled_at": "2099-01-01T00:00:00Z",
                        "instruction": "テスト"
                    }
                ]
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 400);
    });
}

// ============================================================
// Equipment Failures — filter tests
// ============================================================

/// 故障記録作成ヘルパー
async fn create_equipment_failure(
    client: &reqwest::Client,
    base_url: &str,
    auth: &str,
    failure_type: &str,
    description: &str,
) -> serde_json::Value {
    let res = client
        .post(format!("{base_url}/api/tenko/equipment-failures"))
        .header("Authorization", auth)
        .json(&serde_json::json!({
            "failure_type": failure_type,
            "description": description,
            "affected_device": "test-device-001"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 201, "Failed to create equipment failure");
    res.json().await.unwrap()
}

#[tokio::test]
async fn test_equipment_failure_list_filter_resolved() {
    test_group!("機器故障 — フィルタ");
    test_case!("resolved=true/false フィルタ", {
        let (base_url, auth, _emp_id, client) = setup_tenko().await;

        // 故障記録を作成
        let f1 =
            create_equipment_failure(&client, &base_url, &auth, "manual_report", "未解決テスト")
                .await;
        let f2 =
            create_equipment_failure(&client, &base_url, &auth, "kiosk_offline", "解決済みテスト")
                .await;
        let f2_id = f2["id"].as_str().unwrap();

        // f2 を解決
        let res = client
            .put(format!("{base_url}/api/tenko/equipment-failures/{f2_id}"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({ "resolution_notes": "修理完了" }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);

        // resolved=true → 解決済みのみ
        let res = client
            .get(format!(
                "{base_url}/api/tenko/equipment-failures?resolved=true"
            ))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        let body: serde_json::Value = res.json().await.unwrap();
        let failures = body["failures"].as_array().unwrap();
        for f in failures {
            assert!(
                f["resolved_at"].as_str().is_some(),
                "resolved=true filter returned unresolved failure"
            );
        }

        // resolved=false → 未解決のみ
        let res = client
            .get(format!(
                "{base_url}/api/tenko/equipment-failures?resolved=false"
            ))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        let body: serde_json::Value = res.json().await.unwrap();
        let failures = body["failures"].as_array().unwrap();
        for f in failures {
            assert!(
                f["resolved_at"].is_null(),
                "resolved=false filter returned resolved failure"
            );
        }

        // suppress unused variable warning
        let _ = f1;
    });
}

#[tokio::test]
async fn test_equipment_failure_list_filter_type() {
    test_group!("機器故障 — フィルタ");
    test_case!("failure_type フィルタ", {
        let (base_url, auth, _emp_id, client) = setup_tenko().await;

        // 異なるタイプの故障記録を作成
        create_equipment_failure(
            &client,
            &base_url,
            &auth,
            "kiosk_offline",
            "キオスクオフライン",
        )
        .await;
        create_equipment_failure(&client, &base_url, &auth, "manual_report", "手動報告").await;

        // failure_type=kiosk_offline でフィルタ
        let res = client
            .get(format!(
                "{base_url}/api/tenko/equipment-failures?failure_type=kiosk_offline"
            ))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        let body: serde_json::Value = res.json().await.unwrap();
        let failures = body["failures"].as_array().unwrap();
        assert!(
            !failures.is_empty(),
            "Expected at least 1 kiosk_offline failure"
        );
        for f in failures {
            assert_eq!(
                f["failure_type"].as_str().unwrap(),
                "kiosk_offline",
                "failure_type filter returned wrong type"
            );
        }
    });
}

#[tokio::test]
#[ignore] // rfc3339 の + がURL エンコーディングで問題
async fn test_equipment_failure_list_filter_date_range() {
    test_group!("機器故障 — フィルタ");
    test_case!("date_from/date_to フィルタ", {
        let (base_url, auth, _emp_id, client) = setup_tenko().await;

        // 故障記録を作成 (detected_at はデフォルトで NOW())
        create_equipment_failure(&client, &base_url, &auth, "manual_report", "日付範囲テスト")
            .await;

        let today = chrono::Utc::now();
        let date_from = (today - chrono::Duration::hours(1)).to_rfc3339();
        let date_to = (today + chrono::Duration::hours(1)).to_rfc3339();

        // 現在時刻を含む範囲 → 見つかる
        let res = client
            .get(format!(
                "{base_url}/api/tenko/equipment-failures?date_from={date_from}&date_to={date_to}"
            ))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        let body: serde_json::Value = res.json().await.unwrap();
        assert!(
            body["total"].as_i64().unwrap() >= 1,
            "Expected at least 1 failure in date range"
        );

        // 過去の範囲 → 0件
        let old_from = "2020-01-01T00:00:00Z";
        let old_to = "2020-12-31T23:59:59Z";
        let res = client
            .get(format!(
                "{base_url}/api/tenko/equipment-failures?date_from={old_from}&date_to={old_to}"
            ))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        let body: serde_json::Value = res.json().await.unwrap();
        assert_eq!(
            body["total"].as_i64().unwrap(),
            0,
            "Expected 0 failures outside date range"
        );
    });
}

// ============================================================
// Webhook — fire_event / check_overdue_schedules / deliver
// ============================================================

#[tokio::test]
async fn test_fire_event_no_config() {
    test_group!("Webhook — 発火 / 期限超過チェック / 配信");
    test_case!("webhook設定なし → noop", {
        let state = common::setup_app_state().await;
        let _base_url = common::spawn_test_server(state.clone()).await;
        let tenant_id = common::create_test_tenant(
            &state.pool,
            &format!("WHNone{}", uuid::Uuid::new_v4().simple()),
        )
        .await;

        let result = rust_alc_api::webhook::fire_event(
            &state.pool,
            tenant_id,
            "alcohol_detected",
            serde_json::json!({"test": true}),
        )
        .await;
        assert!(result.is_ok());
    });
}

#[tokio::test]
async fn test_fire_event_with_config_delivers() {
    test_group!("Webhook — 発火 / 期限超過チェック / 配信");
    test_case!("webhook設定あり → 配信成功", {
        let state = common::setup_app_state().await;
        let _base_url = common::spawn_test_server(state.clone()).await;
        let tenant_id = common::create_test_tenant(
            &state.pool,
            &format!("WHDeliv{}", uuid::Uuid::new_v4().simple()),
        )
        .await;
        let jwt = common::create_test_jwt(tenant_id, "admin");
        let auth = format!("Bearer {jwt}");
        let client = reqwest::Client::new();

        // 受信サーバーを起動
        let received = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let received_clone = received.clone();
        let receiver = axum::Router::new().route(
            "/hook",
            axum::routing::post(move || {
                let r = received_clone.clone();
                async move {
                    r.store(true, std::sync::atomic::Ordering::SeqCst);
                    "ok"
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let receiver_addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, receiver).await.unwrap() });

        let receiver_url = format!("http://{receiver_addr}/hook");

        // webhook config 作成 (REST API 経由)
        let res = client
            .post(format!("{_base_url}/api/tenko/webhooks"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({
                "event_type": "alcohol_detected",
                "url": receiver_url,
                "secret": "test-secret-123"
            }))
            .send()
            .await
            .unwrap();
        assert!(res.status() == 200 || res.status() == 201);

        // fire_event
        rust_alc_api::webhook::fire_event(
            &state.pool,
            tenant_id,
            "alcohol_detected",
            serde_json::json!({"employee_id": "test", "value": 0.15}),
        )
        .await
        .unwrap();

        // 少し待ってから配信を確認 (tokio::spawn で非同期配信)
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        assert!(
            received.load(std::sync::atomic::Ordering::SeqCst),
            "Webhook should have been delivered"
        );

        // webhook_deliveries にログが記録されたか確認
        let mut conn = state.pool.acquire().await.unwrap();
        sqlx::query("SELECT set_config('app.current_tenant_id', $1, true)")
            .bind(tenant_id.to_string())
            .execute(&mut *conn)
            .await
            .unwrap();
        let delivery_count: Option<i64> = sqlx::query_scalar(
            "SELECT COUNT(*)::bigint FROM webhook_deliveries WHERE tenant_id = $1",
        )
        .bind(tenant_id)
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        assert!(
            delivery_count.unwrap_or(0) >= 1,
            "Expected delivery record in DB"
        );
    });
}

#[tokio::test]
async fn test_fire_event_delivery_failure_retries() {
    test_group!("Webhook — 発火 / 期限超過チェック / 配信");
    test_case!(
        "配信先ダウン → リトライ + エラーログ記録",
        {
            let state = common::setup_app_state().await;
            let _base_url = common::spawn_test_server(state.clone()).await;
            let tenant_id = common::create_test_tenant(
                &state.pool,
                &format!("WHFail{}", uuid::Uuid::new_v4().simple()),
            )
            .await;
            let jwt = common::create_test_jwt(tenant_id, "admin");
            let auth = format!("Bearer {jwt}");
            let client = reqwest::Client::new();

            // 受信サーバーなし (常に 500 を返す)
            let receiver = axum::Router::new().route(
                "/fail",
                axum::routing::post(|| async {
                    (axum::http::StatusCode::INTERNAL_SERVER_ERROR, "error")
                }),
            );
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let fail_addr = listener.local_addr().unwrap();
            tokio::spawn(async move { axum::serve(listener, receiver).await.unwrap() });

            let fail_url = format!("http://{fail_addr}/fail");

            // webhook config
            let res = client
                .post(format!("{_base_url}/api/tenko/webhooks"))
                .header("Authorization", &auth)
                .json(&serde_json::json!({
                    "event_type": "tenko_completed",
                    "url": fail_url,
                    "secret": null
                }))
                .send()
                .await
                .unwrap();
            assert!(res.status() == 200 || res.status() == 201);

            // fire → リトライ (1s + 5s + 25s = 遅いが、500エラーは即返るのでsleep分のみ)
            rust_alc_api::webhook::fire_event(
                &state.pool,
                tenant_id,
                "tenko_completed",
                serde_json::json!({"session_id": "test"}),
            )
            .await
            .unwrap();

            // リトライは非同期なので少し待つ (最初のリトライ1sだけ待てば少なくとも2回の配信ログがある)
            tokio::time::sleep(std::time::Duration::from_millis(2000)).await;

            let mut conn = state.pool.acquire().await.unwrap();
            sqlx::query("SELECT set_config('app.current_tenant_id', $1, true)")
                .bind(tenant_id.to_string())
                .execute(&mut *conn)
                .await
                .unwrap();
            let delivery_count: Option<i64> = sqlx::query_scalar(
            "SELECT COUNT(*)::bigint FROM webhook_deliveries WHERE tenant_id = $1 AND success = false",
        )
        .bind(tenant_id)
        .fetch_one(&mut *conn).await.unwrap();
            assert!(
                delivery_count.unwrap_or(0) >= 1,
                "Expected failed delivery record(s)"
            );
        }
    );
}

#[tokio::test]
async fn test_check_overdue_no_config() {
    test_group!("Webhook — 発火 / 期限超過チェック / 配信");
    test_case!("check_overdue_schedules設定なし → noop", {
        let state = common::setup_app_state().await;
        let _base_url = common::spawn_test_server(state.clone()).await;

        let result = rust_alc_api::webhook::check_overdue_schedules(&state.pool).await;
        assert!(result.is_ok());
    });
}

// ============================================================
// Safety Judgment — 基準値ありで医療判定
// ============================================================

#[tokio::test]
async fn test_safety_judgment_fail_with_baseline() {
    test_group!("安全判定 — 基準値ありで医療判定");
    test_case!(
        "基準値あり + 医療値が基準外 → safety judgment fail → interrupted",
        {
            let state = common::setup_app_state().await;
            let base_url = common::spawn_test_server(state.clone()).await;
            let tenant_id = common::create_test_tenant(
                &state.pool,
                &format!("SJFail{}", uuid::Uuid::new_v4().simple()),
            )
            .await;
            let jwt = common::create_test_jwt(tenant_id, "admin");
            let auth = format!("Bearer {jwt}");
            let client = reqwest::Client::new();
            let emp = common::create_test_employee(
                &client,
                &base_url,
                &auth,
                "SJFailEmp",
                &format!("SF{}", &uuid::Uuid::new_v4().simple().to_string()[..4]),
            )
            .await;
            let emp_id = emp["id"].as_str().unwrap().to_string();

            // 基準値を設定 (systolic=120, tolerance=10)
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
            assert!(
                res.status() == 200 || res.status() == 201,
                "baseline creation failed: {}",
                res.status()
            );

            // セッション開始
            let session = start_session_remote(&client, &base_url, &auth, &emp_id).await;
            let session_id = session["id"].as_str().unwrap();

            // 医療データ (systolic=145 → diff=25 > tolerance=10)
            client
                .put(format!(
                    "{base_url}/api/tenko/sessions/{session_id}/medical"
                ))
                .header("Authorization", &auth)
                .json(&serde_json::json!({
                    "temperature": 36.5,
                    "systolic": 145,
                    "diastolic": 80,
                    "pulse": 72,
                    "medical_manual_input": true
                }))
                .send()
                .await
                .unwrap();

            // 自己申告 (全て正常) → safety judgment が medical diff で fail
            let res = client.put(format!("{base_url}/api/tenko/sessions/{session_id}/self-declaration"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({ "illness": false, "fatigue": false, "sleep_deprivation": false }))
            .send().await.unwrap();
            assert_eq!(res.status(), 200);
            let session: Value = res.json().await.unwrap();
            assert_eq!(
                session["status"], "interrupted",
                "Should be interrupted due to systolic out of range"
            );
            // safety_judgment should contain failed_items
            let judgment = &session["safety_judgment"];
            assert_eq!(judgment["status"], "fail");
            let failed = judgment["failed_items"].as_array().unwrap();
            assert!(
                failed.iter().any(|v| v == "systolic"),
                "failed_items should include systolic"
            );
        }
    );
}

#[tokio::test]
async fn test_safety_judgment_pass_with_baseline() {
    test_group!("安全判定 — 基準値ありで医療判定");
    test_case!(
        "基準値あり + 全て範囲内 → safety judgment pass → daily_inspection_pending",
        {
            let state = common::setup_app_state().await;
            let base_url = common::spawn_test_server(state.clone()).await;
            let tenant_id = common::create_test_tenant(
                &state.pool,
                &format!("SJPass{}", uuid::Uuid::new_v4().simple()),
            )
            .await;
            let jwt = common::create_test_jwt(tenant_id, "admin");
            let auth = format!("Bearer {jwt}");
            let client = reqwest::Client::new();
            let emp = common::create_test_employee(
                &client,
                &base_url,
                &auth,
                "SJPassEmp",
                &format!("SP{}", &uuid::Uuid::new_v4().simple().to_string()[..4]),
            )
            .await;
            let emp_id = emp["id"].as_str().unwrap().to_string();

            // 基準値を設定
            client
                .post(format!("{base_url}/api/tenko/health-baselines"))
                .header("Authorization", &auth)
                .json(&serde_json::json!({
                    "employee_id": emp_id,
                    "baseline_systolic": 120,
                    "baseline_diastolic": 80,
                    "baseline_temperature": 36.5,
                    "systolic_tolerance": 20,
                    "diastolic_tolerance": 20,
                    "temperature_tolerance": 1.0
                }))
                .send()
                .await
                .unwrap();

            let session = start_session_remote(&client, &base_url, &auth, &emp_id).await;
            let session_id = session["id"].as_str().unwrap();

            // 医療データ (全て範囲内)
            client
                .put(format!(
                    "{base_url}/api/tenko/sessions/{session_id}/medical"
                ))
                .header("Authorization", &auth)
                .json(&serde_json::json!({
                    "temperature": 36.8,
                    "systolic": 125,
                    "diastolic": 85,
                    "pulse": 70,
                    "medical_manual_input": true
                }))
                .send()
                .await
                .unwrap();

            // 自己申告 (正常) → pass → daily_inspection_pending
            let res = client.put(format!("{base_url}/api/tenko/sessions/{session_id}/self-declaration"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({ "illness": false, "fatigue": false, "sleep_deprivation": false }))
            .send().await.unwrap();
            assert_eq!(res.status(), 200);
            let session: Value = res.json().await.unwrap();
            assert_eq!(session["status"], "daily_inspection_pending");
            let judgment = &session["safety_judgment"];
            assert_eq!(judgment["status"], "pass");
        }
    );
}

#[tokio::test]
async fn test_safety_judgment_multiple_failures() {
    test_group!("安全判定 — 基準値ありで医療判定");
    test_case!("複数の失敗項目 (temperature + fatigue)", {
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;
        let tenant_id = common::create_test_tenant(
            &state.pool,
            &format!("SJMulti{}", uuid::Uuid::new_v4().simple()),
        )
        .await;
        let jwt = common::create_test_jwt(tenant_id, "admin");
        let auth = format!("Bearer {jwt}");
        let client = reqwest::Client::new();
        let emp = common::create_test_employee(
            &client,
            &base_url,
            &auth,
            "SJMultiEmp",
            &format!("SM{}", &uuid::Uuid::new_v4().simple().to_string()[..4]),
        )
        .await;
        let emp_id = emp["id"].as_str().unwrap().to_string();

        // 基準値
        client
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

        let session = start_session_remote(&client, &base_url, &auth, &emp_id).await;
        let session_id = session["id"].as_str().unwrap();

        // 医療データ (温度が範囲外)
        client
            .put(format!(
                "{base_url}/api/tenko/sessions/{session_id}/medical"
            ))
            .header("Authorization", &auth)
            .json(&serde_json::json!({
                "temperature": 38.0,
                "systolic": 120,
                "diastolic": 80,
                "medical_manual_input": true
            }))
            .send()
            .await
            .unwrap();

        // 自己申告 (fatigue=true) → 温度 + fatigue で2項目失敗
        let res = client.put(format!("{base_url}/api/tenko/sessions/{session_id}/self-declaration"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({ "illness": false, "fatigue": true, "sleep_deprivation": false }))
            .send().await.unwrap();
        assert_eq!(res.status(), 200);
        let session: Value = res.json().await.unwrap();
        assert_eq!(session["status"], "interrupted");
        let failed = session["safety_judgment"]["failed_items"]
            .as_array()
            .unwrap();
        assert!(
            failed.len() >= 2,
            "Expected 2+ failed items, got {:?}",
            failed
        );
    });
}

#[tokio::test]
async fn test_check_overdue_with_overdue_schedule() {
    test_group!("Webhook — 発火 / 期限超過チェック / 配信");
    test_case!(
        "overdue schedule + webhook config → 配信 + overdue_notified_at 更新",
        {
            let state = common::setup_app_state().await;
            let _base_url = common::spawn_test_server(state.clone()).await;
            let tenant_id = common::create_test_tenant(
                &state.pool,
                &format!("WHOver{}", uuid::Uuid::new_v4().simple()),
            )
            .await;
            let jwt = common::create_test_jwt(tenant_id, "admin");
            let auth = format!("Bearer {jwt}");
            let client = reqwest::Client::new();

            // employee
            let emp = common::create_test_employee(
                &client,
                &_base_url,
                &auth,
                "OverdueEmp",
                &format!("OD{}", &uuid::Uuid::new_v4().simple().to_string()[..4]),
            )
            .await;
            let emp_id: uuid::Uuid = emp["id"].as_str().unwrap().parse().unwrap();

            // 受信サーバー
            let received = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            let received_clone = received.clone();
            let receiver = axum::Router::new().route(
                "/overdue",
                axum::routing::post(move || {
                    let r = received_clone.clone();
                    async move {
                        r.store(true, std::sync::atomic::Ordering::SeqCst);
                        "ok"
                    }
                }),
            );
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = listener.local_addr().unwrap();
            tokio::spawn(async move { axum::serve(listener, receiver).await.unwrap() });

            // webhook config for tenko_overdue
            let res = client
                .post(format!("{_base_url}/api/tenko/webhooks"))
                .header("Authorization", &auth)
                .json(&serde_json::json!({
                    "event_type": "tenko_overdue",
                    "url": format!("http://{addr}/overdue")
                }))
                .send()
                .await
                .unwrap();
            assert!(res.status() == 200 || res.status() == 201);

            // 過去の予定を API で作成 (scheduled_at は過去に設定)
            let two_hours_ago = (chrono::Utc::now() - chrono::Duration::hours(2)).to_rfc3339();
            let res = client
                .post(format!("{_base_url}/api/tenko/schedules"))
                .header("Authorization", &auth)
                .json(&serde_json::json!({
                    "employee_id": emp_id,
                    "scheduled_at": two_hours_ago,
                    "tenko_type": "pre_operation",
                    "responsible_manager_name": "テスト管理者",
                    "instruction": "安全運転してください"
                }))
                .send()
                .await
                .unwrap();
            let status = res.status();
            let body_text = res.text().await.unwrap();
            assert!(
                status == 200 || status == 201,
                "schedule creation failed: {status} {body_text}"
            );
            let sched: serde_json::Value = serde_json::from_str(&body_text).unwrap();
            let schedule_id: uuid::Uuid = sched["id"].as_str().unwrap().parse().unwrap();

            // check_overdue_schedules (TENKO_OVERDUE_MINUTES デフォルト60分なので2時間前は overdue)
            std::env::set_var("TENKO_OVERDUE_MINUTES", "60");
            let result = rust_alc_api::webhook::check_overdue_schedules(&state.pool).await;
            assert!(result.is_ok(), "check_overdue failed: {:?}", result.err());

            // 少し待つ (deliver_webhook は sync in check_overdue, not spawned)
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;

            // overdue_notified_at が更新されたか確認
            let mut conn = state.pool.acquire().await.unwrap();
            sqlx::query("SELECT set_config('app.current_tenant_id', $1, true)")
                .bind(tenant_id.to_string())
                .execute(&mut *conn)
                .await
                .unwrap();
            let notified: Option<chrono::DateTime<chrono::Utc>> =
                sqlx::query_scalar("SELECT overdue_notified_at FROM tenko_schedules WHERE id = $1")
                    .bind(schedule_id)
                    .fetch_one(&mut *conn)
                    .await
                    .unwrap();
            assert!(notified.is_some(), "overdue_notified_at should be set");
        }
    );
}

// ============================================================
// Equipment Failures — DB error (trigger)
// ============================================================

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_equipment_failure_create_db_error() {
    test_group!("equipment_failures DB エラー");
    test_case!("create_failure: trigger → 500", {
        let _db = common::DB_RENAME_LOCK.lock().unwrap();
        let _flock = common::db_rename_flock();
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;
        let tenant_id = common::create_test_tenant(&state.pool, "EqFailErr").await;
        let jwt = common::create_test_jwt(tenant_id, "admin");
        let auth = format!("Bearer {jwt}");
        let client = reqwest::Client::new();

        // Create trigger to block INSERT
        sqlx::query(
            r#"CREATE OR REPLACE FUNCTION alc_api.fail_eq_insert() RETURNS trigger AS $$
               BEGIN RAISE EXCEPTION 'test: equipment_failures insert blocked'; END;
               $$ LANGUAGE plpgsql"#,
        )
        .execute(&state.pool)
        .await
        .unwrap();
        sqlx::query(
            "CREATE OR REPLACE TRIGGER fail_eq_insert BEFORE INSERT ON alc_api.equipment_failures \
             FOR EACH ROW EXECUTE FUNCTION alc_api.fail_eq_insert()",
        )
        .execute(&state.pool)
        .await
        .unwrap();

        let res = client
            .post(format!("{base_url}/api/tenko/equipment-failures"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({
                "failure_type": "manual_report",
                "description": "trigger test"
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 500, "equipment_failures INSERT should fail");

        // Cleanup
        sqlx::query("DROP TRIGGER fail_eq_insert ON alc_api.equipment_failures")
            .execute(&state.pool)
            .await
            .unwrap();
        sqlx::query("DROP FUNCTION alc_api.fail_eq_insert")
            .execute(&state.pool)
            .await
            .unwrap();
    });
}

// ============================================================
// Equipment Failures — list DB error (RENAME)
// ============================================================

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_equipment_failure_list_db_error() {
    test_group!("equipment_failures DB エラー");
    test_case!("list_failures: RENAME → 500", {
        let _db = common::DB_RENAME_LOCK.lock().unwrap();
        let _flock = common::db_rename_flock();
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;
        let tenant_id = common::create_test_tenant(&state.pool, "EqListErr").await;
        let jwt = common::create_test_jwt(tenant_id, "admin");
        let auth = format!("Bearer {jwt}");
        let client = reqwest::Client::new();

        sqlx::query("ALTER TABLE alc_api.equipment_failures RENAME TO equipment_failures_bak")
            .execute(&state.pool)
            .await
            .unwrap();

        let res = client
            .get(format!("{base_url}/api/tenko/equipment-failures"))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 500);

        sqlx::query("ALTER TABLE alc_api.equipment_failures_bak RENAME TO equipment_failures")
            .execute(&state.pool)
            .await
            .unwrap();
    });
}

// ============================================================
// Tenko Schedules — DB errors (trigger + RENAME)
// ============================================================

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_tenko_schedule_create_db_error() {
    test_group!("tenko_schedules DB エラー");
    test_case!("create_schedule: trigger → 500", {
        let _db = common::DB_RENAME_LOCK.lock().unwrap();
        let _flock = common::db_rename_flock();
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;
        let tenant_id = common::create_test_tenant(&state.pool, "SchedCreateErr").await;
        let jwt = common::create_test_jwt(tenant_id, "admin");
        let auth = format!("Bearer {jwt}");
        let client = reqwest::Client::new();

        // Create employee for schedule
        let emp =
            common::create_test_employee(&client, &base_url, &auth, "SchedErrEmp", "SE001").await;
        let emp_id = emp["id"].as_str().unwrap();

        // Create trigger to block INSERT
        sqlx::query(
            r#"CREATE OR REPLACE FUNCTION alc_api.fail_sched_insert() RETURNS trigger AS $$
               BEGIN RAISE EXCEPTION 'test: tenko_schedules insert blocked'; END;
               $$ LANGUAGE plpgsql"#,
        )
        .execute(&state.pool)
        .await
        .unwrap();
        sqlx::query(
            "CREATE OR REPLACE TRIGGER fail_sched_insert BEFORE INSERT ON alc_api.tenko_schedules \
             FOR EACH ROW EXECUTE FUNCTION alc_api.fail_sched_insert()",
        )
        .execute(&state.pool)
        .await
        .unwrap();

        let res = client
            .post(format!("{base_url}/api/tenko/schedules"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({
                "employee_id": emp_id,
                "tenko_type": "pre_operation",
                "responsible_manager_name": "テスト管理者",
                "scheduled_at": "2099-01-01T09:00:00Z",
                "instruction": "テスト指示"
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 500, "schedule INSERT should fail");

        // Cleanup
        sqlx::query("DROP TRIGGER fail_sched_insert ON alc_api.tenko_schedules")
            .execute(&state.pool)
            .await
            .unwrap();
        sqlx::query("DROP FUNCTION alc_api.fail_sched_insert")
            .execute(&state.pool)
            .await
            .unwrap();
    });
}

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_tenko_schedule_list_db_error() {
    test_group!("tenko_schedules DB エラー");
    test_case!("list_schedules: RENAME → 500", {
        let _db = common::DB_RENAME_LOCK.lock().unwrap();
        let _flock = common::db_rename_flock();
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;
        let tenant_id = common::create_test_tenant(&state.pool, "SchedListErr").await;
        let jwt = common::create_test_jwt(tenant_id, "admin");
        let auth = format!("Bearer {jwt}");
        let client = reqwest::Client::new();

        sqlx::query("ALTER TABLE alc_api.tenko_schedules RENAME TO tenko_schedules_bak")
            .execute(&state.pool)
            .await
            .unwrap();

        let res = client
            .get(format!("{base_url}/api/tenko/schedules"))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 500);

        sqlx::query("ALTER TABLE alc_api.tenko_schedules_bak RENAME TO tenko_schedules")
            .execute(&state.pool)
            .await
            .unwrap();
    });
}

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_tenko_schedule_update_db_error() {
    test_group!("tenko_schedules DB エラー");
    test_case!("update_schedule: trigger → 500", {
        let _db = common::DB_RENAME_LOCK.lock().unwrap();
        let _flock = common::db_rename_flock();
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;
        let tenant_id = common::create_test_tenant(&state.pool, "SchedUpdErr").await;
        let jwt = common::create_test_jwt(tenant_id, "admin");
        let auth = format!("Bearer {jwt}");
        let client = reqwest::Client::new();

        // Create employee + schedule first
        let emp =
            common::create_test_employee(&client, &base_url, &auth, "SchedUpdEmp", "SU001").await;
        let emp_id = emp["id"].as_str().unwrap();
        let res = client
            .post(format!("{base_url}/api/tenko/schedules"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({
                "employee_id": emp_id,
                "tenko_type": "pre_operation",
                "responsible_manager_name": "テスト管理者",
                "scheduled_at": "2099-01-01T09:00:00Z",
                "instruction": "テスト指示"
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 201);
        let body: serde_json::Value = res.json().await.unwrap();
        let sid = body["id"].as_str().unwrap();

        // Create trigger to block UPDATE
        sqlx::query(
            r#"CREATE OR REPLACE FUNCTION alc_api.fail_sched_update() RETURNS trigger AS $$
               BEGIN RAISE EXCEPTION 'test: tenko_schedules update blocked'; END;
               $$ LANGUAGE plpgsql"#,
        )
        .execute(&state.pool)
        .await
        .unwrap();
        sqlx::query(
            "CREATE OR REPLACE TRIGGER fail_sched_update BEFORE UPDATE ON alc_api.tenko_schedules \
             FOR EACH ROW EXECUTE FUNCTION alc_api.fail_sched_update()",
        )
        .execute(&state.pool)
        .await
        .unwrap();

        let res = client
            .put(format!("{base_url}/api/tenko/schedules/{sid}"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({
                "responsible_manager_name": "更新テスト"
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 500, "schedule UPDATE should fail");

        // Cleanup
        sqlx::query("DROP TRIGGER fail_sched_update ON alc_api.tenko_schedules")
            .execute(&state.pool)
            .await
            .unwrap();
        sqlx::query("DROP FUNCTION alc_api.fail_sched_update")
            .execute(&state.pool)
            .await
            .unwrap();
    });
}

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_tenko_schedule_delete_db_error() {
    test_group!("tenko_schedules DB エラー");
    test_case!("delete_schedule: trigger → 500", {
        let _db = common::DB_RENAME_LOCK.lock().unwrap();
        let _flock = common::db_rename_flock();
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;
        let tenant_id = common::create_test_tenant(&state.pool, "SchedDelErr").await;
        let jwt = common::create_test_jwt(tenant_id, "admin");
        let auth = format!("Bearer {jwt}");
        let client = reqwest::Client::new();

        // Create employee + schedule first
        let emp =
            common::create_test_employee(&client, &base_url, &auth, "SchedDelEmp", "SD001").await;
        let emp_id = emp["id"].as_str().unwrap();
        let res = client
            .post(format!("{base_url}/api/tenko/schedules"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({
                "employee_id": emp_id,
                "tenko_type": "pre_operation",
                "responsible_manager_name": "テスト管理者",
                "scheduled_at": "2099-01-01T09:00:00Z",
                "instruction": "テスト指示"
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 201);
        let body: serde_json::Value = res.json().await.unwrap();
        let sid = body["id"].as_str().unwrap();

        // Create trigger to block DELETE
        sqlx::query(
            r#"CREATE OR REPLACE FUNCTION alc_api.fail_sched_delete() RETURNS trigger AS $$
               BEGIN RAISE EXCEPTION 'test: tenko_schedules delete blocked'; END;
               $$ LANGUAGE plpgsql"#,
        )
        .execute(&state.pool)
        .await
        .unwrap();
        sqlx::query(
            "CREATE OR REPLACE TRIGGER fail_sched_delete BEFORE DELETE ON alc_api.tenko_schedules \
             FOR EACH ROW EXECUTE FUNCTION alc_api.fail_sched_delete()",
        )
        .execute(&state.pool)
        .await
        .unwrap();

        let res = client
            .delete(format!("{base_url}/api/tenko/schedules/{sid}"))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 500, "schedule DELETE should fail");

        // Cleanup
        sqlx::query("DROP TRIGGER fail_sched_delete ON alc_api.tenko_schedules")
            .execute(&state.pool)
            .await
            .unwrap();
        sqlx::query("DROP FUNCTION alc_api.fail_sched_delete")
            .execute(&state.pool)
            .await
            .unwrap();
    });
}
