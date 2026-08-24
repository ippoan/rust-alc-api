#[macro_use]
mod common;

use serde_json::Value;

/// Helper: setup workflow and create a ticket. Returns ticket_id as String.
async fn create_ticket(base_url: &str, auth: &str, category: &str, person: &str) -> String {
    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/trouble/tickets"))
        .header("Authorization", auth)
        .json(&serde_json::json!({
            "category": category,
            "person_name": person,
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 201, "ticket create should return 201");
    let ticket: Value = res.json().await.unwrap();
    ticket["id"].as_str().unwrap().to_string()
}

async fn create_task(
    base_url: &str,
    auth: &str,
    ticket_id: &str,
    body: serde_json::Value,
) -> Value {
    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/trouble/tickets/{ticket_id}/tasks"))
        .header("Authorization", auth)
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 201, "task create should return 201");
    res.json().await.unwrap()
}

#[tokio::test]
async fn test_list_all_tasks_cross_ticket() {
    test_group!("trouble tasks 横断一覧");
    test_case!(
        "2チケット×3タスクで全件取得・フィルタ・ページング・ソートを検証",
        {
            let state = common::setup_app_state().await;
            let base_url = common::spawn_test_server(state.clone()).await;
            let tenant_id =
                common::create_test_tenant(state.pool(), "Tasks Cross Ticket Tenant").await;
            let jwt = common::create_test_jwt(tenant_id, "admin");
            let client = reqwest::Client::new();
            let auth = format!("Bearer {jwt}");

            // Setup workflow (to allow ticket create with valid initial status)
            let res = client
                .post(format!("{base_url}/api/trouble/workflow/setup"))
                .header("Authorization", &auth)
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);

            // Create two tickets
            let ticket_a = create_ticket(&base_url, &auth, "貨物事故", "ドライバーA").await;
            let ticket_b = create_ticket(&base_url, &auth, "その他", "ドライバーB").await;

            // Create 3 tasks per ticket (open, in_progress [via create + update], done [create +
            // update to set status]). We'll just create as "open" and update 2 of them later.
            let t_a1 = create_task(
                &base_url,
                &auth,
                &ticket_a,
                serde_json::json!({"title": "修理手配 alpha", "task_type": "修理手配"}),
            )
            .await;
            let t_a2 = create_task(
                &base_url,
                &auth,
                &ticket_a,
                serde_json::json!({"title": "連絡 beta", "task_type": "連絡"}),
            )
            .await;
            let _t_a3 = create_task(
                &base_url,
                &auth,
                &ticket_a,
                serde_json::json!({"title": "報告 gamma", "task_type": "報告"}),
            )
            .await;
            let _t_b1 = create_task(
                &base_url,
                &auth,
                &ticket_b,
                serde_json::json!({"title": "修理手配 delta", "task_type": "修理手配"}),
            )
            .await;
            let _t_b2 = create_task(
                &base_url,
                &auth,
                &ticket_b,
                serde_json::json!({"title": "連絡 epsilon", "task_type": "連絡"}),
            )
            .await;
            let _t_b3 = create_task(
                &base_url,
                &auth,
                &ticket_b,
                serde_json::json!({"title": "報告 zeta", "task_type": "報告"}),
            )
            .await;

            // Update t_a1 → in_progress, t_a2 → done
            let t_a1_id = t_a1["id"].as_str().unwrap();
            let t_a2_id = t_a2["id"].as_str().unwrap();
            let res = client
                .put(format!("{base_url}/api/trouble/tasks/{t_a1_id}"))
                .header("Authorization", &auth)
                .json(&serde_json::json!({"status": "in_progress"}))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
            let res = client
                .put(format!("{base_url}/api/trouble/tasks/{t_a2_id}"))
                .header("Authorization", &auth)
                .json(&serde_json::json!({"status": "done"}))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);

            // Default list: all 6 returned, sorted created_at desc by default.
            let res = client
                .get(format!("{base_url}/api/trouble/tasks"))
                .header("Authorization", &auth)
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
            let body: Value = res.json().await.unwrap();
            assert_eq!(body["total"].as_i64().unwrap(), 6, "total should be 6");
            assert_eq!(
                body["items"].as_array().unwrap().len(),
                6,
                "items should have 6 tasks"
            );
            assert_eq!(body["page"].as_i64().unwrap(), 1);
            assert_eq!(body["per_page"].as_i64().unwrap(), 50);

            // Filter by status=open → 4 tasks (6 - 1 in_progress - 1 done)
            let res = client
                .get(format!("{base_url}/api/trouble/tasks?status=open"))
                .header("Authorization", &auth)
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
            let body: Value = res.json().await.unwrap();
            assert_eq!(body["total"].as_i64().unwrap(), 4);
            for item in body["items"].as_array().unwrap() {
                assert_eq!(item["status"], "open");
            }

            // Filter by ticket_id → 3 tasks
            let res = client
                .get(format!("{base_url}/api/trouble/tasks?ticket_id={ticket_a}"))
                .header("Authorization", &auth)
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
            let body: Value = res.json().await.unwrap();
            assert_eq!(body["total"].as_i64().unwrap(), 3);
            for item in body["items"].as_array().unwrap() {
                assert_eq!(item["ticket_id"], ticket_a);
            }

            // Filter by q (title ILIKE) → "beta" matches only t_a2
            let res = client
                .get(format!("{base_url}/api/trouble/tasks?q=beta"))
                .header("Authorization", &auth)
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
            let body: Value = res.json().await.unwrap();
            assert_eq!(body["total"].as_i64().unwrap(), 1);
            assert!(body["items"][0]["title"].as_str().unwrap().contains("beta"));

            // Pagination: per_page=2, page=2 → 2 items, total still 6
            let res = client
                .get(format!("{base_url}/api/trouble/tasks?per_page=2&page=2"))
                .header("Authorization", &auth)
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
            let body: Value = res.json().await.unwrap();
            assert_eq!(body["total"].as_i64().unwrap(), 6);
            assert_eq!(body["items"].as_array().unwrap().len(), 2);
            assert_eq!(body["page"].as_i64().unwrap(), 2);
            assert_eq!(body["per_page"].as_i64().unwrap(), 2);

            // Unknown sort_by → 400
            let res = client
                .get(format!("{base_url}/api/trouble/tasks?sort_by=bogus_column"))
                .header("Authorization", &auth)
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 400);

            // Sort by status (ASC) → first items should be "done" (d < i < o alphabetically)
            let res = client
                .get(format!(
                    "{base_url}/api/trouble/tasks?sort_by=status&sort_desc=false"
                ))
                .header("Authorization", &auth)
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
            let body: Value = res.json().await.unwrap();
            let statuses: Vec<&str> = body["items"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v["status"].as_str().unwrap())
                .collect();
            assert_eq!(statuses.first().copied(), Some("done"));

            // Missing auth → 401
            let res = reqwest::Client::new()
                .get(format!("{base_url}/api/trouble/tasks"))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 401);
        }
    );
}

/// 経過記録 (trouble_tasks) の並び替え。全行 sort_order=0 の既存データでも
/// 1 回で効くこと / 不正な id を弾くことを検証する (Refs ippoan/nuxt-trouble#240)。
#[tokio::test]
async fn test_reorder_tasks() {
    test_group!("trouble tasks 並び替え");
    test_case!(
        "task_ids の順で 0 起点に採番され、リロード後も並びが保たれる",
        {
            let state = common::setup_app_state().await;
            let base_url = common::spawn_test_server(state.clone()).await;
            let tenant_id = common::create_test_tenant(state.pool(), "Tasks Reorder Tenant").await;
            let jwt = common::create_test_jwt(tenant_id, "admin");
            let client = reqwest::Client::new();
            let auth = format!("Bearer {jwt}");

            let res = client
                .post(format!("{base_url}/api/trouble/workflow/setup"))
                .header("Authorization", &auth)
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);

            let ticket = create_ticket(&base_url, &auth, "貨物事故", "ドライバーA").await;
            let other_ticket = create_ticket(&base_url, &auth, "その他", "ドライバーB").await;

            let t1 = create_task(
                &base_url,
                &auth,
                &ticket,
                serde_json::json!({"title": "1件目", "task_type": "連絡"}),
            )
            .await;
            let t2 = create_task(
                &base_url,
                &auth,
                &ticket,
                serde_json::json!({"title": "2件目", "task_type": "連絡"}),
            )
            .await;
            let t3 = create_task(
                &base_url,
                &auth,
                &ticket,
                serde_json::json!({"title": "3件目", "task_type": "報告"}),
            )
            .await;
            let foreign = create_task(
                &base_url,
                &auth,
                &other_ticket,
                serde_json::json!({"title": "別チケット", "task_type": "連絡"}),
            )
            .await;

            let (id1, id2, id3) = (
                t1["id"].as_str().unwrap().to_string(),
                t2["id"].as_str().unwrap().to_string(),
                t3["id"].as_str().unwrap().to_string(),
            );

            // sort_order 未指定の作成は末尾採番 (0,1,2) になる
            assert_eq!(t1["sort_order"].as_i64(), Some(0));
            assert_eq!(t2["sort_order"].as_i64(), Some(1));
            assert_eq!(t3["sort_order"].as_i64(), Some(2));

            let reorder_url = format!("{base_url}/api/trouble/tickets/{ticket}/tasks/reorder");

            // 3,1,2 の順に並べ替え
            let res = client
                .put(&reorder_url)
                .header("Authorization", &auth)
                .json(&serde_json::json!({"task_ids": [id3, id1, id2]}))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
            let body: Value = res.json().await.unwrap();
            let titles: Vec<&str> = body
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v["title"].as_str().unwrap())
                .collect();
            assert_eq!(titles, vec!["3件目", "1件目", "2件目"]);
            let orders: Vec<i64> = body
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v["sort_order"].as_i64().unwrap())
                .collect();
            assert_eq!(orders, vec![0, 1, 2]);

            // 再取得しても並びが保たれる
            let res = client
                .get(format!("{base_url}/api/trouble/tickets/{ticket}/tasks"))
                .header("Authorization", &auth)
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
            let body: Value = res.json().await.unwrap();
            let titles: Vec<&str> = body
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v["title"].as_str().unwrap())
                .collect();
            assert_eq!(titles, vec!["3件目", "1件目", "2件目"]);

            // 新規追加は末尾に付く
            let t4 = create_task(
                &base_url,
                &auth,
                &ticket,
                serde_json::json!({"title": "4件目", "task_type": "その他"}),
            )
            .await;
            assert_eq!(t4["sort_order"].as_i64(), Some(3));

            // 空配列 → 400
            let res = client
                .put(&reorder_url)
                .header("Authorization", &auth)
                .json(&serde_json::json!({"task_ids": []}))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 400);

            // 重複 id → 400
            let res = client
                .put(&reorder_url)
                .header("Authorization", &auth)
                .json(&serde_json::json!({"task_ids": [id1, id1]}))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 400);

            // 別チケットの task_id が混ざる → 404 かつ既存の並びは変わらない
            let foreign_id = foreign["id"].as_str().unwrap().to_string();
            let res = client
                .put(&reorder_url)
                .header("Authorization", &auth)
                .json(&serde_json::json!({"task_ids": [id1, id2, foreign_id]}))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 404);

            let res = client
                .get(format!("{base_url}/api/trouble/tickets/{ticket}/tasks"))
                .header("Authorization", &auth)
                .send()
                .await
                .unwrap();
            let body: Value = res.json().await.unwrap();
            let titles: Vec<&str> = body
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v["title"].as_str().unwrap())
                .collect();
            assert_eq!(titles, vec!["3件目", "1件目", "2件目", "4件目"]);

            // 認証なし → 401
            let res = reqwest::Client::new()
                .put(&reorder_url)
                .json(&serde_json::json!({"task_ids": [id1]}))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 401);
        }
    );
}
