/// dtako_csv_proxy: match アーム (ferry/tolls/speed) + エラーパス + フォールバック
use serde_json::Value;

/// ZIP アップロード → MockStorage に CSV 配置 → csv_type でアクセスするヘルパー
async fn csv_proxy_setup_and_get(
    csv_type: &str,
    filename: &str,
    tenant_suffix: &str,
) -> reqwest::StatusCode {
    let state = crate::common::setup_app_state().await;
    let base_url = crate::common::spawn_test_server(state.clone()).await;
    let tenant_id = crate::common::create_test_tenant(state.pool(), tenant_suffix).await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    let zip_bytes = crate::common::create_test_dtako_zip();
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
    assert_eq!(res.status(), 200);

    let csv = "h0,h1,h2\nval0,val1,val2\n";
    let key = format!("{}/unko/1001/{}", tenant_id, filename);
    let (csv_bytes, _, _) = encoding_rs::SHIFT_JIS.encode(csv);
    state
        .dtako_storage
        .as_ref()
        .unwrap()
        .upload(&key, &csv_bytes, "text/csv")
        .await
        .unwrap();

    let res = client
        .get(format!("{base_url}/api/operations/1001/csv/{csv_type}"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    res.status()
}

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_ferry() {
    test_group!("デタコCSVプロキシ (カバレッジ)");
    test_case!("ferry CSVをJSON形式で取得できる", {
        let _db = crate::common::DB_RENAME_LOCK.lock().unwrap();
        let _flock = crate::common::db_rename_flock();
        let status = csv_proxy_setup_and_get("ferry", "KUDGFRY.csv", "CsvFerry").await;
        assert_eq!(status, 200);
    });
}

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_tolls() {
    test_group!("デタコCSVプロキシ (カバレッジ)");
    test_case!("tolls CSVをJSON形式で取得できる", {
        let _db = crate::common::DB_RENAME_LOCK.lock().unwrap();
        let _flock = crate::common::db_rename_flock();
        let status = csv_proxy_setup_and_get("tolls", "KUDGSIR.csv", "CsvTolls").await;
        assert_eq!(status, 200);
    });
}

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_speed() {
    test_group!("デタコCSVプロキシ (カバレッジ)");
    test_case!("speed CSVをJSON形式で取得できる", {
        let _db = crate::common::DB_RENAME_LOCK.lock().unwrap();
        let _flock = crate::common::db_rename_flock();
        let status = csv_proxy_setup_and_get("speed", "SOKUDODATA.csv", "CsvSpeed").await;
        assert_eq!(status, 200);
    });
}

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_invalid_type() {
    test_group!("デタコCSVプロキシ (カバレッジ)");
    test_case!("不正なCSVタイプで400を返す", {
        let _db = crate::common::DB_RENAME_LOCK.lock().unwrap();
        let _flock = crate::common::db_rename_flock();
        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state.clone()).await;
        let tenant_id = crate::common::create_test_tenant(state.pool(), "CsvBad").await;
        let jwt = crate::common::create_test_jwt(tenant_id, "admin");

        let res = reqwest::Client::new()
            .get(format!("{base_url}/api/operations/1001/csv/unknown_type"))
            .header("Authorization", format!("Bearer {jwt}"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 400);
    });
}

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_not_found() {
    test_group!("デタコCSVプロキシ (カバレッジ)");
    test_case!("存在しないCSVで404を返す", {
        let _db = crate::common::DB_RENAME_LOCK.lock().unwrap();
        let _flock = crate::common::db_rename_flock();
        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state.clone()).await;
        let tenant_id = crate::common::create_test_tenant(state.pool(), "CsvNF").await;
        let jwt = crate::common::create_test_jwt(tenant_id, "admin");
        let auth = format!("Bearer {jwt}");
        let client = reqwest::Client::new();

        let zip_bytes = crate::common::create_test_dtako_zip();
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
        assert_eq!(res.status(), 200);

        let res = client
            .get(format!("{base_url}/api/operations/1001/csv/ferry"))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 404);
    });
}

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_no_operation_record() {
    test_group!("デタコCSVプロキシ (カバレッジ)");
    test_case!(
        "操作レコードなしでフォールバックキーを使う",
        {
            let _db = crate::common::DB_RENAME_LOCK.lock().unwrap();
            let _flock = crate::common::db_rename_flock();
            let state = crate::common::setup_app_state().await;
            let base_url = crate::common::spawn_test_server(state.clone()).await;
            let tenant_id = crate::common::create_test_tenant(state.pool(), "CsvNoOp").await;
            let jwt = crate::common::create_test_jwt(tenant_id, "admin");
            let auth = format!("Bearer {jwt}");

            let csv = "col1,col2\nval1,val2\n";
            let key = format!("{}/unko/9999/KUDGURI.csv", tenant_id);
            let (csv_bytes, _, _) = encoding_rs::SHIFT_JIS.encode(csv);
            state
                .dtako_storage
                .as_ref()
                .unwrap()
                .upload(&key, &csv_bytes, "text/csv")
                .await
                .unwrap();

            let res = reqwest::Client::new()
                .get(format!("{base_url}/api/operations/9999/csv/kudguri"))
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
