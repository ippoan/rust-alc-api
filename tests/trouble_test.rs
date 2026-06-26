#[macro_use]
mod common;

use serde_json::Value;

// ============================================================
// 1. Workflow setup and CRUD
// ============================================================

#[tokio::test]
async fn test_trouble_workflow_setup_and_crud() {
    test_group!("ワークフロー設定とCRUD");
    test_case!(
        "ワークフロー初期設定→状態一覧→遷移一覧→状態追加→状態削除",
        {
            let state = common::setup_app_state().await;
            let base_url = common::spawn_test_server(state.clone()).await;
            let tenant_id =
                common::create_test_tenant(state.pool(), "Trouble Workflow Tenant").await;
            let jwt = common::create_test_jwt(tenant_id, "admin");
            let client = reqwest::Client::new();
            let auth = format!("Bearer {jwt}");

            // POST /api/trouble/workflow/setup → 200, returns 4 states
            let res = client
                .post(format!("{base_url}/api/trouble/workflow/setup"))
                .header("Authorization", &auth)
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
            let body: Vec<Value> = res.json().await.unwrap();
            assert_eq!(body.len(), 4, "setup should return 4 default states");

            // GET /api/trouble/workflow/states → 200, 4 states
            let res = client
                .get(format!("{base_url}/api/trouble/workflow/states"))
                .header("Authorization", &auth)
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
            let states: Vec<Value> = res.json().await.unwrap();
            assert_eq!(states.len(), 4, "should have 4 workflow states");

            // GET /api/trouble/workflow/transitions → 200
            let res = client
                .get(format!("{base_url}/api/trouble/workflow/transitions"))
                .header("Authorization", &auth)
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
            let transitions: Vec<Value> = res.json().await.unwrap();
            // setup_defaults creates transitions too
            assert!(
                transitions.len() >= 4,
                "should have transitions after setup"
            );

            // POST /api/trouble/workflow/states with new state
            let res = client
                .post(format!("{base_url}/api/trouble/workflow/states"))
                .header("Authorization", &auth)
                .json(&serde_json::json!({
                    "name": "pending",
                    "label": "保留",
                    "color": "#FF0000"
                }))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 201);
            let new_state: Value = res.json().await.unwrap();
            assert_eq!(new_state["name"], "pending");
            assert_eq!(new_state["label"], "保留");
            assert_eq!(new_state["color"], "#FF0000");
            let new_id = new_state["id"].as_str().unwrap();

            // DELETE /api/trouble/workflow/states/{new_id} → 204
            let res = client
                .delete(format!("{base_url}/api/trouble/workflow/states/{new_id}"))
                .header("Authorization", &auth)
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 204);
        }
    );
}

// ============================================================
// 2. Ticket CRUD
// ============================================================

#[tokio::test]
async fn test_trouble_ticket_crud() {
    test_group!("チケットCRUD");
    test_case!(
        "チケット作成→一覧→取得→更新→削除→削除後404",
        {
            let state = common::setup_app_state().await;
            let base_url = common::spawn_test_server(state.clone()).await;
            let tenant_id = common::create_test_tenant(state.pool(), "Trouble Ticket Tenant").await;
            let jwt = common::create_test_jwt(tenant_id, "admin");
            let client = reqwest::Client::new();
            let auth = format!("Bearer {jwt}");

            // Setup workflow first
            let res = client
                .post(format!("{base_url}/api/trouble/workflow/setup"))
                .header("Authorization", &auth)
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);

            // POST /api/trouble/tickets
            let res = client
                .post(format!("{base_url}/api/trouble/tickets"))
                .header("Authorization", &auth)
                .json(&serde_json::json!({
                    "category": "貨物事故",
                    "title": "テスト事故報告",
                    "person_name": "テスト太郎",
                    "description": "テスト説明",
                    "registration_number": "品川300あ1234"
                }))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 201);
            let ticket: Value = res.json().await.unwrap();
            let ticket_id = ticket["id"].as_str().unwrap();
            assert_eq!(ticket["category"], "貨物事故");
            assert_eq!(ticket["person_name"], "テスト太郎");
            assert_eq!(ticket["registration_number"], "品川300あ1234");

            // GET /api/trouble/tickets → total >= 1
            let res = client
                .get(format!("{base_url}/api/trouble/tickets"))
                .header("Authorization", &auth)
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
            let body: Value = res.json().await.unwrap();
            assert!(body["total"].as_i64().unwrap() >= 1);

            // GET /api/trouble/tickets/{id}
            let res = client
                .get(format!("{base_url}/api/trouble/tickets/{ticket_id}"))
                .header("Authorization", &auth)
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
            let fetched: Value = res.json().await.unwrap();
            assert_eq!(fetched["id"], ticket_id);

            // PUT /api/trouble/tickets/{id}
            let res = client
                .put(format!("{base_url}/api/trouble/tickets/{ticket_id}"))
                .header("Authorization", &auth)
                .json(&serde_json::json!({
                    "progress_notes": "対応中",
                    "registration_number": "横浜500さ5678"
                }))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
            let updated: Value = res.json().await.unwrap();
            assert_eq!(updated["progress_notes"], "対応中");
            assert_eq!(updated["registration_number"], "横浜500さ5678");

            // PUT registration_number=null → 空文字列にクリア (DB は NOT NULL DEFAULT '')
            let res = client
                .put(format!("{base_url}/api/trouble/tickets/{ticket_id}"))
                .header("Authorization", &auth)
                .json(&serde_json::json!({ "registration_number": null }))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
            let cleared: Value = res.json().await.unwrap();
            assert_eq!(
                cleared["registration_number"], "",
                "registration_number should be empty string after explicit clear"
            );
            // 他フィールドは保持されている
            assert_eq!(cleared["progress_notes"], "対応中");

            // PUT registration_number 省略 → 直近値 ("") を保持
            let res = client
                .put(format!("{base_url}/api/trouble/tickets/{ticket_id}"))
                .header("Authorization", &auth)
                .json(&serde_json::json!({ "progress_notes": "再対応" }))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
            let kept: Value = res.json().await.unwrap();
            assert_eq!(kept["registration_number"], "");
            assert_eq!(kept["progress_notes"], "再対応");

            // DELETE /api/trouble/tickets/{id} → 204
            let res = client
                .delete(format!("{base_url}/api/trouble/tickets/{ticket_id}"))
                .header("Authorization", &auth)
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 204);

            // GET /api/trouble/tickets/{id} → 404 (soft deleted)
            let res = client
                .get(format!("{base_url}/api/trouble/tickets/{ticket_id}"))
                .header("Authorization", &auth)
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 404, "soft deleted ticket should return 404");
        }
    );
}

// ============================================================
// 3. Ticket transition
// ============================================================

#[tokio::test]
async fn test_trouble_ticket_transition() {
    test_group!("チケット状態遷移");
    test_case!("チケット作成→in_progress遷移→履歴確認", {
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;
        let tenant_id = common::create_test_tenant(state.pool(), "Trouble Transition Tenant").await;
        let jwt = common::create_test_jwt(tenant_id, "admin");
        let client = reqwest::Client::new();
        let auth = format!("Bearer {jwt}");

        // Setup workflow
        let res = client
            .post(format!("{base_url}/api/trouble/workflow/setup"))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);

        // Create ticket
        let res = client
            .post(format!("{base_url}/api/trouble/tickets"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({
                "category": "被害事故",
                "title": "遷移テスト"
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 201);
        let ticket: Value = res.json().await.unwrap();
        let ticket_id = ticket["id"].as_str().unwrap();

        // Get workflow states to find in_progress state id
        let res = client
            .get(format!("{base_url}/api/trouble/workflow/states"))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        let states: Vec<Value> = res.json().await.unwrap();
        let in_progress = states
            .iter()
            .find(|s| s["name"] == "in_progress")
            .expect("in_progress state should exist");
        let in_progress_id = in_progress["id"].as_str().unwrap();

        // POST /api/trouble/tickets/{id}/transition
        let res = client
            .post(format!(
                "{base_url}/api/trouble/tickets/{ticket_id}/transition"
            ))
            .header("Authorization", &auth)
            .json(&serde_json::json!({
                "to_state_id": in_progress_id
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);

        // GET /api/trouble/tickets/{id}/history → len >= 2
        let res = client
            .get(format!(
                "{base_url}/api/trouble/tickets/{ticket_id}/history"
            ))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        let history: Vec<Value> = res.json().await.unwrap();
        assert!(
            history.len() >= 2,
            "history should have at least 2 entries (initial + transition), got {}",
            history.len()
        );
    });
}

// ============================================================
// 5. CSV export
// ============================================================

#[tokio::test]
async fn test_trouble_ticket_csv_export() {
    test_group!("CSV出力");
    test_case!("チケット作成→CSVエクスポート", {
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;
        let tenant_id = common::create_test_tenant(state.pool(), "Trouble CSV Tenant").await;
        let jwt = common::create_test_jwt(tenant_id, "admin");
        let client = reqwest::Client::new();
        let auth = format!("Bearer {jwt}");

        // Setup workflow + create ticket
        client
            .post(format!("{base_url}/api/trouble/workflow/setup"))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();

        let res = client
            .post(format!("{base_url}/api/trouble/tickets"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({
                "category": "人身事故",
                "title": "CSVテスト",
                "person_name": "CSV太郎",
                "description": "CSV出力テスト用"
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 201);

        // GET CSV
        let res = client
            .get(format!("{base_url}/api/trouble/tickets/csv"))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);

        let ct = res.headers().get("content-type").unwrap().to_str().unwrap();
        assert!(ct.contains("text/csv"), "content-type should be text/csv");

        let cd = res
            .headers()
            .get("content-disposition")
            .unwrap()
            .to_str()
            .unwrap();
        assert!(cd.contains("trouble_tickets.csv"));

        let body = res.text().await.unwrap();
        assert!(body.contains("CSV太郎"), "CSV should contain person_name");
    });
}

// ============================================================
// 6. Tenant isolation
// ============================================================

#[tokio::test]
async fn test_trouble_tenant_isolation() {
    test_group!("テナント分離");
    test_case!(
        "テナントAのチケットがテナントBから見えないこと",
        {
            let state = common::setup_app_state().await;
            let base_url = common::spawn_test_server(state.clone()).await;

            let tenant_a = common::create_test_tenant(state.pool(), "Trouble Tenant A").await;
            let tenant_b = common::create_test_tenant(state.pool(), "Trouble Tenant B").await;

            let jwt_a = common::create_test_jwt(tenant_a, "admin");
            let jwt_b = common::create_test_jwt(tenant_b, "admin");

            let client = reqwest::Client::new();
            let auth_a = format!("Bearer {jwt_a}");
            let auth_b = format!("Bearer {jwt_b}");

            // Setup workflow for both tenants
            for auth in [&auth_a, &auth_b] {
                client
                    .post(format!("{base_url}/api/trouble/workflow/setup"))
                    .header("Authorization", auth)
                    .send()
                    .await
                    .unwrap();
            }

            // Create ticket in tenant A
            let res = client
                .post(format!("{base_url}/api/trouble/tickets"))
                .header("Authorization", &auth_a)
                .json(&serde_json::json!({
                    "category": "貨物事故",
                    "title": "テナントAのチケット"
                }))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 201);

            // Tenant A sees 1 ticket
            let res = client
                .get(format!("{base_url}/api/trouble/tickets"))
                .header("Authorization", &auth_a)
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
            let body: Value = res.json().await.unwrap();
            assert_eq!(body["total"], 1, "Tenant A should see 1 ticket");

            // Tenant B sees 0 tickets (RLS isolation)
            let res = client
                .get(format!("{base_url}/api/trouble/tickets"))
                .header("Authorization", &auth_b)
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
            let body: Value = res.json().await.unwrap();
            assert_eq!(
                body["total"], 0,
                "Tenant B should see 0 tickets (RLS isolation)"
            );
        }
    );
}

// ============================================================
// 7. Invalid category
// ============================================================

#[tokio::test]
async fn test_trouble_ticket_invalid_category() {
    test_group!("バリデーション");
    test_case!("無効なカテゴリでチケット作成→400", {
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;
        let tenant_id =
            common::create_test_tenant(state.pool(), "Trouble Invalid Cat Tenant").await;
        let jwt = common::create_test_jwt(tenant_id, "admin");
        let client = reqwest::Client::new();
        let auth = format!("Bearer {jwt}");

        let res = client
            .post(format!("{base_url}/api/trouble/tickets"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({
                "category": "invalid_category_name",
                "title": "should fail"
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 400);
    });
}

// ============================================================
// 8. Workflow transition CRUD
// ============================================================

#[tokio::test]
async fn test_trouble_workflow_transition_crud() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(state.pool(), "Transition CRUD").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();
    let auth = format!("Bearer {jwt}");

    // Setup defaults
    let res = client
        .post(format!("{base_url}/api/trouble/workflow/setup"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let states: Vec<Value> = res.json().await.unwrap();

    // Add state "on_hold"
    let res = client
        .post(format!("{base_url}/api/trouble/workflow/states"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({"name": "on_hold", "label": "保留"}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 201);
    let on_hold: Value = res.json().await.unwrap();
    let on_hold_id = on_hold["id"].as_str().unwrap();

    // Create transition: new → on_hold
    let new_id = states.iter().find(|s| s["name"] == "new").unwrap()["id"]
        .as_str()
        .unwrap();
    let res = client
        .post(format!("{base_url}/api/trouble/workflow/transitions"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "from_state_id": new_id,
            "to_state_id": on_hold_id,
            "label": "保留にする"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 201);
    let tr: Value = res.json().await.unwrap();
    let tr_id = tr["id"].as_str().unwrap();

    // Delete transition
    let res = client
        .delete(format!(
            "{base_url}/api/trouble/workflow/transitions/{tr_id}"
        ))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 204);
}

// ============================================================
// 10. File metadata CRUD (covers repo/trouble_files.rs)
// ============================================================

#[tokio::test]
async fn test_trouble_file_metadata_crud() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(state.pool(), "File Meta").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();
    let auth = format!("Bearer {jwt}");

    // Setup + create ticket
    client
        .post(format!("{base_url}/api/trouble/workflow/setup"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    let res = client
        .post(format!("{base_url}/api/trouble/tickets"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({"category": "その他", "person_name": "ファイルテスト"}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 201);
    let ticket: Value = res.json().await.unwrap();
    let ticket_id = ticket["id"].as_str().unwrap();

    // List files (empty)
    let res = client
        .get(format!("{base_url}/api/trouble/tickets/{ticket_id}/files"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let files: Vec<Value> = res.json().await.unwrap();
    assert_eq!(files.len(), 0);

    // Upload file via multipart API (covers PgTroubleFilesRepository::create)
    let form = reqwest::multipart::Form::new().part(
        "file",
        reqwest::multipart::Part::bytes(b"hello world".to_vec())
            .file_name("test.pdf")
            .mime_str("application/pdf")
            .unwrap(),
    );
    let res = client
        .post(format!("{base_url}/api/trouble/tickets/{ticket_id}/files"))
        .header("Authorization", &auth)
        .multipart(form)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 201);
    let file: Value = res.json().await.unwrap();
    let file_id = file["id"].as_str().unwrap();
    assert_eq!(file["filename"], "test.pdf");
    assert_eq!(file["content_type"], "application/pdf");

    // List files (1)
    let res = client
        .get(format!("{base_url}/api/trouble/tickets/{ticket_id}/files"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let files: Vec<Value> = res.json().await.unwrap();
    assert_eq!(files.len(), 1);

    // Download file (MockStorage has the data)
    let res = client
        .get(format!("{base_url}/api/trouble/files/{file_id}/download"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    // Delete file (soft delete)
    let res = client
        .delete(format!("{base_url}/api/trouble/files/{file_id}"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 204);

    // List files (0 — soft deleted files are hidden)
    let res = client
        .get(format!("{base_url}/api/trouble/tickets/{ticket_id}/files"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let files: Vec<Value> = res.json().await.unwrap();
    assert_eq!(
        files.len(),
        0,
        "soft deleted file should not appear in list"
    );

    // List trash (1 — soft deleted file appears here)
    let res = client
        .get(format!(
            "{base_url}/api/trouble/tickets/{ticket_id}/files/trash"
        ))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let trash: Vec<Value> = res.json().await.unwrap();
    assert_eq!(trash.len(), 1, "trash should contain 1 file");
    assert!(
        trash[0]["deleted_at"].as_str().is_some(),
        "deleted_at should be set"
    );

    // Restore file
    let res = client
        .post(format!("{base_url}/api/trouble/files/{file_id}/restore"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 204);

    // List files (1 — restored)
    let res = client
        .get(format!("{base_url}/api/trouble/tickets/{ticket_id}/files"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let files: Vec<Value> = res.json().await.unwrap();
    assert_eq!(files.len(), 1, "restored file should appear in list");

    // Trash empty after restore
    let res = client
        .get(format!(
            "{base_url}/api/trouble/tickets/{ticket_id}/files/trash"
        ))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let trash: Vec<Value> = res.json().await.unwrap();
    assert_eq!(trash.len(), 0, "trash should be empty after restore");
}

// ============================================================
// 11. Task file CRUD (covers repo/trouble_files.rs create_for_task + list_by_task)
// ============================================================

#[tokio::test]
async fn test_trouble_task_file_crud() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(state.pool(), "Task File").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();
    let auth = format!("Bearer {jwt}");

    // Setup workflow + create ticket
    client
        .post(format!("{base_url}/api/trouble/workflow/setup"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    let res = client
        .post(format!("{base_url}/api/trouble/tickets"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({"category": "その他", "person_name": "タスクファイルテスト"}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 201);
    let ticket: Value = res.json().await.unwrap();
    let ticket_id = ticket["id"].as_str().unwrap();

    // Create task
    let res = client
        .post(format!("{base_url}/api/trouble/tickets/{ticket_id}/tasks"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({"title": "修理手配", "task_type": "修理手配"}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 201);
    let task: Value = res.json().await.unwrap();
    let task_id = task["id"].as_str().unwrap();

    // List task files (empty)
    let res = client
        .get(format!("{base_url}/api/trouble/tasks/{task_id}/files"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let files: Vec<Value> = res.json().await.unwrap();
    assert_eq!(files.len(), 0);

    // Upload task file (covers create_for_task)
    let form = reqwest::multipart::Form::new().part(
        "file",
        reqwest::multipart::Part::bytes(b"task file data".to_vec())
            .file_name("report.pdf")
            .mime_str("application/pdf")
            .unwrap(),
    );
    let res = client
        .post(format!("{base_url}/api/trouble/tasks/{task_id}/files"))
        .header("Authorization", &auth)
        .multipart(form)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 201);
    let file: Value = res.json().await.unwrap();
    let file_id = file["id"].as_str().unwrap();
    assert_eq!(file["filename"], "report.pdf");
    assert_eq!(file["content_type"], "application/pdf");
    assert!(file["task_id"].as_str().is_some(), "task_id should be set");
    assert_eq!(file["ticket_id"], ticket_id, "ticket_id should match");

    // List task files (1) — covers list_by_task
    let res = client
        .get(format!("{base_url}/api/trouble/tasks/{task_id}/files"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let files: Vec<Value> = res.json().await.unwrap();
    assert_eq!(files.len(), 1);

    // Download task file
    let res = client
        .get(format!(
            "{base_url}/api/trouble/task-files/{file_id}/download"
        ))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body = res.bytes().await.unwrap();
    assert_eq!(&body[..], b"task file data");

    // Delete task file
    let res = client
        .delete(format!("{base_url}/api/trouble/task-files/{file_id}"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 204);

    // List task files (0 after delete)
    let res = client
        .get(format!("{base_url}/api/trouble/tasks/{task_id}/files"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let files: Vec<Value> = res.json().await.unwrap();
    assert_eq!(files.len(), 0);
}

// ============================================================
// Registration number search: half/full-width digits & hyphens
// ============================================================

#[tokio::test]
async fn test_trouble_ticket_search_registration_number_width() {
    test_group!("登録番号 全角/半角 検索");
    test_case!(
        "全角登録番号を半角で / 半角登録番号を全角で / ハイフン混在がヒットし、既存 description 検索が非回帰",
        {
            let state = common::setup_app_state().await;
            let base_url = common::spawn_test_server(state.clone()).await;
            let tenant_id =
                common::create_test_tenant(state.pool(), "Trouble Search Width Tenant").await;
            let jwt = common::create_test_jwt(tenant_id, "admin");
            let client = reqwest::Client::new();
            let auth = format!("Bearer {jwt}");

            // Setup workflow
            let res = client
                .post(format!("{base_url}/api/trouble/workflow/setup"))
                .header("Authorization", &auth)
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);

            // Create ticket with FULL-WIDTH registration number
            let res = client
                .post(format!("{base_url}/api/trouble/tickets"))
                .header("Authorization", &auth)
                .json(&serde_json::json!({
                    "category": "貨物事故",
                    "registration_number": "品川５００あ１２３４",
                    "description": "フェンダー凹み"
                }))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 201);
            let t_full: Value = res.json().await.unwrap();
            let id_full = t_full["id"].as_str().unwrap().to_string();

            // Create ticket with HALF-WIDTH registration number
            let res = client
                .post(format!("{base_url}/api/trouble/tickets"))
                .header("Authorization", &auth)
                .json(&serde_json::json!({
                    "category": "貨物事故",
                    "registration_number": "多摩300う5678",
                    "description": "バンパー傷"
                }))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 201);
            let t_half: Value = res.json().await.unwrap();
            let id_half = t_half["id"].as_str().unwrap().to_string();

            // Create ticket with FULL-WIDTH hyphen in registration number
            let res = client
                .post(format!("{base_url}/api/trouble/tickets"))
                .header("Authorization", &auth)
                .json(&serde_json::json!({
                    "category": "貨物事故",
                    "registration_number": "京都１２－３４",
                    "description": "ドア凹み"
                }))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 201);
            let t_hyphen: Value = res.json().await.unwrap();
            let id_hyphen = t_hyphen["id"].as_str().unwrap().to_string();

            let list_ids = |body: &Value| -> Vec<String> {
                body["tickets"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|t| t["id"].as_str().unwrap().to_string())
                    .collect()
            };

            // Case 1: full-width stored, half-width query → hits t_full
            let res = client
                .get(format!("{base_url}/api/trouble/tickets"))
                .header("Authorization", &auth)
                .query(&[("q", "1234")])
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
            let body: Value = res.json().await.unwrap();
            let ids = list_ids(&body);
            assert!(
                ids.contains(&id_full),
                "half-width query '1234' should match full-width stored '品川５００あ１２３４'"
            );

            // Case 2: half-width stored, full-width query → hits t_half
            let res = client
                .get(format!("{base_url}/api/trouble/tickets"))
                .header("Authorization", &auth)
                .query(&[("q", "５６７８")])
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
            let body: Value = res.json().await.unwrap();
            let ids = list_ids(&body);
            assert!(
                ids.contains(&id_half),
                "full-width query '５６７８' should match half-width stored '多摩300う5678'"
            );

            // Case 3: full-width hyphen stored, half-width hyphen query → hits t_hyphen
            let res = client
                .get(format!("{base_url}/api/trouble/tickets"))
                .header("Authorization", &auth)
                .query(&[("q", "12-34")])
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
            let body: Value = res.json().await.unwrap();
            let ids = list_ids(&body);
            assert!(
                ids.contains(&id_hyphen),
                "half-width hyphen query '12-34' should match full-width stored '京都１２－３４'"
            );

            // Case 4: regression — description still searchable
            let res = client
                .get(format!("{base_url}/api/trouble/tickets"))
                .header("Authorization", &auth)
                .query(&[("q", "バンパー")])
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
            let body: Value = res.json().await.unwrap();
            let ids = list_ids(&body);
            assert!(
                ids.contains(&id_half),
                "description search 'バンパー' should still hit t_half"
            );
        }
    );
}

// NOTE: 旧「tenant 実在チェック (require_tenant ミドルウェア)」セクション (4 テスト) は
// #434 で撤去した。rust-alc-api は JWT 検証 / bare X-Tenant-ID フォールバック / tenant
// 実在チェックを行わず、前段 proxy が auth-worker introspect で検証して注入する identity
// ヘッダー (X-Tenant-ID / X-User-*) を信頼するだけになった (require_tenant_header)。外部
// 直叩き防止は Cloud Run IAM 網層ロックダウンが担う (確定アーキ #4807535677)。よって
// 「未知 tenant → 401」「fail-open」を rust 層で検証していた 4 テストは意味を失い削除した。
