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
