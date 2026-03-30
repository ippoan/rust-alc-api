use serde_json::Value;

// ============================================================
// carins_files: GET エンドポイント
// ============================================================

#[tokio::test]
async fn test_list_files() {
    test_group!("車検証ファイル一覧");
    test_case!("GET /api/files でファイル一覧を取得する", {
        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state.clone()).await;
        let tenant_id = crate::common::create_test_tenant(state.pool(), "CarinsFiles").await;
        let jwt = crate::common::create_test_jwt(tenant_id, "admin");
        let client = reqwest::Client::new();

        let res = client
            .get(format!("{base_url}/api/files"))
            .header("Authorization", format!("Bearer {jwt}"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        let body: Value = res.json().await.unwrap();
        assert!(body["files"].is_array());
    });
}

#[tokio::test]
async fn test_list_files_with_type_filter() {
    test_group!("車検証ファイル一覧");
    test_case!(
        "type フィルタ付きでファイル一覧を取得する",
        {
            let state = crate::common::setup_app_state().await;
            let base_url = crate::common::spawn_test_server(state.clone()).await;
            let tenant_id = crate::common::create_test_tenant(state.pool(), "CarinsType").await;
            let jwt = crate::common::create_test_jwt(tenant_id, "admin");
            let client = reqwest::Client::new();

            let res = client
                .get(format!("{base_url}/api/files?type=image/jpeg"))
                .header("Authorization", format!("Bearer {jwt}"))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
        }
    );
}

#[tokio::test]
async fn test_list_recent_files() {
    test_group!("車検証ファイル一覧");
    test_case!("最近のファイル一覧を取得する", {
        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state.clone()).await;
        let tenant_id = crate::common::create_test_tenant(state.pool(), "CarinsRecent").await;
        let jwt = crate::common::create_test_jwt(tenant_id, "admin");
        let client = reqwest::Client::new();

        let res = client
            .get(format!("{base_url}/api/files/recent"))
            .header("Authorization", format!("Bearer {jwt}"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
    });
}

#[tokio::test]
async fn test_list_not_attached_files() {
    test_group!("車検証ファイル一覧");
    test_case!("未添付ファイル一覧を取得する", {
        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state.clone()).await;
        let tenant_id = crate::common::create_test_tenant(state.pool(), "CarinsNotAtt").await;
        let jwt = crate::common::create_test_jwt(tenant_id, "admin");
        let client = reqwest::Client::new();

        let res = client
            .get(format!("{base_url}/api/files/not-attached"))
            .header("Authorization", format!("Bearer {jwt}"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
    });
}

#[tokio::test]
async fn test_files_requires_auth() {
    test_group!("車検証ファイル認証");
    test_case!(
        "認証なしで全ファイルエンドポイントが 401 を返す",
        {
            let state = crate::common::setup_app_state().await;
            let base_url = crate::common::spawn_test_server(state).await;
            let client = reqwest::Client::new();

            for endpoint in ["/api/files", "/api/files/recent", "/api/files/not-attached"] {
                let res = client
                    .get(format!("{base_url}{endpoint}"))
                    .send()
                    .await
                    .unwrap();
                assert_eq!(res.status(), 401, "Expected 401 for {endpoint}");
            }
        }
    );
}

#[tokio::test]
async fn test_get_file_not_found() {
    test_group!("車検証ファイル取得");
    test_case!("存在しないファイル UUID で 404 を返す", {
        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state.clone()).await;
        let tenant_id = crate::common::create_test_tenant(state.pool(), "CarinsGetNF").await;
        let jwt = crate::common::create_test_jwt(tenant_id, "admin");
        let client = reqwest::Client::new();

        let fake_uuid = uuid::Uuid::new_v4();
        let res = client
            .get(format!("{base_url}/api/files/{fake_uuid}"))
            .header("Authorization", format!("Bearer {jwt}"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 404);
    });
}

// ============================================================
// upload: multipart upload
// ============================================================

#[tokio::test]
async fn test_upload_face_photo() {
    test_group!("ファイルアップロード");
    test_case!("顔写真をアップロードする", {
        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state.clone()).await;
        let tenant_id = crate::common::create_test_tenant(state.pool(), "UploadFace").await;
        let jwt = crate::common::create_test_jwt(tenant_id, "admin");
        let client = reqwest::Client::new();

        let file_part = reqwest::multipart::Part::bytes(b"fake-jpeg-data".to_vec())
            .file_name("test.jpg")
            .mime_str("image/jpeg")
            .unwrap();
        let form = reqwest::multipart::Form::new().part("file", file_part);

        let res = client
            .post(format!("{base_url}/api/upload/face-photo"))
            .header("Authorization", format!("Bearer {jwt}"))
            .multipart(form)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        let body: Value = res.json().await.unwrap();
        assert!(body["url"].as_str().is_some());
    });
}

#[tokio::test]
async fn test_upload_report_audio() {
    test_group!("ファイルアップロード");
    test_case!("レポート音声をアップロードする", {
        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state.clone()).await;
        let tenant_id = crate::common::create_test_tenant(state.pool(), "UploadAudio").await;
        let jwt = crate::common::create_test_jwt(tenant_id, "admin");
        let client = reqwest::Client::new();

        let file_part = reqwest::multipart::Part::bytes(b"fake-audio".to_vec())
            .file_name("test.webm")
            .mime_str("audio/webm")
            .unwrap();
        let form = reqwest::multipart::Form::new().part("file", file_part);

        let res = client
            .post(format!("{base_url}/api/upload/report-audio"))
            .header("Authorization", format!("Bearer {jwt}"))
            .multipart(form)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        let body: Value = res.json().await.unwrap();
        assert!(body["url"].as_str().is_some());
    });
}

#[tokio::test]
async fn test_upload_blow_video() {
    test_group!("ファイルアップロード");
    test_case!("吹込動画をアップロードする", {
        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state.clone()).await;
        let tenant_id = crate::common::create_test_tenant(state.pool(), "UploadVideo").await;
        let jwt = crate::common::create_test_jwt(tenant_id, "admin");
        let client = reqwest::Client::new();

        let file_part = reqwest::multipart::Part::bytes(b"fake-video".to_vec())
            .file_name("test.webm")
            .mime_str("video/webm")
            .unwrap();
        let form = reqwest::multipart::Form::new().part("file", file_part);

        let res = client
            .post(format!("{base_url}/api/upload/blow-video"))
            .header("Authorization", format!("Bearer {jwt}"))
            .multipart(form)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        let body: Value = res.json().await.unwrap();
        assert!(body["url"].as_str().is_some());
    });
}

// ============================================================
// carins_files: CRUD (create → get → delete → restore)
// ============================================================

#[tokio::test]
async fn test_create_file_base64() {
    test_group!("車検証ファイル CRUD");
    test_case!(
        "Base64 でファイル作成 → 取得 → 削除 → 復元する",
        {
            use base64::{engine::general_purpose::STANDARD, Engine};

            let state = crate::common::setup_app_state().await;
            let base_url = crate::common::spawn_test_server(state.clone()).await;
            let tenant_id = crate::common::create_test_tenant(state.pool(), "CarinsCreate").await;
            let jwt = crate::common::create_test_jwt(tenant_id, "admin");
            let auth = format!("Bearer {jwt}");
            let client = reqwest::Client::new();

            let content = STANDARD.encode(b"test file data");

            let res = client
                .post(format!("{base_url}/api/files"))
                .header("Authorization", &auth)
                .json(&serde_json::json!({
                    "filename": "test.txt",
                    "type": "text/plain",
                    "content": content
                }))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 201);
            let file: Value = res.json().await.unwrap();
            let file_uuid = file["uuid"].as_str().unwrap();

            // get
            let res = client
                .get(format!("{base_url}/api/files/{file_uuid}"))
                .header("Authorization", &auth)
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);

            // delete (soft)
            let res = client
                .post(format!("{base_url}/api/files/{file_uuid}/delete"))
                .header("Authorization", &auth)
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 204);

            // restore
            let res = client
                .post(format!("{base_url}/api/files/{file_uuid}/restore"))
                .header("Authorization", &auth)
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 204);
        }
    );
}

#[tokio::test]
async fn test_delete_file_not_found() {
    test_group!("車検証ファイル CRUD");
    test_case!("存在しないファイルの削除で 404 を返す", {
        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state.clone()).await;
        let tenant_id = crate::common::create_test_tenant(state.pool(), "CarinsDelNF").await;
        let jwt = crate::common::create_test_jwt(tenant_id, "admin");
        let client = reqwest::Client::new();

        let fake = uuid::Uuid::new_v4();
        let res = client
            .post(format!("{base_url}/api/files/{fake}/delete"))
            .header("Authorization", format!("Bearer {jwt}"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 404);
    });
}

#[tokio::test]
async fn test_download_file() {
    test_group!("車検証ファイルダウンロード");
    test_case!(
        "ファイルをアップロードしてダウンロードする",
        {
            use base64::{engine::general_purpose::STANDARD, Engine};

            let state = crate::common::setup_app_state().await;
            let base_url = crate::common::spawn_test_server(state.clone()).await;
            let tenant_id = crate::common::create_test_tenant(state.pool(), "CarinsDL").await;
            let jwt = crate::common::create_test_jwt(tenant_id, "admin");
            let auth = format!("Bearer {jwt}");
            let client = reqwest::Client::new();

            let content = STANDARD.encode(b"download-test-data");
            let res = client
                .post(format!("{base_url}/api/files"))
                .header("Authorization", &auth)
                .json(&serde_json::json!({
                    "filename": "dl-test.bin",
                    "type": "application/octet-stream",
                    "content": content
                }))
                .send()
                .await
                .unwrap();
            let file: Value = res.json().await.unwrap();
            let file_uuid = file["uuid"].as_str().unwrap();

            let res = client
                .get(format!("{base_url}/api/files/{file_uuid}/download"))
                .header("Authorization", &auth)
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
            let bytes = res.bytes().await.unwrap();
            assert_eq!(&bytes[..], b"download-test-data");
        }
    );
}

// ============================================================
// car_inspection / car_inspection_files: DB エラー (table RENAME)
// ============================================================

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_car_inspections_db_error() {
    test_group!("車検証 DB エラー");
    test_case!(
        "car_inspection RENAME → 全エンドポイントが 500 を返す",
        {
            let _db = crate::common::DB_RENAME_LOCK.lock().unwrap();
            let _flock = crate::common::db_rename_flock();
            let state = crate::common::setup_app_state().await;
            let base_url = crate::common::spawn_test_server(state.clone()).await;
            let tenant_id = crate::common::create_test_tenant(state.pool(), "CarInsDbErr").await;
            let jwt = crate::common::create_test_jwt(tenant_id, "admin");
            let auth = format!("Bearer {jwt}");
            let client = reqwest::Client::new();

            // RENAME car_inspection to break all queries
            sqlx::query("ALTER TABLE alc_api.car_inspection RENAME TO car_inspection_bak")
                .execute(state.pool())
                .await
                .unwrap();

            // car-inspections/current → 500
            let res = client
                .get(format!("{base_url}/api/car-inspections/current"))
                .header("Authorization", &auth)
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 500, "current should 500");

            // car-inspections/{id} → 500
            let res = client
                .get(format!("{base_url}/api/car-inspections/1"))
                .header("Authorization", &auth)
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 500, "get_by_id should 500");

            // car-inspections/vehicle-categories → 500
            let res = client
                .get(format!("{base_url}/api/car-inspections/vehicle-categories"))
                .header("Authorization", &auth)
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 500, "vehicle-categories should 500");

            // car-inspections/expired → 500
            let res = client
                .get(format!("{base_url}/api/car-inspections/expired"))
                .header("Authorization", &auth)
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 500, "expired should 500");

            // car-inspections/renew → 500
            let res = client
                .get(format!("{base_url}/api/car-inspections/renew"))
                .header("Authorization", &auth)
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 500, "renew should 500");

            // car-inspection-files/current → 500 (joins car_inspection)
            let res = client
                .get(format!("{base_url}/api/car-inspection-files/current"))
                .header("Authorization", &auth)
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 500, "car-inspection-files should 500");

            // Restore
            sqlx::query("ALTER TABLE alc_api.car_inspection_bak RENAME TO car_inspection")
                .execute(state.pool())
                .await
                .unwrap();
        }
    );
}

// ============================================================
// upload: multipart
// ============================================================

#[tokio::test]
async fn test_upload_requires_auth() {
    test_group!("ファイルアップロード認証");
    test_case!(
        "認証なしで全アップロードエンドポイントが 401 を返す",
        {
            let state = crate::common::setup_app_state().await;
            let base_url = crate::common::spawn_test_server(state).await;
            let client = reqwest::Client::new();

            for endpoint in [
                "/api/upload/face-photo",
                "/api/upload/report-audio",
                "/api/upload/blow-video",
            ] {
                let res = client
                    .post(format!("{base_url}{endpoint}"))
                    .send()
                    .await
                    .unwrap();
                assert_eq!(res.status(), 401, "Expected 401 for {endpoint}");
            }
        }
    );
}
