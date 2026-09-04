//! `/api/hub/measurements` (CoreS3 ハブ、Refs #564 ingest / #592 read) の mock テスト。
//!
//! - POST … cf-alc-recorder Worker → auth-worker /alc-internal-proxy 経由で
//!   X-Internal-Shared-Secret + X-Tenant-ID が付いて届く経路
//!   (`internal_shared_secret_router`) を DB なしで固定する。
//! - GET … テナント認証付き router (X-Tenant-ID) の絞り込み・ページング・
//!   バリデーションを DB なしで固定する。

use std::sync::atomic::Ordering;
use std::sync::Arc;

use uuid::Uuid;

use crate::mock_helpers::app_state::setup_mock_app_state;
use crate::mock_helpers::MockHubMeasurementsRepository;

fn measurement(seq: i64, kind: &str) -> serde_json::Value {
    serde_json::json!({
        "device_id": "hub-dev-1",
        "kind": kind,
        "seq": seq,
        "recorded_at_ms": 1_752_300_000_000i64,
        "payload": { "type": "temperature", "value": 36.5, "unit": "celsius" }
    })
}

// ---------------------------------------------------------------------------
// 認証: secret なし / 不一致 → 401、X-Tenant-ID 欠落 → 401
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_hub_measurements_requires_shared_secret() {
    let state = setup_mock_app_state();
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let client = reqwest::Client::new();

    // secret なし
    let res = client
        .post(format!("{base_url}/api/hub/measurements"))
        .json(&vec![measurement(1, "alcohol")])
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);

    // secret 不一致
    let res = client
        .post(format!("{base_url}/api/hub/measurements"))
        .header("X-Internal-Shared-Secret", "wrong-secret")
        .header("X-Tenant-ID", Uuid::new_v4().to_string())
        .json(&vec![measurement(1, "alcohol")])
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);

    // X-Tenant-ID 欠落
    let res = client
        .post(format!("{base_url}/api/hub/measurements"))
        .header(
            "X-Internal-Shared-Secret",
            crate::common::TEST_INTERNAL_SHARED_SECRET,
        )
        .json(&vec![measurement(1, "alcohol")])
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);
}

// ---------------------------------------------------------------------------
// 正常系: バッチ insert (X-Tenant-ID の tenant が repo に注入される)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_hub_measurements_batch_insert() {
    let mut state = setup_mock_app_state();
    let repo = Arc::new(MockHubMeasurementsRepository::default());
    state.hub_measurements = repo.clone();
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let tenant_id = Uuid::new_v4();
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/hub/measurements"))
        .header(
            "X-Internal-Shared-Secret",
            crate::common::TEST_INTERNAL_SHARED_SECRET,
        )
        .header("X-Tenant-ID", tenant_id.to_string())
        .json(&vec![
            measurement(1, "temperature"),
            measurement(2, "blood_pressure"),
            measurement(3, "alcohol"),
        ])
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 201);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["inserted"], 3);
    assert_eq!(body["duplicates"], 0);

    // tenant はヘッダー由来のものが repo に渡る
    let inserted = repo.inserted.lock().unwrap();
    assert_eq!(inserted.len(), 3);
    assert!(inserted.iter().all(|(t, _)| *t == tenant_id));
    assert_eq!(inserted[0].1.seq, 1);
    assert_eq!(inserted[0].1.recorded_at_ms, Some(1_752_300_000_000));
}

// ---------------------------------------------------------------------------
// 正常系: 単発 object も受ける / 再送 (同 seq) は duplicates で冪等
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_hub_measurements_single_and_resend_idempotent() {
    let mut state = setup_mock_app_state();
    let repo = Arc::new(MockHubMeasurementsRepository::default());
    state.hub_measurements = repo.clone();
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let tenant_id = Uuid::new_v4();
    let client = reqwest::Client::new();

    let send = |body: serde_json::Value| {
        client
            .post(format!("{base_url}/api/hub/measurements"))
            .header(
                "X-Internal-Shared-Secret",
                crate::common::TEST_INTERNAL_SHARED_SECRET,
            )
            .header("X-Tenant-ID", tenant_id.to_string())
            .json(&body)
            .send()
    };

    // 単発 object
    let res = send(measurement(10, "alcohol")).await.unwrap();
    assert_eq!(res.status(), 201);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["inserted"], 1);

    // 同じ seq の再送 → duplicates=1 (ACK 再送に対する冪等)
    let res = send(measurement(10, "alcohol")).await.unwrap();
    assert_eq!(res.status(), 201);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["inserted"], 0);
    assert_eq!(body["duplicates"], 1);
}

// ---------------------------------------------------------------------------
// 検証: allowlist 外 kind / 空バッチ / 負 seq → 400
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_hub_measurements_validation() {
    let state = setup_mock_app_state();
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let tenant_id = Uuid::new_v4();
    let client = reqwest::Client::new();

    for body in [
        serde_json::json!([measurement(1, "unknown-kind")]),
        serde_json::json!([]),
        serde_json::json!([measurement(-1, "alcohol")]),
    ] {
        let res = client
            .post(format!("{base_url}/api/hub/measurements"))
            .header(
                "X-Internal-Shared-Secret",
                crate::common::TEST_INTERNAL_SHARED_SECRET,
            )
            .header("X-Tenant-ID", tenant_id.to_string())
            .json(&body)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 400, "body={body}");
    }
}

// ---------------------------------------------------------------------------
// repo エラー → 500 (詳細は echo しない)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_hub_measurements_repo_error_is_500() {
    let mut state = setup_mock_app_state();
    let repo = Arc::new(MockHubMeasurementsRepository::default());
    repo.fail_next.store(true, Ordering::SeqCst);
    state.hub_measurements = repo.clone();
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/hub/measurements"))
        .header(
            "X-Internal-Shared-Secret",
            crate::common::TEST_INTERNAL_SHARED_SECRET,
        )
        .header("X-Tenant-ID", Uuid::new_v4().to_string())
        .json(&vec![measurement(1, "alcohol")])
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// ---------------------------------------------------------------------------
// GET: 絞り込み / created_at DESC ページング / テナント分離 (Refs #592)
// ---------------------------------------------------------------------------

/// mock repo に直接 seed する (POST 経路と同じ insert_batch を使う)。
async fn seed(
    repo: &MockHubMeasurementsRepository,
    tenant_id: Uuid,
    items: Vec<serde_json::Value>,
) {
    use rust_alc_api::db::repository::hub_measurements::HubMeasurementsRepository;
    let items: Vec<rust_alc_api::db::models::HubMeasurementCreate> = items
        .into_iter()
        .map(|v| serde_json::from_value(v).unwrap())
        .collect();
    repo.insert_batch(tenant_id, &items).await.unwrap();
}

fn measurement_for(device_id: &str, seq: i64, kind: &str) -> serde_json::Value {
    serde_json::json!({
        "device_id": device_id,
        "kind": kind,
        "seq": seq,
        "recorded_at_ms": 1_752_300_000_000i64,
        "payload": { "value": 36.5 }
    })
}

fn seqs(body: &serde_json::Value) -> Vec<i64> {
    body["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["seq"].as_i64().unwrap())
        .collect()
}

#[tokio::test]
async fn test_hub_measurements_list_filters_paging_and_isolation() {
    let mut state = setup_mock_app_state();
    let repo = Arc::new(MockHubMeasurementsRepository::default());
    state.hub_measurements = repo.clone();
    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();
    // created_at は insert 順に進む (mock repo の doc コメント参照) ので
    // created_at DESC = seq 降順になる。
    seed(
        &repo,
        tenant_a,
        vec![
            measurement_for("hub-dev-1", 1, "temperature"),
            measurement_for("hub-dev-1", 2, "alcohol"),
            measurement_for("hub-dev-2", 3, "alcohol"),
        ],
    )
    .await;
    seed(
        &repo,
        tenant_b,
        vec![measurement_for("hub-dev-1", 9, "alcohol")],
    )
    .await;

    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let client = reqwest::Client::new();
    let get = |tenant: Uuid, query: &str| {
        client
            .get(format!("{base_url}/api/hub/measurements?{query}"))
            .header("X-Tenant-ID", tenant.to_string())
            .send()
    };

    // 絞り込みなし → created_at DESC、他テナント (seq=9) は混ざらない
    let res = get(tenant_a, "").await.unwrap();
    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(seqs(&body), vec![3, 2, 1]);
    assert_eq!(body["limit"], 50);
    assert_eq!(body["has_more"], false);

    // device_id / kind 絞り込み
    let body: serde_json::Value = get(tenant_a, "device_id=hub-dev-1")
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(seqs(&body), vec![2, 1]);
    let body: serde_json::Value = get(tenant_a, "kind=alcohol")
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(seqs(&body), vec![3, 2]);

    // limit / offset と has_more
    let body: serde_json::Value = get(tenant_a, "limit=2")
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(seqs(&body), vec![3, 2]);
    assert_eq!(body["has_more"], true);
    let body: serde_json::Value = get(tenant_a, "limit=2&offset=2")
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(seqs(&body), vec![1]);
    assert_eq!(body["has_more"], false);
    assert_eq!(body["offset"], 2);

    // limit は clamp される (0 → 1、上限超え → 200)
    let body: serde_json::Value = get(tenant_a, "limit=0")
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(body["limit"], 1);
    assert_eq!(seqs(&body), vec![3]);
    let body: serde_json::Value = get(tenant_a, "limit=100000")
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(body["limit"], 200);

    // 別テナントからは自分の行だけ
    let body: serde_json::Value = get(tenant_b, "").await.unwrap().json().await.unwrap();
    assert_eq!(seqs(&body), vec![9]);
}

#[tokio::test]
async fn test_hub_measurements_list_validation_and_auth() {
    let state = setup_mock_app_state();
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let client = reqwest::Client::new();
    let tenant_id = Uuid::new_v4();

    // X-Tenant-ID なし → 401
    let res = client
        .get(format!("{base_url}/api/hub/measurements"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);

    // allowlist 外 kind / from > to → 400
    for query in [
        "kind=not-a-kind",
        "from=2026-08-04T00:00:00Z&to=2026-08-01T00:00:00Z",
    ] {
        let res = client
            .get(format!("{base_url}/api/hub/measurements?{query}"))
            .header("X-Tenant-ID", tenant_id.to_string())
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 400, "query={query}");
    }

    // 期間が同値 (閉区間) は 400 にしない
    let res = client
        .get(format!(
            "{base_url}/api/hub/measurements?from=2026-08-01T00:00:00Z&to=2026-08-01T00:00:00Z"
        ))
        .header("X-Tenant-ID", tenant_id.to_string())
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
}

#[tokio::test]
async fn test_hub_measurements_list_repo_error_is_500() {
    let mut state = setup_mock_app_state();
    let repo = Arc::new(MockHubMeasurementsRepository::default());
    repo.fail_next.store(true, Ordering::SeqCst);
    state.hub_measurements = repo.clone();
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;

    let res = reqwest::Client::new()
        .get(format!("{base_url}/api/hub/measurements"))
        .header("X-Tenant-ID", Uuid::new_v4().to_string())
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// ---------------------------------------------------------------------------
// session_id (Refs ippoan/alc-app-s3#112)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_hub_measurements_session_id_ingest_and_filter() {
    let mut state = setup_mock_app_state();
    let repo = Arc::new(MockHubMeasurementsRepository::default());
    state.hub_measurements = repo.clone();
    let tenant_id = Uuid::new_v4();
    seed(
        &repo,
        tenant_id,
        vec![
            serde_json::json!({"device_id":"hub-dev-1","kind":"alcohol","seq":1,"session_id":"s1","payload":{}}),
            serde_json::json!({"device_id":"hub-dev-1","kind":"temperature","seq":2,"session_id":"s1","payload":{}}),
            serde_json::json!({"device_id":"hub-dev-1","kind":"alcohol","seq":3,"session_id":"s2","payload":{}}),
            // 点呼外の単発 (session_id 無し)
            serde_json::json!({"device_id":"hub-dev-1","kind":"temperature","seq":4,"payload":{}}),
        ],
    )
    .await;

    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let client = reqwest::Client::new();
    let get = |query: &str| {
        client
            .get(format!("{base_url}/api/hub/measurements?{query}"))
            .header("X-Tenant-ID", tenant_id.to_string())
            .send()
    };

    // session_id で束ねて引ける
    let body: serde_json::Value = get("session_id=s1").await.unwrap().json().await.unwrap();
    assert_eq!(seqs(&body), vec![2, 1]);

    // session_id 無しの行は混ざらない
    let body: serde_json::Value = get("session_id=s2").await.unwrap().json().await.unwrap();
    assert_eq!(seqs(&body), vec![3]);

    // 絞り込み無しなら全部返り、単発は null
    let body: serde_json::Value = get("").await.unwrap().json().await.unwrap();
    assert_eq!(seqs(&body), vec![4, 3, 2, 1]);
    assert!(body["items"][0]["session_id"].is_null());
    assert_eq!(body["items"][3]["session_id"], "s1");

    // 不正な session_id は 400 (端末由来 = untrusted)
    for bad in ["session_id=", "session_id=bad%20id", "session_id=a%2Fb"] {
        let res = get(bad).await.unwrap();
        assert_eq!(res.status(), 400, "query={bad}");
    }
}

#[tokio::test]
async fn test_hub_measurements_ingest_rejects_bad_session_id() {
    let mut state = setup_mock_app_state();
    let repo = Arc::new(MockHubMeasurementsRepository::default());
    state.hub_measurements = repo.clone();
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let tenant_id = Uuid::new_v4();
    let client = reqwest::Client::new();

    let post = |session_id: serde_json::Value| {
        client
            .post(format!("{base_url}/api/hub/measurements"))
            .header(
                "X-Internal-Shared-Secret",
                crate::common::TEST_INTERNAL_SHARED_SECRET,
            )
            .header("X-Tenant-ID", tenant_id.to_string())
            .json(&serde_json::json!([{
                "device_id": "hub-dev-1",
                "kind": "alcohol",
                "seq": 1,
                "session_id": session_id,
                "payload": {}
            }]))
            .send()
    };

    // 記号混じり / 空 / 長すぎ → 400
    for bad in [
        serde_json::json!("bad id"),
        serde_json::json!(""),
        serde_json::json!("x".repeat(65)),
    ] {
        let res = post(bad.clone()).await.unwrap();
        assert_eq!(res.status(), 400, "session_id={bad}");
    }

    // 正常値 → 201 で repo にそのまま渡る
    let res = post(serde_json::json!("s-42_7")).await.unwrap();
    assert_eq!(res.status(), 201);
    let inserted = repo.inserted.lock().unwrap();
    assert_eq!(inserted[0].1.session_id.as_deref(), Some("s-42_7"));
}

// ---------------------------------------------------------------------------
// kind="timecard" の社員解決の凍結 (Refs ippoan/alc-app-s3#134)
//
// NFC タイムカード端末は打刻を「測定」として送る。ingest は insert の**前に**
// カードから社員を解決し、結果を payload に凍結する。読み出しはこれを読むだけで、
// JOIN も再解決もしない — 再解決すると、退職者のカードを新人に回した瞬間に
// 退職者の過去の打刻が新人に付く (timecard_cards は hard DELETE + UNIQUE なので
// 付け替えが「削除 → 再登録」になる)。
// ---------------------------------------------------------------------------

fn timecard_item(seq: i64, card_id: &str) -> serde_json::Value {
    serde_json::json!({
        "device_id": "timecard-dev-1",
        "kind": "timecard",
        "seq": seq,
        "recorded_at_ms": 1_752_300_000_000i64,
        "payload": { "card_id": card_id, "card_kind": "felica_idm" }
    })
}

/// timecard 中継用の state (hub_measurements と timecard の両 repo を差し替える)
fn timecard_state() -> (
    rust_alc_api::AppState,
    Arc<MockHubMeasurementsRepository>,
    Arc<crate::mock_helpers::MockTimecardRepository>,
) {
    let mut state = setup_mock_app_state();
    let hub = Arc::new(MockHubMeasurementsRepository::default());
    let tc = Arc::new(crate::mock_helpers::MockTimecardRepository::default());
    state.hub_measurements = hub.clone();
    state.timecard = tc.clone();
    (state, hub, tc)
}

async fn post_timecard(
    base_url: &str,
    tenant_id: Uuid,
    items: Vec<serde_json::Value>,
) -> reqwest::Response {
    reqwest::Client::new()
        .post(format!("{base_url}/api/hub/measurements"))
        .header(
            "X-Internal-Shared-Secret",
            crate::common::TEST_INTERNAL_SHARED_SECRET,
        )
        .header("X-Tenant-ID", tenant_id.to_string())
        .json(&items)
        .send()
        .await
        .unwrap()
}

/// 凍結された payload の employee_id を取り出す (無ければ None)
fn frozen_employee_id(
    hub: &crate::mock_helpers::MockHubMeasurementsRepository,
    seq: i64,
) -> Option<String> {
    hub.inserted
        .lock()
        .unwrap()
        .iter()
        .find(|(_, it)| it.seq == seq)
        .and_then(|(_, it)| it.payload.get("employee_id").cloned())
        .and_then(|v| v.as_str().map(str::to_owned))
}

/// カード登録あり → 解決結果が payload に凍結される
#[tokio::test]
async fn test_timecard_freezes_resolved_employee_in_payload() {
    let (state, hub, tc) = timecard_state();
    let tenant_id = Uuid::new_v4();
    let employee_id = Uuid::new_v4();
    *tc.find_card_data.lock().unwrap() = Some(rust_alc_api::db::models::TimecardCard {
        id: Uuid::new_v4(),
        tenant_id,
        employee_id,
        card_id: "01401d0b1d37b660".to_string(),
        label: None,
        created_at: chrono::Utc::now(),
    });
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;

    let res = post_timecard(
        &base_url,
        tenant_id,
        vec![timecard_item(1, "01401d0b1d37b660")],
    )
    .await;
    assert_eq!(res.status(), 201);
    assert_eq!(frozen_employee_id(&hub, 1), Some(employee_id.to_string()));
}

/// 端末が送る大文字 IDm は、照合の手前で正規化されてから引かれる
/// (Refs ippoan/alc-app-s3#134)。ブラウザ版 punch と同じ choke point
/// (`resolve_employee_by_card`) を通るので、片方だけ正規化が外れることはない
#[tokio::test]
async fn test_timecard_normalizes_card_id_before_lookup() {
    let (state, hub, tc) = timecard_state();
    let tenant_id = Uuid::new_v4();
    let employee_id = Uuid::new_v4();
    *tc.find_card_data.lock().unwrap() = Some(rust_alc_api::db::models::TimecardCard {
        id: Uuid::new_v4(),
        tenant_id,
        employee_id,
        card_id: "01401d0b1d37b660".to_string(),
        label: None,
        created_at: chrono::Utc::now(),
    });
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;

    let res = post_timecard(
        &base_url,
        tenant_id,
        vec![timecard_item(1, "01401D0B1D37B660")],
    )
    .await;
    assert_eq!(res.status(), 201);

    assert_eq!(
        tc.card_lookups.lock().unwrap().as_slice(),
        ["01401d0b1d37b660"]
    );
    assert_eq!(frozen_employee_id(&hub, 1), Some(employee_id.to_string()));
}

/// カード未登録でも employees.nfc_id にあれば解決される (免許証 16 桁の経路)
#[tokio::test]
async fn test_timecard_falls_back_to_employee_nfc_id() {
    let (state, hub, tc) = timecard_state();
    let tenant_id = Uuid::new_v4();
    let employee_id = Uuid::new_v4();
    // find_card_data は None のまま = timecard_cards に無い
    *tc.nfc_employee_id.lock().unwrap() = Some(employee_id);
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;

    let res = post_timecard(
        &base_url,
        tenant_id,
        vec![timecard_item(1, "2023060920280513")],
    )
    .await;
    assert_eq!(res.status(), 201);
    assert_eq!(frozen_employee_id(&hub, 1), Some(employee_id.to_string()));
}

/// **未登録カードでも行は作る。** employee_id が入らないだけ。
/// 行さえ残っていれば後から埋め直す backfill が書ける
#[tokio::test]
async fn test_timecard_unknown_card_still_inserts_row_without_employee() {
    let (state, hub, _tc) = timecard_state();
    let tenant_id = Uuid::new_v4();
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;

    let res = post_timecard(&base_url, tenant_id, vec![timecard_item(1, "DEADBEEF")]).await;
    assert_eq!(res.status(), 201);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["inserted"], 1);
    assert_eq!(hub.inserted.lock().unwrap().len(), 1);
    assert_eq!(frozen_employee_id(&hub, 1), None);
}

/// payload に card_id が無い壊れた行も ingest は 201 のまま (解決しないだけ)
#[tokio::test]
async fn test_timecard_missing_card_id_still_inserts_row() {
    let (state, hub, _tc) = timecard_state();
    let tenant_id = Uuid::new_v4();
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;

    let item = serde_json::json!({
        "device_id": "timecard-dev-1",
        "kind": "timecard",
        "seq": 1,
        "payload": { "card_kind": "felica_idm" }
    });
    let res = post_timecard(&base_url, tenant_id, vec![item]).await;
    assert_eq!(res.status(), 201);
    assert_eq!(hub.inserted.lock().unwrap().len(), 1);
    assert_eq!(frozen_employee_id(&hub, 1), None);
}

/// ★ **凍結の本体**: 同じ seq を再送しても、後からカードを付け替えた結果には
/// 差し替わらない。ON CONFLICT DO NOTHING で最初に入った payload が残る
#[tokio::test]
async fn test_timecard_resend_does_not_rewrite_frozen_employee() {
    let (state, hub, tc) = timecard_state();
    let tenant_id = Uuid::new_v4();
    let first = Uuid::new_v4();
    let second = Uuid::new_v4();
    *tc.nfc_employee_id.lock().unwrap() = Some(first);
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;

    let res = post_timecard(
        &base_url,
        tenant_id,
        vec![timecard_item(7, "2023060920280513")],
    )
    .await;
    assert_eq!(res.status(), 201);

    // カードを別人へ付け替えてから、端末が同じ seq を再送する
    *tc.nfc_employee_id.lock().unwrap() = Some(second);
    for _ in 0..2 {
        let res = post_timecard(
            &base_url,
            tenant_id,
            vec![timecard_item(7, "2023060920280513")],
        )
        .await;
        assert_eq!(res.status(), 201);
        let body: serde_json::Value = res.json().await.unwrap();
        assert_eq!(body["inserted"], 0);
        assert_eq!(body["duplicates"], 1);
    }

    assert_eq!(hub.inserted.lock().unwrap().len(), 1);
    assert_eq!(frozen_employee_id(&hub, 7), Some(first.to_string()));
}

/// timecard 以外の kind ではカード照合をしない (混在バッチでも timecard だけ)
#[tokio::test]
async fn test_timecard_ignores_other_kinds() {
    let (state, hub, tc) = timecard_state();
    let tenant_id = Uuid::new_v4();
    let employee_id = Uuid::new_v4();
    *tc.nfc_employee_id.lock().unwrap() = Some(employee_id);
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;

    let res = post_timecard(
        &base_url,
        tenant_id,
        vec![
            measurement(1, "temperature"),
            timecard_item(2, "2023060920280513"),
            measurement(3, "alcohol"),
        ],
    )
    .await;
    assert_eq!(res.status(), 201);

    assert_eq!(tc.card_lookups.lock().unwrap().len(), 1);
    assert_eq!(frozen_employee_id(&hub, 1), None);
    assert_eq!(frozen_employee_id(&hub, 2), Some(employee_id.to_string()));
    assert_eq!(frozen_employee_id(&hub, 3), None);
}

// ---------------------------------------------------------------------------
// ★ 端末が名乗った employee_id は必ず捨てる
//
// 打刻履歴は payload の employee_id を「サーバが解決した社員」として読む。
// 端末の申告を残すと **端末が任意の社員に打刻を付けられる** — device JWT は
// tenant しか名乗れず、その tenant すら introspect 結果から解決する、という
// 既存の不変条件 (plan/standing-devices.md §3.3) が payload 経由で破れる。
// ---------------------------------------------------------------------------

/// 端末が偽の employee_id を載せ、カードは未登録 → **偽値は残らない**
#[tokio::test]
async fn test_client_supplied_employee_id_is_dropped_when_unresolved() {
    let (state, hub, _tc) = timecard_state();
    let tenant_id = Uuid::new_v4();
    let forged = Uuid::new_v4();
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;

    let item = serde_json::json!({
        "device_id": "timecard-dev-1",
        "kind": "timecard",
        "seq": 1,
        "payload": { "card_id": "DEADBEEF", "employee_id": forged.to_string() }
    });
    let res = post_timecard(&base_url, tenant_id, vec![item]).await;
    assert_eq!(res.status(), 201);
    assert_eq!(frozen_employee_id(&hub, 1), None);
}

/// 端末が偽の employee_id を載せ、カードは登録済み → **サーバの解決結果で上書き**
#[tokio::test]
async fn test_client_supplied_employee_id_is_overwritten_by_server() {
    let (state, hub, tc) = timecard_state();
    let tenant_id = Uuid::new_v4();
    let forged = Uuid::new_v4();
    let real = Uuid::new_v4();
    *tc.nfc_employee_id.lock().unwrap() = Some(real);
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;

    let item = serde_json::json!({
        "device_id": "timecard-dev-1",
        "kind": "timecard",
        "seq": 1,
        "payload": { "card_id": "2023060920280513", "employee_id": forged.to_string() }
    });
    let res = post_timecard(&base_url, tenant_id, vec![item]).await;
    assert_eq!(res.status(), 201);
    assert_eq!(frozen_employee_id(&hub, 1), Some(real.to_string()));
}

/// **timecard 以外の kind でも捨てる。** 将来 license 等で同じキーを読むように
/// なったときに、そこだけ素通しになる穴を残さないため
#[tokio::test]
async fn test_client_supplied_employee_id_is_dropped_on_other_kinds() {
    let (state, hub, _tc) = timecard_state();
    let tenant_id = Uuid::new_v4();
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;

    let item = serde_json::json!({
        "device_id": "hub-1",
        "kind": "license",
        "seq": 1,
        "payload": { "nfc_id": "2023060920280513", "employee_id": Uuid::new_v4().to_string() }
    });
    let res = post_timecard(&base_url, tenant_id, vec![item]).await;
    assert_eq!(res.status(), 201);
    assert_eq!(frozen_employee_id(&hub, 1), None);
}
