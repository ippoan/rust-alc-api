use uuid::Uuid;

use std::sync::atomic::Ordering;
use std::sync::Arc;

use chrono::Utc;
use serde_json::Value;

use rust_alc_api::db::models::NfcTag;
use rust_alc_api::db::repository::nfc_tags::NfcTagRepository;

// ============================================================
// Helper: spawn server with a given NfcTag mock
// ============================================================

async fn spawn_with_mock(mock: Arc<dyn NfcTagRepository>) -> (String, String) {
    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");

    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    state.nfc_tags = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    (base_url, jwt)
}

fn auth(jwt: &str) -> String {
    format!("Bearer {jwt}")
}

fn make_tag(nfc_uuid: &str, car_inspection_id: i32) -> NfcTag {
    NfcTag {
        id: 1,
        nfc_uuid: nfc_uuid.to_string(),
        car_inspection_id,
        created_at: Utc::now(),
    }
}

// ============================================================
// SearchByUuidFailOnInspection: succeeds on search_by_uuid, fails on get_car_inspection_json
// ============================================================

struct SearchByUuidFailOnInspection;

#[async_trait::async_trait]
impl NfcTagRepository for SearchByUuidFailOnInspection {
    async fn search_by_uuid(
        &self,
        _tenant_id: Uuid,
        nfc_uuid: &str,
    ) -> Result<Option<NfcTag>, sqlx::Error> {
        Ok(Some(make_tag(nfc_uuid, 42)))
    }

    async fn get_car_inspection_json(
        &self,
        _tenant_id: Uuid,
        _car_inspection_id: i32,
    ) -> Result<Option<serde_json::Value>, sqlx::Error> {
        Err(sqlx::Error::RowNotFound)
    }

    async fn list(&self, _: Uuid, _: Option<i32>) -> Result<Vec<NfcTag>, sqlx::Error> {
        unreachable!()
    }

    async fn register(&self, _: Uuid, _: &str, _: i32) -> Result<NfcTag, sqlx::Error> {
        unreachable!()
    }

    async fn delete(&self, _: Uuid, _: &str) -> Result<bool, sqlx::Error> {
        unreachable!()
    }
}

// ============================================================
// search_by_uuid: GET /api/nfc-tags/search?uuid=xxx
// ============================================================

#[tokio::test]
async fn test_search_by_uuid_success() {
    let mock = Arc::new(crate::mock_helpers::MockNfcTagRepository::default());
    *mock.tag_data.lock().unwrap() = Some(make_tag("aabb0011", 42));
    *mock.car_inspection_json.lock().unwrap() =
        Some(serde_json::json!({"id": 42, "EntryNoCarNo": "ABC-123"}));

    let (base_url, jwt) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/nfc-tags/search?uuid=aabb0011"))
        .header("Authorization", auth(&jwt))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    let body: Value = res.json().await.unwrap();
    assert_eq!(body["nfc_tag"]["nfcUuid"], "aabb0011");
    assert_eq!(body["nfc_tag"]["carInspectionId"], 42);
    assert_eq!(body["car_inspection"]["EntryNoCarNo"], "ABC-123");
}

#[tokio::test]
async fn test_search_by_uuid_normalizes_colons() {
    // UUID with colons like "AA:BB:00:11" should be normalized to "aabb0011"
    let mock = Arc::new(crate::mock_helpers::MockNfcTagRepository::default());
    *mock.tag_data.lock().unwrap() = Some(make_tag("aabb0011", 10));

    let (base_url, jwt) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/nfc-tags/search?uuid=AA:BB:00:11"))
        .header("Authorization", auth(&jwt))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    let body: Value = res.json().await.unwrap();
    assert_eq!(body["nfc_tag"]["nfcUuid"], "aabb0011");
}

#[tokio::test]
async fn test_search_by_uuid_car_inspection_null() {
    // Tag found but no car_inspection JSON (returns null)
    let mock = Arc::new(crate::mock_helpers::MockNfcTagRepository::default());
    *mock.tag_data.lock().unwrap() = Some(make_tag("aabb0011", 99));
    // car_inspection_json stays None

    let (base_url, jwt) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/nfc-tags/search?uuid=aabb0011"))
        .header("Authorization", auth(&jwt))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    let body: Value = res.json().await.unwrap();
    assert!(body["car_inspection"].is_null());
}

#[tokio::test]
async fn test_search_by_uuid_not_found() {
    // Default mock returns None for search_by_uuid
    let mock = Arc::new(crate::mock_helpers::MockNfcTagRepository::default());
    let (base_url, jwt) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/nfc-tags/search?uuid=nonexistent"))
        .header("Authorization", auth(&jwt))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn test_search_by_uuid_db_error() {
    let mock = Arc::new(crate::mock_helpers::MockNfcTagRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let (base_url, jwt) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/nfc-tags/search?uuid=abc"))
        .header("Authorization", auth(&jwt))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

#[tokio::test]
async fn test_search_by_uuid_car_inspection_db_error() {
    // search_by_uuid succeeds but get_car_inspection_json fails
    let mock = Arc::new(SearchByUuidFailOnInspection);
    let (base_url, jwt) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/nfc-tags/search?uuid=aabb0011"))
        .header("Authorization", auth(&jwt))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// ============================================================
// list_tags: GET /api/nfc-tags
// ============================================================

#[tokio::test]
async fn test_list_tags_success_empty() {
    let mock = Arc::new(crate::mock_helpers::MockNfcTagRepository::default());
    let (base_url, jwt) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/nfc-tags"))
        .header("Authorization", auth(&jwt))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    let body: Vec<Value> = res.json().await.unwrap();
    assert!(body.is_empty());
}

#[tokio::test]
async fn test_list_tags_success_with_data() {
    let mock = Arc::new(crate::mock_helpers::MockNfcTagRepository::default());
    *mock.tag_data.lock().unwrap() = Some(make_tag("aabb0011", 5));
    let (base_url, jwt) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/nfc-tags"))
        .header("Authorization", auth(&jwt))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    let body: Vec<Value> = res.json().await.unwrap();
    assert_eq!(body.len(), 1);
    assert_eq!(body[0]["nfcUuid"], "aabb0011");
}

#[tokio::test]
async fn test_list_tags_with_car_inspection_id_filter() {
    let mock = Arc::new(crate::mock_helpers::MockNfcTagRepository::default());
    *mock.tag_data.lock().unwrap() = Some(make_tag("cc0022", 7));
    let (base_url, jwt) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/nfc-tags?car_inspection_id=7"))
        .header("Authorization", auth(&jwt))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    let body: Vec<Value> = res.json().await.unwrap();
    assert_eq!(body.len(), 1);
}

#[tokio::test]
async fn test_list_tags_db_error() {
    let mock = Arc::new(crate::mock_helpers::MockNfcTagRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let (base_url, jwt) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/nfc-tags"))
        .header("Authorization", auth(&jwt))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// ============================================================
// register_tag: POST /api/nfc-tags
// ============================================================

#[tokio::test]
async fn test_register_tag_success() {
    let mock = Arc::new(crate::mock_helpers::MockNfcTagRepository::default());
    let (base_url, jwt) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/nfc-tags"))
        .header("Authorization", auth(&jwt))
        .json(&serde_json::json!({
            "nfc_uuid": "DD:EE:FF:00",
            "car_inspection_id": 123
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 201);

    let body: Value = res.json().await.unwrap();
    // UUID should be normalized: lowercase, colons removed
    assert_eq!(body["nfcUuid"], "ddeeff00");
    assert_eq!(body["carInspectionId"], 123);
}

#[tokio::test]
async fn test_register_tag_normalizes_uuid() {
    let mock = Arc::new(crate::mock_helpers::MockNfcTagRepository::default());
    let (base_url, jwt) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/nfc-tags"))
        .header("Authorization", auth(&jwt))
        .json(&serde_json::json!({
            "nfc_uuid": "AA:BB:CC:DD",
            "car_inspection_id": 1
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 201);

    let body: Value = res.json().await.unwrap();
    assert_eq!(body["nfcUuid"], "aabbccdd");
}

#[tokio::test]
async fn test_register_tag_db_error() {
    let mock = Arc::new(crate::mock_helpers::MockNfcTagRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let (base_url, jwt) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/nfc-tags"))
        .header("Authorization", auth(&jwt))
        .json(&serde_json::json!({
            "nfc_uuid": "aabb0011",
            "car_inspection_id": 1
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// ============================================================
// delete_tag: DELETE /api/nfc-tags/{nfc_uuid}
// ============================================================

#[tokio::test]
async fn test_delete_tag_success() {
    let mock = Arc::new(crate::mock_helpers::MockNfcTagRepository::default());
    mock.delete_returns_true.store(true, Ordering::SeqCst);
    let (base_url, jwt) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    let res = client
        .delete(format!("{base_url}/api/nfc-tags/aabb0011"))
        .header("Authorization", auth(&jwt))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 204);
}

#[tokio::test]
async fn test_delete_tag_normalizes_uuid() {
    let mock = Arc::new(crate::mock_helpers::MockNfcTagRepository::default());
    mock.delete_returns_true.store(true, Ordering::SeqCst);
    let (base_url, jwt) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    // Colons and uppercase in path should be normalized
    let res = client
        .delete(format!("{base_url}/api/nfc-tags/AA:BB:00:11"))
        .header("Authorization", auth(&jwt))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 204);
}

#[tokio::test]
async fn test_delete_tag_not_found() {
    // Default mock returns Ok(false) from delete
    let mock = Arc::new(crate::mock_helpers::MockNfcTagRepository::default());
    let (base_url, jwt) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    let res = client
        .delete(format!("{base_url}/api/nfc-tags/nonexistent"))
        .header("Authorization", auth(&jwt))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn test_delete_tag_db_error() {
    let mock = Arc::new(crate::mock_helpers::MockNfcTagRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let (base_url, jwt) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    let res = client
        .delete(format!("{base_url}/api/nfc-tags/aabb0011"))
        .header("Authorization", auth(&jwt))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// ============================================================
// Auth: no JWT -> 401
// ============================================================

#[tokio::test]
async fn test_no_auth_returns_401() {
    let mock = Arc::new(crate::mock_helpers::MockNfcTagRepository::default());
    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    state.nfc_tags = mock;
    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();

    // GET /nfc-tags
    let res = client
        .get(format!("{base_url}/api/nfc-tags"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);

    // POST /nfc-tags
    let res = client
        .post(format!("{base_url}/api/nfc-tags"))
        .json(&serde_json::json!({
            "nfc_uuid": "aabb",
            "car_inspection_id": 1
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);

    // GET /nfc-tags/search
    let res = client
        .get(format!("{base_url}/api/nfc-tags/search?uuid=abc"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);

    // DELETE /nfc-tags/{uuid}
    let res = client
        .delete(format!("{base_url}/api/nfc-tags/aabb0011"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);
}
