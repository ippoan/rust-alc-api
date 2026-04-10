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
                    "description": "テスト説明"
                }))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 201);
            let ticket: Value = res.json().await.unwrap();
            let ticket_id = ticket["id"].as_str().unwrap();
            assert_eq!(ticket["category"], "貨物事故");
            assert_eq!(ticket["person_name"], "テスト太郎");

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
                    "progress_notes": "対応中"
                }))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
            let updated: Value = res.json().await.unwrap();
            assert_eq!(updated["progress_notes"], "対応中");

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
// 4. Comments CRUD
// ============================================================

#[tokio::test]
async fn test_trouble_comments_crud() {
    test_group!("コメントCRUD");
    test_case!("コメント作成→一覧→削除", {
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;
        let tenant_id = common::create_test_tenant(state.pool(), "Trouble Comment Tenant").await;
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
                "category": "苦情・トラブル",
                "title": "コメントテスト"
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 201);
        let ticket: Value = res.json().await.unwrap();
        let ticket_id = ticket["id"].as_str().unwrap();

        // POST comment
        let res = client
            .post(format!(
                "{base_url}/api/trouble/tickets/{ticket_id}/comments"
            ))
            .header("Authorization", &auth)
            .json(&serde_json::json!({
                "body": "テストコメント"
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 201);
        let comment: Value = res.json().await.unwrap();
        let comment_id = comment["id"].as_str().unwrap();
        assert_eq!(comment["body"], "テストコメント");

        // GET comments
        let res = client
            .get(format!(
                "{base_url}/api/trouble/tickets/{ticket_id}/comments"
            ))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        let comments: Vec<Value> = res.json().await.unwrap();
        assert!(comments.len() >= 1);

        // DELETE comment
        let res = client
            .delete(format!("{base_url}/api/trouble/comments/{comment_id}"))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 204);
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
// 8. Empty comment body
// ============================================================

#[tokio::test]
async fn test_trouble_comment_empty_body() {
    test_group!("バリデーション");
    test_case!("空コメント→400", {
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;
        let tenant_id =
            common::create_test_tenant(state.pool(), "Trouble Empty Comment Tenant").await;
        let jwt = common::create_test_jwt(tenant_id, "admin");
        let client = reqwest::Client::new();
        let auth = format!("Bearer {jwt}");

        let ticket_id = uuid::Uuid::new_v4();
        let res = client
            .post(format!(
                "{base_url}/api/trouble/tickets/{ticket_id}/comments"
            ))
            .header("Authorization", &auth)
            .json(&serde_json::json!({
                "body": ""
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 400);
    });
}

// ============================================================
// 9. Workflow transition CRUD
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

    // Insert file metadata directly via DB
    let file_id = uuid::Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO trouble_files (id, tenant_id, ticket_id, filename, content_type, size_bytes, storage_key)
        VALUES ($1, $2, $3::uuid, 'test.pdf', 'application/pdf', 1024, 'test/key.pdf')"#,
    )
    .bind(file_id)
    .bind(tenant_id)
    .bind(uuid::Uuid::parse_str(ticket_id).unwrap())
    .execute(state.pool())
    .await
    .unwrap();

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

    // Get file (download → 503 no storage, but repo get() is covered)
    let res = client
        .get(format!("{base_url}/api/trouble/files/{file_id}/download"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert!(res.status() == 503 || res.status() == 404);

    // Delete file
    let res = client
        .delete(format!("{base_url}/api/trouble/files/{file_id}"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 204);
}
