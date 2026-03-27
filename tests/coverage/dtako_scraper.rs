use serde_json::Value;
use tokio::io::AsyncWriteExt;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::common;

// ============================================================
// get_scrape_history — 正常系 + 空結果
// ============================================================

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_get_scrape_history_empty() {
    test_group!("dtako_scraper カバレッジ");
    test_case!("履歴が空 → 空配列を返す", {
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;
        let tenant_id = common::create_test_tenant(&state.pool, "Scraper Hist").await;
        let jwt = common::create_test_jwt(tenant_id, "admin");

        let client = reqwest::Client::new();
        let res = client
            .get(format!("{base_url}/api/scraper/history"))
            .header("Authorization", format!("Bearer {jwt}"))
            .send()
            .await
            .unwrap();

        assert_eq!(res.status(), 200);
        let body: Vec<Value> = res.json().await.unwrap();
        assert!(body.is_empty(), "Should return empty array");
    });
}

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_get_scrape_history_with_data() {
    test_group!("dtako_scraper カバレッジ");
    test_case!("履歴あり → limit/offset 対応", {
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;
        let tenant_id = common::create_test_tenant(&state.pool, "Scraper Hist2").await;
        let jwt = common::create_test_jwt(tenant_id, "admin");

        // テストデータ挿入
        sqlx::query(
            r#"INSERT INTO dtako_scrape_history (tenant_id, target_date, comp_id, status, message)
               VALUES ($1, '2026-03-01', 'COMP001', 'success', 'test message')"#,
        )
        .bind(tenant_id)
        .execute(&state.pool)
        .await
        .unwrap();

        let client = reqwest::Client::new();
        let res = client
            .get(format!("{base_url}/api/scraper/history?limit=10&offset=0"))
            .header("Authorization", format!("Bearer {jwt}"))
            .send()
            .await
            .unwrap();

        assert_eq!(res.status(), 200);
        let body: Vec<Value> = res.json().await.unwrap();
        assert_eq!(body.len(), 1);
        assert_eq!(body[0]["comp_id"], "COMP001");
        assert_eq!(body[0]["status"], "success");
    });
}

// ============================================================
// trigger_scrape — scraper 接続エラー → 502
// ============================================================

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_trigger_scrape_connection_error() {
    test_group!("dtako_scraper カバレッジ");
    test_case!("scraper 接続不可 → 502", {
        // 存在しないポートを指定
        std::env::set_var("TEST_SCRAPER_URL", "http://127.0.0.1:19999");

        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;
        let tenant_id = common::create_test_tenant(&state.pool, "Scraper Err").await;
        let jwt = common::create_test_jwt(tenant_id, "admin");

        let client = reqwest::Client::new();
        let res = client
            .post(format!("{base_url}/api/scraper/trigger"))
            .header("Authorization", format!("Bearer {jwt}"))
            .json(&serde_json::json!({}))
            .send()
            .await
            .unwrap();

        assert_eq!(res.status(), 502, "Connection error → 502");

        std::env::remove_var("TEST_SCRAPER_URL");
    });
}

// ============================================================
// trigger_scrape — scraper が非200を返す → 502
// ============================================================

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_trigger_scrape_scraper_error_response() {
    test_group!("dtako_scraper カバレッジ");
    test_case!("scraper 500 レスポンス → 502", {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/scrape"))
            .respond_with(
                ResponseTemplate::new(500).set_body_string("Internal Server Error from scraper"),
            )
            .mount(&mock_server)
            .await;

        std::env::set_var("TEST_SCRAPER_URL", mock_server.uri());

        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;
        let tenant_id = common::create_test_tenant(&state.pool, "Scraper 500").await;
        let jwt = common::create_test_jwt(tenant_id, "admin");

        let client = reqwest::Client::new();
        let res = client
            .post(format!("{base_url}/api/scraper/trigger"))
            .header("Authorization", format!("Bearer {jwt}"))
            .json(&serde_json::json!({
                "start_date": "2026-03-01",
                "end_date": "2026-03-01"
            }))
            .send()
            .await
            .unwrap();

        assert_eq!(res.status(), 502, "Scraper error → 502");

        std::env::remove_var("TEST_SCRAPER_URL");
    });
}

// ============================================================
// trigger_scrape — SSE ハッピーパス (wiremock で SSE レスポンス)
// ============================================================

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_trigger_scrape_sse_happy_path() {
    test_group!("dtako_scraper カバレッジ");
    test_case!("scraper SSE レスポンスをプロキシ + DB 保存", {
        let mock_server = MockServer::start().await;

        // SSE レスポンスを返すモック
        let sse_body = [
                "data:{\"event\":\"result\",\"comp_id\":\"C001\",\"status\":\"success\",\"message\":\"OK\"}\n\n",
                "data:{\"event\":\"result\",\"comp_id\":\"C002\",\"status\":\"error\",\"message\":\"Failed\"}\n\n",
                "data:{\"event\":\"progress\",\"message\":\"50%\"}\n\n",
                "data:{\"event\":\"result\",\"comp_id\":\"C003\",\"status\":\"success\"}\n\n",
            ]
            .concat();

        Mock::given(method("POST"))
            .and(path("/scrape"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(sse_body),
            )
            .mount(&mock_server)
            .await;

        std::env::set_var("TEST_SCRAPER_URL", mock_server.uri());

        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;
        let tenant_id = common::create_test_tenant(&state.pool, "Scraper SSE").await;
        let jwt = common::create_test_jwt(tenant_id, "admin");

        let client = reqwest::Client::new();
        let res = client
            .post(format!("{base_url}/api/scraper/trigger"))
            .header("Authorization", format!("Bearer {jwt}"))
            .json(&serde_json::json!({
                "start_date": "2026-03-15",
                "end_date": "2026-03-15",
                "comp_id": "C001"
            }))
            .send()
            .await
            .unwrap();

        assert_eq!(res.status(), 200, "SSE proxy should return 200");

        // SSE レスポンスのボディを読み取る
        let body = res.text().await.unwrap();
        assert!(
            body.contains("C001") || body.contains("success"),
            "SSE body should contain relayed events, got: {body}"
        );

        // DB に保存されたか確認 (非同期処理のため少し待つ)
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        let count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM dtako_scrape_history WHERE tenant_id = $1")
                .bind(tenant_id)
                .fetch_one(&state.pool)
                .await
                .unwrap();
        assert!(
            count.0 >= 1,
            "Should have saved at least 1 history record, got {}",
            count.0
        );

        std::env::remove_var("TEST_SCRAPER_URL");
    });
}

// ============================================================
// trigger_scrape — start_date なし (デフォルト昨日) + skip_upload
// ============================================================

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_trigger_scrape_default_date() {
    test_group!("dtako_scraper カバレッジ");
    test_case!(
        "start_date なし → デフォルト昨日 + skip_upload",
        {
            let mock_server = MockServer::start().await;

            let sse_body =
                "data:{\"event\":\"result\",\"comp_id\":\"D001\",\"status\":\"success\",\"message\":\"done\"}\n\n";

            Mock::given(method("POST"))
                .and(path("/scrape"))
                .respond_with(
                    ResponseTemplate::new(200)
                        .insert_header("content-type", "text/event-stream")
                        .set_body_string(sse_body),
                )
                .mount(&mock_server)
                .await;

            std::env::set_var("TEST_SCRAPER_URL", mock_server.uri());

            let state = common::setup_app_state().await;
            let base_url = common::spawn_test_server(state.clone()).await;
            let tenant_id = common::create_test_tenant(&state.pool, "Scraper Def").await;
            let jwt = common::create_test_jwt(tenant_id, "admin");

            let client = reqwest::Client::new();
            let res = client
                .post(format!("{base_url}/api/scraper/trigger"))
                .header("Authorization", format!("Bearer {jwt}"))
                .json(&serde_json::json!({ "skip_upload": true }))
                .send()
                .await
                .unwrap();

            assert_eq!(res.status(), 200);
            let _body = res.text().await.unwrap();

            std::env::remove_var("TEST_SCRAPER_URL");
        }
    );
}

// ============================================================
// trigger_scrape — 不正な start_date (パース失敗 → fallback)
// ============================================================

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_trigger_scrape_invalid_date() {
    test_group!("dtako_scraper カバレッジ");
    test_case!(
        "不正な start_date → パース失敗 → 今日にフォールバック",
        {
            let mock_server = MockServer::start().await;

            let sse_body = "data:{\"event\":\"done\"}\n\n";

            Mock::given(method("POST"))
                .and(path("/scrape"))
                .respond_with(
                    ResponseTemplate::new(200)
                        .insert_header("content-type", "text/event-stream")
                        .set_body_string(sse_body),
                )
                .mount(&mock_server)
                .await;

            std::env::set_var("TEST_SCRAPER_URL", mock_server.uri());

            let state = common::setup_app_state().await;
            let base_url = common::spawn_test_server(state.clone()).await;
            let tenant_id = common::create_test_tenant(&state.pool, "Scraper BadDate").await;
            let jwt = common::create_test_jwt(tenant_id, "admin");

            let client = reqwest::Client::new();
            let res = client
                .post(format!("{base_url}/api/scraper/trigger"))
                .header("Authorization", format!("Bearer {jwt}"))
                .json(&serde_json::json!({ "start_date": "not-a-date" }))
                .send()
                .await
                .unwrap();

            assert_eq!(res.status(), 200);
            let _body = res.text().await.unwrap();

            std::env::remove_var("TEST_SCRAPER_URL");
        }
    );
}

// ============================================================
// get_id_token — メタデータサーバー成功
// ============================================================

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_trigger_scrape_with_id_token() {
    test_group!("dtako_scraper カバレッジ");
    test_case!(
        "メタデータサーバーから ID トークン取得 → Bearer 付与",
        {
            let metadata_server = MockServer::start().await;
            let scraper_server = MockServer::start().await;

            // メタデータサーバーモック
            Mock::given(method("GET"))
                .respond_with(ResponseTemplate::new(200).set_body_string("mock-id-token-12345"))
                .mount(&metadata_server)
                .await;

            // Scraper モック
            Mock::given(method("POST"))
                .and(path("/scrape"))
                .respond_with(
                    ResponseTemplate::new(200)
                        .insert_header("content-type", "text/event-stream")
                        .set_body_string("data:{\"event\":\"done\"}\n\n"),
                )
                .mount(&scraper_server)
                .await;

            std::env::set_var("GCP_METADATA_URL", metadata_server.uri());
            std::env::set_var("TEST_SCRAPER_URL", scraper_server.uri());

            let state = common::setup_app_state().await;
            let base_url = common::spawn_test_server(state.clone()).await;
            let tenant_id = common::create_test_tenant(&state.pool, "Scraper Token").await;
            let jwt = common::create_test_jwt(tenant_id, "admin");

            let client = reqwest::Client::new();
            let res = client
                .post(format!("{base_url}/api/scraper/trigger"))
                .header("Authorization", format!("Bearer {jwt}"))
                .json(&serde_json::json!({ "start_date": "2026-03-15" }))
                .send()
                .await
                .unwrap();

            assert_eq!(res.status(), 200);
            let _body = res.text().await.unwrap();

            std::env::remove_var("GCP_METADATA_URL");
            std::env::remove_var("TEST_SCRAPER_URL");
        }
    );
}

// ============================================================
// get_id_token — メタデータサーバー非200
// ============================================================

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_trigger_scrape_metadata_error() {
    test_group!("dtako_scraper カバレッジ");
    test_case!(
        "メタデータサーバー 403 → トークンなしで続行",
        {
            let metadata_server = MockServer::start().await;
            let scraper_server = MockServer::start().await;

            Mock::given(method("GET"))
                .respond_with(ResponseTemplate::new(403).set_body_string("Forbidden"))
                .mount(&metadata_server)
                .await;

            Mock::given(method("POST"))
                .and(path("/scrape"))
                .respond_with(
                    ResponseTemplate::new(200)
                        .insert_header("content-type", "text/event-stream")
                        .set_body_string("data:{\"event\":\"done\"}\n\n"),
                )
                .mount(&scraper_server)
                .await;

            std::env::set_var("GCP_METADATA_URL", metadata_server.uri());
            std::env::set_var("TEST_SCRAPER_URL", scraper_server.uri());

            let state = common::setup_app_state().await;
            let base_url = common::spawn_test_server(state.clone()).await;
            let tenant_id = common::create_test_tenant(&state.pool, "Scraper MetaErr").await;
            let jwt = common::create_test_jwt(tenant_id, "admin");

            let client = reqwest::Client::new();
            let res = client
                .post(format!("{base_url}/api/scraper/trigger"))
                .header("Authorization", format!("Bearer {jwt}"))
                .json(&serde_json::json!({ "start_date": "2026-03-15" }))
                .send()
                .await
                .unwrap();

            assert_eq!(res.status(), 200, "Should still succeed without token");
            let _body = res.text().await.unwrap();

            std::env::remove_var("GCP_METADATA_URL");
            std::env::remove_var("TEST_SCRAPER_URL");
        }
    );
}

// ============================================================
// trigger_scrape — SSE stream with non-result events + empty data
// ============================================================

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_trigger_scrape_sse_edge_cases() {
    test_group!("dtako_scraper カバレッジ");
    test_case!(
        "SSE: 空データ行 + non-data 行 + comp_id なし result",
        {
            let mock_server = MockServer::start().await;

            let sse_body = [
                // 非 data: 行 (スキップされる)
                "event: ping\n\n",
                // 空 data (スキップされる)
                "data:\n\n",
                // result だが comp_id なし (DB 保存スキップ)
                "data:{\"event\":\"result\",\"status\":\"success\"}\n\n",
                // 不正 JSON (パース失敗 → スキップ)
                "data:not-json\n\n",
                // 正常な result
                "data:{\"event\":\"result\",\"comp_id\":\"E001\",\"status\":\"success\"}\n\n",
            ]
            .concat();

            Mock::given(method("POST"))
                .and(path("/scrape"))
                .respond_with(
                    ResponseTemplate::new(200)
                        .insert_header("content-type", "text/event-stream")
                        .set_body_string(sse_body),
                )
                .mount(&mock_server)
                .await;

            std::env::set_var("TEST_SCRAPER_URL", mock_server.uri());

            let state = common::setup_app_state().await;
            let base_url = common::spawn_test_server(state.clone()).await;
            let tenant_id = common::create_test_tenant(&state.pool, "Scraper Edge").await;
            let jwt = common::create_test_jwt(tenant_id, "admin");

            let client = reqwest::Client::new();
            let res = client
                .post(format!("{base_url}/api/scraper/trigger"))
                .header("Authorization", format!("Bearer {jwt}"))
                .json(&serde_json::json!({ "start_date": "2026-03-20" }))
                .send()
                .await
                .unwrap();

            assert_eq!(res.status(), 200);
            let _body = res.text().await.unwrap();

            std::env::remove_var("TEST_SCRAPER_URL");
        }
    );
}

// ============================================================
// get_scrape_history — DB error (lines 237-241)
// ============================================================

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_get_scrape_history_db_error() {
    test_group!("dtako_scraper カバレッジ");
    test_case!(
        "dtako_scrape_history テーブル RENAME → DB エラー → 500",
        {
            let state = common::setup_app_state().await;
            let base_url = common::spawn_test_server(state.clone()).await;
            let tenant_id = common::create_test_tenant(&state.pool, "Scraper DB Err").await;
            let jwt = common::create_test_jwt(tenant_id, "admin");

            // テーブルを RENAME (コミット済み)
            sqlx::query("ALTER TABLE alc_api.dtako_scrape_history RENAME TO _scrape_hist_bak")
                .execute(&state.pool)
                .await
                .unwrap();

            let client = reqwest::Client::new();
            let res = client
                .get(format!("{base_url}/api/scraper/history"))
                .header("Authorization", format!("Bearer {jwt}"))
                .send()
                .await
                .unwrap();

            assert_eq!(res.status(), 500, "DB error → 500");

            // 復元
            sqlx::query("ALTER TABLE alc_api._scrape_hist_bak RENAME TO dtako_scrape_history")
                .execute(&state.pool)
                .await
                .unwrap();
        }
    );
}

// ============================================================
// trigger_scrape — SSE client disconnect (lines 196-197)
// ============================================================

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_trigger_scrape_client_disconnect() {
    test_group!("dtako_scraper カバレッジ");
    test_case!("SSE ストリーム中にクライアント切断", {
        let mock_server = MockServer::start().await;

        // 大量のイベントを返すモック
        let mut sse_body = String::new();
        for i in 0..100 {
            sse_body.push_str(&format!(
                    "data:{{\"event\":\"result\",\"comp_id\":\"DISC{i:03}\",\"status\":\"success\"}}\n\n"
                ));
        }

        Mock::given(method("POST"))
            .and(path("/scrape"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(sse_body),
            )
            .mount(&mock_server)
            .await;

        std::env::set_var("TEST_SCRAPER_URL", mock_server.uri());

        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;
        let tenant_id = common::create_test_tenant(&state.pool, "Scraper Disc").await;
        let jwt = common::create_test_jwt(tenant_id, "admin");

        // リクエスト送信後、レスポンスを読まずに drop → tx.send() 失敗
        let client = reqwest::Client::new();
        let res = client
            .post(format!("{base_url}/api/scraper/trigger"))
            .header("Authorization", format!("Bearer {jwt}"))
            .json(&serde_json::json!({ "start_date": "2026-03-20" }))
            .send()
            .await
            .unwrap();

        assert_eq!(res.status(), 200);
        drop(res);

        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        std::env::remove_var("TEST_SCRAPER_URL");
    });
}

// ============================================================
// trigger_scrape — SSE stream error (lines 161-163)
// 不正な chunked encoding で bytes_stream Err を発生させる
// ============================================================

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_trigger_scrape_stream_error() {
    test_group!("dtako_scraper カバレッジ");
    test_case!("不正な chunked encoding → bytes_stream Err", {
        // 生 TCP サーバーで不正な chunked レスポンスを返す
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            // POST /scrape を受け付ける
            if let Ok((mut stream, _)) = listener.accept().await {
                // リクエストを読み捨て
                let mut buf = [0u8; 4096];
                let _ = tokio::io::AsyncReadExt::read(&mut stream, &mut buf).await;

                // chunked transfer encoding で応答開始
                let header = "HTTP/1.1 200 OK\r\n\
                                  Content-Type: text/event-stream\r\n\
                                  Transfer-Encoding: chunked\r\n\r\n";
                let _ = stream.write_all(header.as_bytes()).await;

                // 正常なチャンクを1つ送信
                let chunk_data =
                    "data:{\"event\":\"result\",\"comp_id\":\"S001\",\"status\":\"success\"}\n\n";
                let chunk = format!("{:x}\r\n{}\r\n", chunk_data.len(), chunk_data);
                let _ = stream.write_all(chunk.as_bytes()).await;
                let _ = stream.flush().await;

                // 少し待ってから不正なチャンクを送信して接続を切断
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                // 不正な chunked data (サイズと実際のデータが不一致)
                let _ = stream.write_all(b"FFFFFF\r\n").await;
                let _ = stream.flush().await;
                // 接続を即座に切断
                drop(stream);
            }
        });

        std::env::set_var("TEST_SCRAPER_URL", format!("http://{addr}"));

        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;
        let tenant_id = common::create_test_tenant(&state.pool, "Scraper StreamErr").await;
        let jwt = common::create_test_jwt(tenant_id, "admin");

        let client = reqwest::Client::new();
        let res = client
            .post(format!("{base_url}/api/scraper/trigger"))
            .header("Authorization", format!("Bearer {jwt}"))
            .json(&serde_json::json!({ "start_date": "2026-03-20" }))
            .send()
            .await
            .unwrap();

        assert_eq!(res.status(), 200);
        // SSE ストリームを読み取る（途中でエラーになるが SSE は 200 で開始済み）
        let _body = res.text().await.unwrap_or_default();

        // 非同期タスクの完了を待つ
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;

        std::env::remove_var("TEST_SCRAPER_URL");
    });
}
