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
