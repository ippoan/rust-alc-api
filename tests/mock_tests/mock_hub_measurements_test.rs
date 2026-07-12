//! POST /api/hub/measurements (CoreS3 ハブ ingest、Refs #564) の mock テスト。
//!
//! cf-alc-recorder Worker → auth-worker /alc-internal-proxy 経由で
//! X-Internal-Shared-Secret + X-Tenant-ID が付いて届く経路
//! (`internal_shared_secret_router`) を DB なしで固定する。

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
