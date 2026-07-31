//! mock テスト for `GET /api/dtako/events` (Refs ohishi-exp/rust-ichibanboshi#205 実装計画 01)。
//!
//! - DB 不要 (`MockDtakoYTimeExportRepository` を差し込み)
//! - R2 不要 (`MockStorage` に CSV を upload してから handler を叩く)
//!
//! 検証の主眼は「**生行がそのまま返る**」こと。分類・正規化・行の取捨選択をしていたら
//! ここで落ちる。

use std::sync::atomic::Ordering;
use std::sync::Arc;

use chrono::TimeZone;

use crate::mock_helpers::app_state::setup_mock_app_state;
use crate::mock_helpers::MockDtakoYTimeExportRepository;
use rust_alc_api::db::repository::dtako_y_time_export::{
    DtakoDriverOperation, DtakoDriverRef, YTimeExportOperation,
};

fn test_tenant_id() -> uuid::Uuid {
    uuid::Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap()
}

fn test_auth_header() -> String {
    let jwt = crate::common::create_test_jwt_for_user(
        uuid::Uuid::new_v4(),
        test_tenant_id(),
        "mock-test@example.com",
        "admin",
    );
    format!("Bearer {jwt}")
}

/// 運転手 (対象乗務員区分 = 1) と副運転手 (= 2) が混在した KUDGIVT.csv。
/// UTF-8 で置く (`split_csv_from_r2` は UTF-8 化して保存するため本番と同じ)。
const KUDGIVT_CSV: &str =
    "運行NO,読取日,乗務員CD1,乗務員名１,対象乗務員区分,開始日時,イベントCD,イベント名,区間時間\n\
     U001,2026/06/15 00:00:00,D001,テスト 一郎,1,2026/06/15 09:00:00,201,走行,30\n\
     U001,2026/06/15 00:00:00,D002,テスト 二郎,2,2026/06/15 09:00:00,201,走行,30\n\
     U001,2026/06/15 00:00:00,D001,テスト 一郎,1,2026/06/15 12:00:00,301,休憩,60\n";

async fn upload_kudgivt(
    storage: &dyn rust_alc_api::storage::StorageBackend,
    tenant_id: &uuid::Uuid,
    unko_no: &str,
    csv: &str,
) {
    let key = format!("{}/unko/{}/KUDGIVT.csv", tenant_id, unko_no);
    storage
        .upload(&key, csv.as_bytes(), "text/csv")
        .await
        .unwrap();
}

fn op(unko_no: &str, hour: u32) -> YTimeExportOperation {
    YTimeExportOperation {
        unko_no: unko_no.into(),
        crew_role: 1,
        departure_at: Some(
            chrono::Utc
                .with_ymd_and_hms(2026, 6, 15, hour, 0, 0)
                .unwrap(),
        ),
        return_at: Some(chrono::Utc.with_ymd_and_hms(2026, 6, 15, 18, 0, 0).unwrap()),
        r2_key_prefix: None,
    }
}

#[tokio::test]
async fn returns_404_when_driver_cd_unknown() {
    let state = setup_mock_app_state();
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let res = reqwest::Client::new()
        .get(format!(
            "{base_url}/api/dtako/events?driver_cd=NOPE&date_from=2026-06-01&date_to=2026-06-30"
        ))
        .header("Authorization", test_auth_header())
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn returns_400_when_date_from_after_date_to() {
    let state = setup_mock_app_state();
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let res = reqwest::Client::new()
        .get(format!(
            "{base_url}/api/dtako/events?driver_cd=D001&date_from=2026-06-30&date_to=2026-06-01"
        ))
        .header("Authorization", test_auth_header())
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 400);
    assert!(res.text().await.unwrap().contains("date_from > date_to"));
}

#[tokio::test]
async fn returns_400_when_range_exceeds_limit() {
    let state = setup_mock_app_state();
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let res = reqwest::Client::new()
        .get(format!(
            "{base_url}/api/dtako/events?driver_cd=D001&date_from=2020-01-01&date_to=2026-06-30"
        ))
        .header("Authorization", test_auth_header())
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 400);
    assert!(res.text().await.unwrap().contains("range too wide"));
}

#[tokio::test]
async fn returns_400_when_required_query_params_missing() {
    // axum の Query extractor が弾く。driver_cd / date_from / date_to は必須。
    let state = setup_mock_app_state();
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let res = reqwest::Client::new()
        .get(format!("{base_url}/api/dtako/events?driver_cd=D001"))
        .header("Authorization", test_auth_header())
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 400);
}

#[tokio::test]
async fn returns_500_when_lookup_driver_db_error() {
    let mut state = setup_mock_app_state();
    let mock = Arc::new(MockDtakoYTimeExportRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    state.dtako_y_time_export = mock;

    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let res = reqwest::Client::new()
        .get(format!(
            "{base_url}/api/dtako/events?driver_cd=D001&date_from=2026-06-01&date_to=2026-06-30"
        ))
        .header("Authorization", test_auth_header())
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

#[tokio::test]
async fn returns_rows_verbatim_for_every_operation_in_one_request() {
    let mut state = setup_mock_app_state();
    let tenant_id = test_tenant_id();

    for unko_no in ["U001", "U002"] {
        upload_kudgivt(
            state.dtako_storage.as_ref().unwrap().as_ref(),
            &tenant_id,
            unko_no,
            KUDGIVT_CSV,
        )
        .await;
    }

    state.dtako_y_time_export = Arc::new(
        MockDtakoYTimeExportRepository::default()
            .with_driver(uuid::Uuid::new_v4(), "テスト 一郎")
            // departure_at で昇順に整列されることも確認したいので逆順で渡す
            .with_operations(vec![op("U002", 15), op("U001", 9)]),
    );

    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let res = reqwest::Client::new()
        .get(format!(
            "{base_url}/api/dtako/events?driver_cd=D001&date_from=2026-06-01&date_to=2026-06-30"
        ))
        .header("Authorization", test_auth_header())
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["driver"]["cd"], "D001");
    assert_eq!(body["driver"]["name"], "テスト 一郎");
    assert_eq!(body["period"]["date_from"], "2026-06-01");
    assert_eq!(body["period"]["date_to"], "2026-06-30");
    assert!(body["warnings"].as_array().unwrap().is_empty());

    let ops = body["operations"].as_array().unwrap();
    assert_eq!(ops.len(), 2);
    // departure_at 昇順
    assert_eq!(ops[0]["unko_no"], "U001");
    assert_eq!(ops[1]["unko_no"], "U002");
    assert_eq!(ops[0]["crew_role"], 1);
    assert_eq!(ops[0]["departure_at"], "2026-06-15T09:00:00Z");
    assert_eq!(ops[0]["return_at"], "2026-06-15T18:00:00Z");

    // headers は運行ごとに持つ
    let headers = ops[0]["headers"].as_array().unwrap();
    assert_eq!(headers[0], "運行NO");
    assert_eq!(headers[4], "対象乗務員区分");
    assert_eq!(headers.len(), 9);

    // 生行そのまま — 副運転手 (対象乗務員区分 = 2) の行も落とさない
    let rows = ops[0]["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[1][4], "2");
    // 時刻は文字列のまま。パースも TZ 変換もしていないこと
    assert_eq!(rows[0][5], "2026/06/15 09:00:00");
    assert_eq!(rows[0][6], "201");
}

#[tokio::test]
async fn missing_csv_becomes_warning_not_error() {
    // has_kudgivt = TRUE でも R2 分割が数秒遅れて NoSuchKey になることがある (#205 リスク表)。
    // その運行だけ warning に落とし、他の運行と 200 を返す。
    let mut state = setup_mock_app_state();
    let tenant_id = test_tenant_id();
    upload_kudgivt(
        state.dtako_storage.as_ref().unwrap().as_ref(),
        &tenant_id,
        "U001",
        KUDGIVT_CSV,
    )
    .await;

    state.dtako_y_time_export = Arc::new(
        MockDtakoYTimeExportRepository::default()
            .with_driver(uuid::Uuid::new_v4(), "テスト 一郎")
            .with_operations(vec![op("U001", 9), op("U_MISSING", 15)]),
    );

    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let res = reqwest::Client::new()
        .get(format!(
            "{base_url}/api/dtako/events?driver_cd=D001&date_from=2026-06-01&date_to=2026-06-30"
        ))
        .header("Authorization", test_auth_header())
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["operations"].as_array().unwrap().len(), 1);
    let warns = body["warnings"].as_array().unwrap();
    assert_eq!(warns.len(), 1);
    assert!(warns[0].as_str().unwrap().contains("U_MISSING"));
}

#[tokio::test]
async fn returns_empty_operations_when_no_operation_matches() {
    let mut state = setup_mock_app_state();
    state.dtako_y_time_export = Arc::new(
        MockDtakoYTimeExportRepository::default().with_driver(uuid::Uuid::new_v4(), "テスト 一郎"),
    );

    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let res = reqwest::Client::new()
        .get(format!(
            "{base_url}/api/dtako/events?driver_cd=D001&date_from=2026-06-01&date_to=2026-06-30"
        ))
        .header("Authorization", test_auth_header())
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert!(body["operations"].as_array().unwrap().is_empty());
    assert!(body["warnings"].as_array().unwrap().is_empty());
}

// =============================================================================
// 全乗務員版 (driver_cd 省略、Refs #205 実装計画 01)
// =============================================================================

fn driver_ref(id: uuid::Uuid, cd: &str, name: &str) -> DtakoDriverRef {
    DtakoDriverRef {
        driver_id: id,
        driver_cd: cd.into(),
        driver_name: name.into(),
    }
}

fn driver_op(id: uuid::Uuid, unko_no: &str, crew_role: i32, hour: u32) -> DtakoDriverOperation {
    DtakoDriverOperation {
        driver_id: id,
        unko_no: unko_no.into(),
        crew_role,
        departure_at: Some(
            chrono::Utc
                .with_ymd_and_hms(2026, 6, 15, hour, 0, 0)
                .unwrap(),
        ),
        return_at: Some(chrono::Utc.with_ymd_and_hms(2026, 6, 15, 18, 0, 0).unwrap()),
        r2_key_prefix: None,
    }
}

#[tokio::test]
async fn all_drivers_groups_operations_per_driver() {
    let mut state = setup_mock_app_state();
    let tenant_id = test_tenant_id();
    let a = uuid::Uuid::new_v4();
    let b = uuid::Uuid::new_v4();

    for unko_no in ["U001", "U002"] {
        upload_kudgivt(
            state.dtako_storage.as_ref().unwrap().as_ref(),
            &tenant_id,
            unko_no,
            KUDGIVT_CSV,
        )
        .await;
    }

    state.dtako_y_time_export = Arc::new(
        MockDtakoYTimeExportRepository::default()
            // driver_cd 昇順に整列されることも見たいので逆順で渡す
            .with_drivers(vec![
                driver_ref(b, "D002", "テスト 二郎"),
                driver_ref(a, "D001", "テスト 一郎"),
            ])
            .with_driver_operations(vec![
                driver_op(a, "U001", 1, 9),
                driver_op(b, "U002", 1, 15),
            ]),
    );

    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let res = reqwest::Client::new()
        .get(format!(
            "{base_url}/api/dtako/events?date_from=2026-06-01&date_to=2026-06-30"
        ))
        .header("Authorization", test_auth_header())
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    let body: serde_json::Value = res.json().await.unwrap();
    // 全乗務員版に top-level driver は無い
    assert!(body.get("driver").is_none());
    assert_eq!(body["period"]["date_from"], "2026-06-01");
    assert!(body["next_after_driver_cd"].is_null());
    assert!(body["warnings"].as_array().unwrap().is_empty());

    let drivers = body["drivers"].as_array().unwrap();
    assert_eq!(drivers.len(), 2);
    assert_eq!(drivers[0]["driver"]["cd"], "D001");
    assert_eq!(drivers[1]["driver"]["cd"], "D002");

    // 各要素は単一乗務員版の driver + operations と同じ形
    let ops = drivers[0]["operations"].as_array().unwrap();
    assert_eq!(ops.len(), 1);
    assert_eq!(ops[0]["unko_no"], "U001");
    assert_eq!(ops[0]["crew_role"], 1);
    assert_eq!(ops[0]["departure_at"], "2026-06-15T09:00:00Z");
    assert_eq!(ops[0]["headers"].as_array().unwrap().len(), 9);
    // 生行そのまま (副運転手の行も残る)
    assert_eq!(ops[0]["rows"].as_array().unwrap().len(), 3);
    assert_eq!(
        drivers[1]["operations"].as_array().unwrap()[0]["unko_no"],
        "U002"
    );
}

#[tokio::test]
async fn all_drivers_shares_one_r2_fetch_for_a_shared_operation() {
    // 同じ運行に運転手と副運転手が相乗り = R2 key が重複する。
    // 重複排除されても両方の乗務員に同じ生行が配られること。
    let mut state = setup_mock_app_state();
    let tenant_id = test_tenant_id();
    let a = uuid::Uuid::new_v4();
    let b = uuid::Uuid::new_v4();

    upload_kudgivt(
        state.dtako_storage.as_ref().unwrap().as_ref(),
        &tenant_id,
        "U001",
        KUDGIVT_CSV,
    )
    .await;

    state.dtako_y_time_export = Arc::new(
        MockDtakoYTimeExportRepository::default()
            .with_drivers(vec![
                driver_ref(a, "D001", "テスト 一郎"),
                driver_ref(b, "D002", "テスト 二郎"),
            ])
            .with_driver_operations(vec![driver_op(a, "U001", 1, 9), driver_op(b, "U001", 2, 9)]),
    );

    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let res = reqwest::Client::new()
        .get(format!(
            "{base_url}/api/dtako/events?date_from=2026-06-01&date_to=2026-06-30"
        ))
        .header("Authorization", test_auth_header())
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    let body: serde_json::Value = res.json().await.unwrap();
    let drivers = body["drivers"].as_array().unwrap();
    assert_eq!(drivers.len(), 2);
    let rows_a = &drivers[0]["operations"][0]["rows"];
    let rows_b = &drivers[1]["operations"][0]["rows"];
    assert_eq!(rows_a, rows_b);
    assert_eq!(drivers[0]["operations"][0]["crew_role"], 1);
    assert_eq!(drivers[1]["operations"][0]["crew_role"], 2);
}

#[tokio::test]
async fn all_drivers_pages_by_driver_and_returns_cursor() {
    let mut state = setup_mock_app_state();
    let tenant_id = test_tenant_id();
    let a = uuid::Uuid::new_v4();
    let b = uuid::Uuid::new_v4();

    upload_kudgivt(
        state.dtako_storage.as_ref().unwrap().as_ref(),
        &tenant_id,
        "U001",
        KUDGIVT_CSV,
    )
    .await;

    let repo = Arc::new(
        MockDtakoYTimeExportRepository::default()
            .with_drivers(vec![
                driver_ref(a, "D001", "テスト 一郎"),
                driver_ref(b, "D002", "テスト 二郎"),
            ])
            .with_driver_operations(vec![driver_op(a, "U001", 1, 9)]),
    );
    state.dtako_y_time_export = repo;

    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let client = reqwest::Client::new();

    // 1 ページ目: page_size=1 ちょうど返るのでカーソルが付く
    let body: serde_json::Value = client
        .get(format!(
            "{base_url}/api/dtako/events?date_from=2026-06-01&date_to=2026-06-30&page_size=1"
        ))
        .header("Authorization", test_auth_header())
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(body["drivers"].as_array().unwrap().len(), 1);
    assert_eq!(body["drivers"][0]["driver"]["cd"], "D001");
    assert_eq!(body["next_after_driver_cd"], "D001");

    // 2 ページ目: カーソルの続きから。件数が page_size 未満なので終端
    let body: serde_json::Value = client
        .get(format!(
            "{base_url}/api/dtako/events?date_from=2026-06-01&date_to=2026-06-30&page_size=5&after_driver_cd=D001"
        ))
        .header("Authorization", test_auth_header())
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(body["drivers"].as_array().unwrap().len(), 1);
    assert_eq!(body["drivers"][0]["driver"]["cd"], "D002");
    // この乗務員には運行が無いので空
    assert!(body["drivers"][0]["operations"]
        .as_array()
        .unwrap()
        .is_empty());
    assert!(body["next_after_driver_cd"].is_null());
}

#[tokio::test]
async fn all_drivers_clamps_oversized_page_size() {
    // page_size=999 は MAX_PAGE_SIZE(50) に clamp される。
    // mock は limit 件で truncate するので、2 名とも返れば clamp 後も 2 以上だと判る。
    let mut state = setup_mock_app_state();
    let a = uuid::Uuid::new_v4();
    let b = uuid::Uuid::new_v4();
    state.dtako_y_time_export =
        Arc::new(MockDtakoYTimeExportRepository::default().with_drivers(vec![
            driver_ref(a, "D001", "テスト 一郎"),
            driver_ref(b, "D002", "テスト 二郎"),
        ]));

    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let body: serde_json::Value = reqwest::Client::new()
        .get(format!(
            "{base_url}/api/dtako/events?date_from=2026-06-01&date_to=2026-06-30&page_size=999"
        ))
        .header("Authorization", test_auth_header())
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(body["drivers"].as_array().unwrap().len(), 2);
    // 2 < clamp 後の 50 なので終端
    assert!(body["next_after_driver_cd"].is_null());
}

#[tokio::test]
async fn all_drivers_missing_csv_becomes_deduped_warning() {
    // 同じ運行に 2 名 → 落ちると同文の warning が 2 本立つので dedup されること
    let mut state = setup_mock_app_state();
    let a = uuid::Uuid::new_v4();
    let b = uuid::Uuid::new_v4();
    state.dtako_y_time_export = Arc::new(
        MockDtakoYTimeExportRepository::default()
            .with_drivers(vec![
                driver_ref(a, "D001", "テスト 一郎"),
                driver_ref(b, "D002", "テスト 二郎"),
            ])
            .with_driver_operations(vec![
                driver_op(a, "U_MISSING", 1, 9),
                driver_op(b, "U_MISSING", 2, 9),
            ]),
    );

    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let body: serde_json::Value = reqwest::Client::new()
        .get(format!(
            "{base_url}/api/dtako/events?date_from=2026-06-01&date_to=2026-06-30"
        ))
        .header("Authorization", test_auth_header())
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let warns = body["warnings"].as_array().unwrap();
    assert_eq!(
        warns.len(),
        1,
        "同文の warning は 1 本にまとめる: {warns:?}"
    );
    assert!(warns[0].as_str().unwrap().contains("U_MISSING"));
}

#[tokio::test]
async fn all_drivers_rejects_range_over_one_month() {
    let state = setup_mock_app_state();
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let res = reqwest::Client::new()
        .get(format!(
            "{base_url}/api/dtako/events?date_from=2026-06-01&date_to=2026-07-02"
        ))
        .header("Authorization", test_auth_header())
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 400);
    assert!(res.text().await.unwrap().contains("max 31"));
}

#[tokio::test]
async fn returns_400_when_paging_params_used_with_driver_cd() {
    let state = setup_mock_app_state();
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let res = reqwest::Client::new()
        .get(format!(
            "{base_url}/api/dtako/events?driver_cd=D001&date_from=2026-06-01&date_to=2026-06-30&page_size=5"
        ))
        .header("Authorization", test_auth_header())
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 400);
    assert!(res
        .text()
        .await
        .unwrap()
        .contains("only valid without driver_cd"));
}

#[tokio::test]
async fn all_drivers_returns_500_when_list_drivers_db_error() {
    let mut state = setup_mock_app_state();
    let mock = Arc::new(MockDtakoYTimeExportRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    state.dtako_y_time_export = mock;

    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let res = reqwest::Client::new()
        .get(format!(
            "{base_url}/api/dtako/events?date_from=2026-06-01&date_to=2026-06-30"
        ))
        .header("Authorization", test_auth_header())
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

#[tokio::test]
async fn all_drivers_returns_500_when_list_operations_db_error() {
    let mut state = setup_mock_app_state();
    let a = uuid::Uuid::new_v4();
    state.dtako_y_time_export = Arc::new(
        MockDtakoYTimeExportRepository::default()
            .with_drivers(vec![driver_ref(a, "D001", "テスト 一郎")])
            .failing_operations(),
    );

    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let res = reqwest::Client::new()
        .get(format!(
            "{base_url}/api/dtako/events?date_from=2026-06-01&date_to=2026-06-30"
        ))
        .header("Authorization", test_auth_header())
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

#[tokio::test]
async fn single_driver_returns_500_when_list_operations_db_error() {
    let mut state = setup_mock_app_state();
    state.dtako_y_time_export = Arc::new(
        MockDtakoYTimeExportRepository::default()
            .with_driver(uuid::Uuid::new_v4(), "テスト 一郎")
            .failing_operations(),
    );

    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let res = reqwest::Client::new()
        .get(format!(
            "{base_url}/api/dtako/events?driver_cd=D001&date_from=2026-06-01&date_to=2026-06-30"
        ))
        .header("Authorization", test_auth_header())
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

#[tokio::test]
async fn returns_500_when_dtako_storage_not_configured() {
    // STORAGE_BACKEND 未設定で起動した場合に相当。main.rs は Option<Arc<dyn StorageBackend>>
    // をそのまま AppState に載せるので、None を差し込めば本番と同じ経路を踏む。
    let mut state = setup_mock_app_state();
    state.dtako_storage = None;

    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let res = reqwest::Client::new()
        .get(format!(
            "{base_url}/api/dtako/events?driver_cd=D001&date_from=2026-06-01&date_to=2026-06-30"
        ))
        .header("Authorization", test_auth_header())
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
    assert!(res.text().await.unwrap().contains("storage not configured"));
}

// =============================================================================
// GET /api/dtako/events/etags (Refs ohishi-exp/rust-ichibanboshi#205 実装計画 13)
// =============================================================================

#[tokio::test]
async fn etags_returns_400_when_range_exceeds_limit() {
    // etags 専用の上限 (40日) は既存の全乗務員版 (31日) より広い。
    // 45 日は etags の上限すら超える。
    let state = setup_mock_app_state();
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let res = reqwest::Client::new()
        .get(format!(
            "{base_url}/api/dtako/events/etags?date_from=2026-06-01&date_to=2026-07-15"
        ))
        .header("Authorization", test_auth_header())
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 400);
    assert!(res.text().await.unwrap().contains("range too wide"));
}

#[tokio::test]
async fn etags_accepts_range_wider_than_all_drivers_cap() {
    // 32 日 (暦月 31 日 + 翌月またぎ 1 日) は既存の全乗務員版 (31日上限) だと
    // 400 になるが、etags は R2 GET を伴わないのでここを通せる必要がある。
    let mut state = setup_mock_app_state();
    state.dtako_y_time_export = Arc::new(MockDtakoYTimeExportRepository::default());

    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let res = reqwest::Client::new()
        .get(format!(
            "{base_url}/api/dtako/events/etags?date_from=2026-07-01&date_to=2026-08-01"
        ))
        .header("Authorization", test_auth_header())
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
}

#[tokio::test]
async fn etags_returns_500_when_dtako_storage_not_configured() {
    let mut state = setup_mock_app_state();
    state.dtako_storage = None;

    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let res = reqwest::Client::new()
        .get(format!(
            "{base_url}/api/dtako/events/etags?date_from=2026-06-01&date_to=2026-06-30"
        ))
        .header("Authorization", test_auth_header())
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
    assert!(res.text().await.unwrap().contains("storage not configured"));
}

#[tokio::test]
async fn etags_returns_500_when_list_drivers_db_error() {
    let mut state = setup_mock_app_state();
    let mock = Arc::new(MockDtakoYTimeExportRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    state.dtako_y_time_export = mock;

    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let res = reqwest::Client::new()
        .get(format!(
            "{base_url}/api/dtako/events/etags?date_from=2026-06-01&date_to=2026-06-30"
        ))
        .header("Authorization", test_auth_header())
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

#[tokio::test]
async fn etags_returns_500_when_list_operations_db_error() {
    let mut state = setup_mock_app_state();
    let a = uuid::Uuid::new_v4();
    state.dtako_y_time_export = Arc::new(
        MockDtakoYTimeExportRepository::default()
            .with_drivers(vec![driver_ref(a, "D001", "テスト 一郎")])
            .failing_operations(),
    );

    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let res = reqwest::Client::new()
        .get(format!(
            "{base_url}/api/dtako/events/etags?date_from=2026-06-01&date_to=2026-06-30"
        ))
        .header("Authorization", test_auth_header())
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

#[tokio::test]
async fn etags_returns_500_when_r2_list_fails() {
    // unko_no が非数字 (安全弁フォールバック) 経路での LIST 失敗。
    let mock_storage = Arc::new(crate::common::mock_storage::MockStorage::new(
        "dtako-bucket",
    ));
    mock_storage.fail_list.store(true, Ordering::SeqCst);

    let mut state = setup_mock_app_state();
    state.dtako_storage = Some(mock_storage);
    let a = uuid::Uuid::new_v4();
    state.dtako_y_time_export = Arc::new(
        MockDtakoYTimeExportRepository::default()
            .with_drivers(vec![driver_ref(a, "D001", "テスト 一郎")])
            .with_driver_operations(vec![driver_op(a, "U001", 1, 9)]),
    );

    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let res = reqwest::Client::new()
        .get(format!(
            "{base_url}/api/dtako/events/etags?date_from=2026-06-01&date_to=2026-06-30"
        ))
        .header("Authorization", test_auth_header())
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

#[tokio::test]
async fn etags_returns_500_when_scoped_day_list_fails() {
    // unko_no が数字 (日で絞る通常経路) での LIST 失敗。並列 LIST のうち 1 本でも
    // 落ちたら部分結果を返さず 500 に倒すこと (部分結果は下流の月ゲートが
    // 「運行が消えた」と誤検出する)。
    let mock_storage = Arc::new(crate::common::mock_storage::MockStorage::new(
        "dtako-bucket",
    ));
    mock_storage.fail_list.store(true, Ordering::SeqCst);

    let mut state = setup_mock_app_state();
    state.dtako_storage = Some(mock_storage);
    let a = uuid::Uuid::new_v4();
    state.dtako_y_time_export = Arc::new(
        MockDtakoYTimeExportRepository::default()
            .with_drivers(vec![driver_ref(a, "D001", "テスト 一郎")])
            .with_driver_operations(vec![driver_op(a, "260601090000001", 1, 9)]),
    );

    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let res = reqwest::Client::new()
        .get(format!(
            "{base_url}/api/dtako/events/etags?date_from=2026-06-01&date_to=2026-06-30"
        ))
        .header("Authorization", test_auth_header())
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

#[tokio::test]
async fn etags_intersects_db_and_r2_list_and_ignores_sibling_csv_types() {
    let mut state = setup_mock_app_state();
    let tenant_id = test_tenant_id();
    let a = uuid::Uuid::new_v4();
    let b = uuid::Uuid::new_v4();

    // U001: DB にも R2 にもある → etag が付く
    upload_kudgivt(
        state.dtako_storage.as_ref().unwrap().as_ref(),
        &tenant_id,
        "U001",
        KUDGIVT_CSV,
    )
    .await;
    // 同じ運行ディレクトリの KUDGURI.csv (sibling)。unko_no の抽出を誤ってこちらを
    // 拾わないこと (別項目や重複が出ないこと) を確認する。
    state
        .dtako_storage
        .as_ref()
        .unwrap()
        .upload(
            &format!("{}/unko/U001/KUDGURI.csv", tenant_id),
            b"dummy",
            "text/csv",
        )
        .await
        .unwrap();

    state.dtako_y_time_export = Arc::new(
        MockDtakoYTimeExportRepository::default()
            .with_drivers(vec![
                driver_ref(a, "D001", "テスト 一郎"),
                driver_ref(b, "D002", "テスト 二郎"),
            ])
            .with_driver_operations(vec![
                driver_op(a, "U001", 1, 9),
                // U002: DB にはあるが R2 の LIST にはまだ現れない (split 未完了想定)
                driver_op(b, "U002", 1, 15),
            ]),
    );

    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let body: serde_json::Value = reqwest::Client::new()
        .get(format!(
            "{base_url}/api/dtako/events/etags?date_from=2026-06-01&date_to=2026-06-30"
        ))
        .header("Authorization", test_auth_header())
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    assert_eq!(body["period"]["date_from"], "2026-06-01");
    assert_eq!(body["period"]["date_to"], "2026-06-30");
    assert!(body["warnings"].as_array().unwrap().is_empty());

    let items = body["items"].as_array().unwrap();
    // U001, U002 のみ (KUDGURI.csv からは項目が作られない)
    assert_eq!(items.len(), 2, "{items:?}");

    let u001 = items.iter().find(|i| i["unko_no"] == "U001").unwrap();
    assert!(
        u001["etag"].is_string(),
        "U001 は R2 に KUDGIVT.csv があるので etag が付く: {u001:?}"
    );

    let u002 = items.iter().find(|i| i["unko_no"] == "U002").unwrap();
    assert!(
        u002["etag"].is_null(),
        "U002 はまだ R2 の LIST に現れていないので etag: null: {u002:?}"
    );
}

#[tokio::test]
async fn etags_returns_empty_items_when_no_operation_matches() {
    let mock_storage = Arc::new(crate::common::mock_storage::MockStorage::new(
        "dtako-bucket",
    ));
    let mut state = setup_mock_app_state();
    state.dtako_storage = Some(mock_storage.clone());
    state.dtako_y_time_export =
        Arc::new(MockDtakoYTimeExportRepository::default().with_drivers(vec![]));

    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let body: serde_json::Value = reqwest::Client::new()
        .get(format!(
            "{base_url}/api/dtako/events/etags?date_from=2026-06-01&date_to=2026-06-30"
        ))
        .header("Authorization", test_auth_header())
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(body["items"].as_array().unwrap().is_empty());
    // 突き合わせる db_unko_no が無いので R2 の LIST は 1 回も呼ばれない。
    assert!(
        mock_storage.list_calls().is_empty(),
        "{:?}",
        mock_storage.list_calls()
    );
}

#[tokio::test]
async fn etags_scopes_r2_list_to_distinct_day_prefixes() {
    // db_unko_nos が 260601 (6月) と 260701 (7月、翌月またぎ先読み分) にまたがるケース。
    // 検査するのは 2 点:
    // (1) 応答 (unko_no ごとの etag) が「全部を LIST した場合」と完全に一致すること
    //     — 下の etags_day_scoped_list_matches_full_list_byte_for_byte がこれを
    //     参照実装との突き合わせで厳密にやる。ここでは値の性質だけ見る
    // (2) LIST が tenant 全体の裸 prefix でも月 prefix でもなく、distinct な YYMMDD の
    //     数だけ日で絞った prefix で呼ばれていること (Refs #205 comment 205-27)
    let mock_storage = Arc::new(crate::common::mock_storage::MockStorage::new(
        "dtako-bucket",
    ));
    let mut state = setup_mock_app_state();
    state.dtako_storage = Some(mock_storage.clone());
    let tenant_id = test_tenant_id();
    let a = uuid::Uuid::new_v4();
    let b = uuid::Uuid::new_v4();
    let c = uuid::Uuid::new_v4();

    // 6月、R2 にもある → etag が付く
    upload_kudgivt(
        mock_storage.as_ref(),
        &tenant_id,
        "260601090000001",
        KUDGIVT_CSV,
    )
    .await;
    // 7月、R2 にもある → etag が付く
    upload_kudgivt(
        mock_storage.as_ref(),
        &tenant_id,
        "260701090000002",
        KUDGIVT_CSV,
    )
    .await;
    // 対象日の外 (5月)。DB には無いキー。日で絞れていれば LIST 呼び出しの対象にすら
    // 入らない (呼び出し回数・prefix の assertion で検査する)。
    upload_kudgivt(
        mock_storage.as_ref(),
        &tenant_id,
        "260501090000009",
        KUDGIVT_CSV,
    )
    .await;
    // 同じ 6 月でも別の日 (06-02)。月 prefix (2606) なら巻き込まれるが、日 prefix
    // なら LIST されない — 4 桁と 6 桁の差が出る唯一のキー。
    upload_kudgivt(
        mock_storage.as_ref(),
        &tenant_id,
        "260602090000008",
        KUDGIVT_CSV,
    )
    .await;
    // 7月だが DB にはあるが R2 にはまだ無い運行 (split 未完了想定) → etag: null
    // (upload しない)

    state.dtako_y_time_export = Arc::new(
        MockDtakoYTimeExportRepository::default()
            .with_drivers(vec![
                driver_ref(a, "D001", "テスト 一郎"),
                driver_ref(b, "D002", "テスト 二郎"),
                driver_ref(c, "D003", "テスト 三郎"),
            ])
            .with_driver_operations(vec![
                driver_op(a, "260601090000001", 1, 9),
                driver_op(b, "260701090000002", 1, 10),
                driver_op(c, "260701090000003", 1, 11),
            ]),
    );

    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let body: serde_json::Value = reqwest::Client::new()
        .get(format!(
            "{base_url}/api/dtako/events/etags?date_from=2026-06-01&date_to=2026-07-01"
        ))
        .header("Authorization", test_auth_header())
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    let items = body["items"].as_array().unwrap();
    assert_eq!(items.len(), 3, "{items:?}");
    let find = |unko_no: &str| items.iter().find(|i| i["unko_no"] == unko_no).unwrap();
    assert!(find("260601090000001")["etag"].is_string(), "{items:?}");
    assert!(find("260701090000002")["etag"].is_string(), "{items:?}");
    assert!(
        find("260701090000003")["etag"].is_null(),
        "R2 にまだ現れていない運行は etag: null のはず: {items:?}"
    );

    // LIST は並列に投げるので順序は不定。集合として検査する。
    let mut calls = mock_storage.list_calls();
    calls.sort();
    let bare_prefix = format!("{tenant_id}/unko/");
    assert_eq!(
        calls,
        vec![
            format!("{bare_prefix}260601"),
            format!("{bare_prefix}260701"),
        ],
        "distinct な YYMMDD (260601, 260701) の数だけ日 prefix で呼ばれるはず: {calls:?}"
    );
}

#[tokio::test]
async fn etags_day_scoped_list_matches_full_list_byte_for_byte() {
    // #205-22 から引き継ぐ縛り: この口の応答は下流 (rust-ichibanboshi の月ゲート) の
    // 指紋そのものなので、LIST の絞り方を変えても **1 バイトも変わってはいけない**。
    // 「tenant 全体を裸 prefix で LIST して突き合わせる」= 絞り込み導入前の挙動を
    // テスト側で参照実装として組み直し、endpoint の応答と完全一致することを検査する。
    let mock_storage = Arc::new(crate::common::mock_storage::MockStorage::new(
        "dtako-bucket",
    ));
    let mut state = setup_mock_app_state();
    state.dtako_storage = Some(mock_storage.clone());
    let tenant_id = test_tenant_id();

    // 月境界 (5/31・7/01) と同月内の複数日をまたぐ、意地の悪い並び。
    let in_db_and_r2 = [
        "260531230000001",
        "260601090000002",
        "260601100000003",
        "260615120000004",
        "260701000000005",
    ];
    for unko_no in in_db_and_r2 {
        upload_kudgivt(mock_storage.as_ref(), &tenant_id, unko_no, KUDGIVT_CSV).await;
    }
    // R2 にだけあって DB に無い運行 (応答に出てはいけない)
    upload_kudgivt(
        mock_storage.as_ref(),
        &tenant_id,
        "260610080000099",
        KUDGIVT_CSV,
    )
    .await;
    // DB にだけあって R2 に無い運行 (etag: null で出る)
    let db_only = ["260601110000006", "260701010000007"];

    let mut ops = Vec::new();
    let mut drivers = Vec::new();
    for (i, unko_no) in in_db_and_r2.iter().chain(db_only.iter()).enumerate() {
        let id = uuid::Uuid::new_v4();
        drivers.push(driver_ref(id, &format!("D{i:03}"), "テスト"));
        ops.push(driver_op(id, unko_no, 1, 9));
    }
    state.dtako_y_time_export = Arc::new(
        MockDtakoYTimeExportRepository::default()
            .with_drivers(drivers)
            .with_driver_operations(ops),
    );

    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let body: serde_json::Value = reqwest::Client::new()
        .get(format!(
            "{base_url}/api/dtako/events/etags?date_from=2026-06-01&date_to=2026-07-01"
        ))
        .header("Authorization", test_auth_header())
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    // 参照実装: 裸 prefix で全 LIST → KUDGIVT.csv だけ拾う → db_unko_nos (sort + dedup)
    // を基準に突き合わせる。絞り込み導入前の production コードと同じ手順。
    let bare_prefix = format!("{tenant_id}/unko/");
    let listed = rust_alc_api::storage::StorageBackend::list(mock_storage.as_ref(), &bare_prefix)
        .await
        .unwrap();
    let full: std::collections::HashMap<String, String> = listed
        .into_iter()
        .filter_map(|obj| {
            let rest = obj.key.strip_prefix(&bare_prefix)?.to_string();
            let unko_no = rest.strip_suffix("/KUDGIVT.csv")?.to_string();
            obj.etag.map(|etag| (unko_no, etag))
        })
        .collect();
    let mut db_unko_nos: Vec<&str> = in_db_and_r2.iter().chain(db_only.iter()).copied().collect();
    db_unko_nos.sort_unstable();
    let expected: serde_json::Value = db_unko_nos
        .iter()
        .map(|unko_no| {
            serde_json::json!({
                "unko_no": unko_no,
                "etag": full.get(*unko_no).cloned(),
            })
        })
        .collect();

    assert_eq!(
        body["items"], expected,
        "日 prefix で絞っても応答は全 LIST 版と完全一致すること"
    );
}

#[tokio::test]
async fn etags_falls_back_to_full_list_when_unko_no_day_is_non_numeric() {
    // db_unko_nos の 1 件でも先頭 6 文字が数字でなければ、日で絞る前提が崩れるので
    // 速さより正しさを優先し、tenant 全体の裸 prefix で全 LIST に倒す。
    let mock_storage = Arc::new(crate::common::mock_storage::MockStorage::new(
        "dtako-bucket",
    ));
    let mut state = setup_mock_app_state();
    state.dtako_storage = Some(mock_storage.clone());
    let tenant_id = test_tenant_id();
    let a = uuid::Uuid::new_v4();
    let b = uuid::Uuid::new_v4();

    upload_kudgivt(
        mock_storage.as_ref(),
        &tenant_id,
        "260601090000001",
        KUDGIVT_CSV,
    )
    .await;

    state.dtako_y_time_export = Arc::new(
        MockDtakoYTimeExportRepository::default()
            .with_drivers(vec![
                driver_ref(a, "D001", "テスト 一郎"),
                driver_ref(b, "D002", "テスト 二郎"),
            ])
            .with_driver_operations(vec![
                driver_op(a, "260601090000001", 1, 9),
                // 何らかの理由で YYMMDD 部分が数字でない unko_no が紛れ込んだケース
                driver_op(b, "2606X1090000002", 1, 10),
            ]),
    );

    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let res = reqwest::Client::new()
        .get(format!(
            "{base_url}/api/dtako/events/etags?date_from=2026-06-01&date_to=2026-06-30"
        ))
        .header("Authorization", test_auth_header())
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["items"].as_array().unwrap().len(), 2);

    let calls = mock_storage.list_calls();
    let bare_prefix = format!("{tenant_id}/unko/");
    assert_eq!(calls, vec![bare_prefix], "{calls:?}");
}

#[tokio::test]
async fn etags_falls_back_to_full_list_when_unko_no_is_too_short() {
    // 先頭 6 文字すら取れない (3 文字しかない) 壊れた unko_no のケース。
    // 4 桁 (月) から 6 桁 (日) に変えた分、安全弁の発動条件はわずかに厳しくなっている
    // (先頭 5 文字までしか無い unko_no も倒れる)。倒れた先は正しい結果を返す経路
    // なので、壊れるのは速さだけ (Refs #205 comment 205-27)。
    let mock_storage = Arc::new(crate::common::mock_storage::MockStorage::new(
        "dtako-bucket",
    ));
    let mut state = setup_mock_app_state();
    state.dtako_storage = Some(mock_storage.clone());
    let tenant_id = test_tenant_id();
    let a = uuid::Uuid::new_v4();

    state.dtako_y_time_export = Arc::new(
        MockDtakoYTimeExportRepository::default()
            .with_drivers(vec![driver_ref(a, "D001", "テスト 一郎")])
            .with_driver_operations(vec![driver_op(a, "260", 1, 9)]),
    );

    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let res = reqwest::Client::new()
        .get(format!(
            "{base_url}/api/dtako/events/etags?date_from=2026-06-01&date_to=2026-06-30"
        ))
        .header("Authorization", test_auth_header())
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    let calls = mock_storage.list_calls();
    let bare_prefix = format!("{tenant_id}/unko/");
    assert_eq!(calls, vec![bare_prefix], "{calls:?}");
}

#[tokio::test]
async fn decodes_shift_jis_kudgivt_for_legacy_objects() {
    // R2 に Shift-JIS のまま置かれた古いデータ (split 前) のフォールバック経路。
    let mut state = setup_mock_app_state();
    let tenant_id = test_tenant_id();
    let (encoded, _, _) = encoding_rs::SHIFT_JIS.encode(KUDGIVT_CSV);
    state
        .dtako_storage
        .as_ref()
        .unwrap()
        .upload(
            &format!("{}/unko/U_SJIS/KUDGIVT.csv", tenant_id),
            &encoded,
            "text/csv",
        )
        .await
        .unwrap();

    state.dtako_y_time_export = Arc::new(
        MockDtakoYTimeExportRepository::default()
            .with_driver(uuid::Uuid::new_v4(), "テスト 一郎")
            .with_operations(vec![op("U_SJIS", 9)]),
    );

    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let body: serde_json::Value = reqwest::Client::new()
        .get(format!(
            "{base_url}/api/dtako/events?driver_cd=D001&date_from=2026-06-01&date_to=2026-06-30"
        ))
        .header("Authorization", test_auth_header())
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    // 文字化けせずヘッダが読めていればフォールバックが効いている
    assert_eq!(body["operations"][0]["headers"][0], "運行NO");
}
