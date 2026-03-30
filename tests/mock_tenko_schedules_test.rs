mod common;
mod mock_helpers;

use uuid::Uuid;

use std::sync::atomic::Ordering;
use std::sync::Arc;

use mock_helpers::MockTenkoSchedulesRepository;

/// Helper: set up mock AppState and spawn test server with admin JWT.
/// Returns (base_url, auth_header, tenant_id).
async fn setup() -> (String, String, uuid::Uuid) {
    let state = mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = uuid::Uuid::new_v4();
    let base_url = common::spawn_test_server(state).await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let auth_header = format!("Bearer {jwt}");
    (base_url, auth_header, tenant_id)
}

/// Helper: set up with a failing mock for tenko_schedules.
async fn setup_failing() -> (String, String) {
    let mock = Arc::new(MockTenkoSchedulesRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let mut state = mock_helpers::app_state::setup_mock_app_state();
    state.tenko_schedules = mock;
    let tenant_id = uuid::Uuid::new_v4();
    let base_url = common::spawn_test_server(state).await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let auth_header = format!("Bearer {jwt}");
    (base_url, auth_header)
}

/// Helper: set up with return_none mock (get/update/delete return None/false).
async fn setup_not_found() -> (String, String) {
    let mock = Arc::new(MockTenkoSchedulesRepository::default());
    mock.return_none.store(true, Ordering::SeqCst);
    let mut state = mock_helpers::app_state::setup_mock_app_state();
    state.tenko_schedules = mock;
    let tenant_id = uuid::Uuid::new_v4();
    let base_url = common::spawn_test_server(state).await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let auth_header = format!("Bearer {jwt}");
    (base_url, auth_header)
}

fn valid_pre_operation_body() -> serde_json::Value {
    serde_json::json!({
        "employee_id": uuid::Uuid::new_v4(),
        "tenko_type": "pre_operation",
        "responsible_manager_name": "Manager A",
        "scheduled_at": "2026-04-01T08:00:00Z",
        "instruction": "Drive safely"
    })
}

fn valid_post_operation_body() -> serde_json::Value {
    serde_json::json!({
        "employee_id": uuid::Uuid::new_v4(),
        "tenko_type": "post_operation",
        "responsible_manager_name": "Manager B",
        "scheduled_at": "2026-04-01T18:00:00Z"
    })
}

// =========================================================================
// POST /api/tenko/schedules — create_schedule
// =========================================================================

#[tokio::test]
async fn test_create_schedule_pre_operation_success() {
    let (base_url, auth_header, _) = setup().await;
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/tenko/schedules"))
        .header("Authorization", &auth_header)
        .json(&valid_pre_operation_body())
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 201);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["tenko_type"], "pre_operation");
    assert!(!body["id"].as_str().unwrap().is_empty());
}

#[tokio::test]
async fn test_create_schedule_post_operation_success() {
    let (base_url, auth_header, _) = setup().await;
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/tenko/schedules"))
        .header("Authorization", &auth_header)
        .json(&valid_post_operation_body())
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 201);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["tenko_type"], "post_operation");
}

#[tokio::test]
async fn test_create_schedule_invalid_type() {
    let (base_url, auth_header, _) = setup().await;
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/tenko/schedules"))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "employee_id": uuid::Uuid::new_v4(),
            "tenko_type": "invalid_type",
            "responsible_manager_name": "Manager",
            "scheduled_at": "2026-04-01T08:00:00Z",
            "instruction": "Test"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 400);
}

#[tokio::test]
async fn test_create_schedule_pre_operation_missing_instruction() {
    let (base_url, auth_header, _) = setup().await;
    let client = reqwest::Client::new();

    // pre_operation without instruction -> BAD_REQUEST
    let res = client
        .post(format!("{base_url}/api/tenko/schedules"))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "employee_id": uuid::Uuid::new_v4(),
            "tenko_type": "pre_operation",
            "responsible_manager_name": "Manager",
            "scheduled_at": "2026-04-01T08:00:00Z"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 400);
}

#[tokio::test]
async fn test_create_schedule_pre_operation_empty_instruction() {
    let (base_url, auth_header, _) = setup().await;
    let client = reqwest::Client::new();

    // pre_operation with empty instruction -> BAD_REQUEST
    let res = client
        .post(format!("{base_url}/api/tenko/schedules"))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "employee_id": uuid::Uuid::new_v4(),
            "tenko_type": "pre_operation",
            "responsible_manager_name": "Manager",
            "scheduled_at": "2026-04-01T08:00:00Z",
            "instruction": ""
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 400);
}

#[tokio::test]
async fn test_create_schedule_no_auth() {
    let (base_url, _, _) = setup().await;
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/tenko/schedules"))
        .json(&valid_pre_operation_body())
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);
}

#[tokio::test]
async fn test_create_schedule_x_tenant_id_header() {
    let (base_url, _, _) = setup().await;
    let client = reqwest::Client::new();
    let tenant_id = uuid::Uuid::new_v4();

    let res = client
        .post(format!("{base_url}/api/tenko/schedules"))
        .header("X-Tenant-ID", tenant_id.to_string())
        .json(&valid_pre_operation_body())
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 201);
}

#[tokio::test]
async fn test_create_schedule_db_error() {
    let (base_url, auth_header) = setup_failing().await;
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/tenko/schedules"))
        .header("Authorization", &auth_header)
        .json(&valid_pre_operation_body())
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// =========================================================================
// POST /api/tenko/schedules/batch — batch_create_schedules
// =========================================================================

#[tokio::test]
async fn test_batch_create_schedules_success() {
    let (base_url, auth_header, _) = setup().await;
    let client = reqwest::Client::new();

    let emp1 = uuid::Uuid::new_v4();
    let emp2 = uuid::Uuid::new_v4();

    let res = client
        .post(format!("{base_url}/api/tenko/schedules/batch"))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "schedules": [
                {
                    "employee_id": emp1,
                    "tenko_type": "pre_operation",
                    "responsible_manager_name": "Manager A",
                    "scheduled_at": "2026-04-01T08:00:00Z",
                    "instruction": "Instruction 1"
                },
                {
                    "employee_id": emp2,
                    "tenko_type": "post_operation",
                    "responsible_manager_name": "Manager B",
                    "scheduled_at": "2026-04-01T18:00:00Z"
                }
            ]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 201);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body.as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn test_batch_create_schedules_empty() {
    let (base_url, auth_header, _) = setup().await;
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/tenko/schedules/batch"))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({ "schedules": [] }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 400);
}

#[tokio::test]
async fn test_batch_create_schedules_invalid_type_in_batch() {
    let (base_url, auth_header, _) = setup().await;
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/tenko/schedules/batch"))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "schedules": [
                {
                    "employee_id": uuid::Uuid::new_v4(),
                    "tenko_type": "invalid",
                    "responsible_manager_name": "Manager",
                    "scheduled_at": "2026-04-01T08:00:00Z",
                    "instruction": "Test"
                }
            ]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 400);
}

#[tokio::test]
async fn test_batch_create_schedules_pre_op_missing_instruction() {
    let (base_url, auth_header, _) = setup().await;
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/tenko/schedules/batch"))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "schedules": [
                {
                    "employee_id": uuid::Uuid::new_v4(),
                    "tenko_type": "pre_operation",
                    "responsible_manager_name": "Manager",
                    "scheduled_at": "2026-04-01T08:00:00Z"
                }
            ]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 400);
}

#[tokio::test]
async fn test_batch_create_schedules_no_auth() {
    let (base_url, _, _) = setup().await;
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/tenko/schedules/batch"))
        .json(&serde_json::json!({
            "schedules": [{
                "employee_id": uuid::Uuid::new_v4(),
                "tenko_type": "post_operation",
                "responsible_manager_name": "Manager",
                "scheduled_at": "2026-04-01T08:00:00Z"
            }]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);
}

#[tokio::test]
async fn test_batch_create_schedules_db_error() {
    let (base_url, auth_header) = setup_failing().await;
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/tenko/schedules/batch"))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "schedules": [{
                "employee_id": uuid::Uuid::new_v4(),
                "tenko_type": "post_operation",
                "responsible_manager_name": "Manager",
                "scheduled_at": "2026-04-01T08:00:00Z"
            }]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// =========================================================================
// GET /api/tenko/schedules — list_schedules
// =========================================================================

#[tokio::test]
async fn test_list_schedules_success() {
    let (base_url, auth_header, _) = setup().await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/tenko/schedules"))
        .header("Authorization", &auth_header)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert!(body["schedules"].is_array());
    assert_eq!(body["total"], 0);
    assert_eq!(body["page"], 1);
    assert_eq!(body["per_page"], 50);
}

#[tokio::test]
async fn test_list_schedules_with_pagination() {
    let (base_url, auth_header, _) = setup().await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/tenko/schedules?page=2&per_page=10"))
        .header("Authorization", &auth_header)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["page"], 2);
    assert_eq!(body["per_page"], 10);
}

#[tokio::test]
async fn test_list_schedules_per_page_capped_at_100() {
    let (base_url, auth_header, _) = setup().await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/tenko/schedules?per_page=999"))
        .header("Authorization", &auth_header)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["per_page"], 100);
}

#[tokio::test]
async fn test_list_schedules_page_min_1() {
    let (base_url, auth_header, _) = setup().await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/tenko/schedules?page=0"))
        .header("Authorization", &auth_header)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["page"], 1);
}

#[tokio::test]
async fn test_list_schedules_with_filters() {
    let (base_url, auth_header, _) = setup().await;
    let client = reqwest::Client::new();
    let emp_id = uuid::Uuid::new_v4();

    let res = client
        .get(format!(
            "{base_url}/api/tenko/schedules?employee_id={emp_id}&tenko_type=pre_operation&consumed=false&date_from=2026-01-01T00:00:00Z&date_to=2026-12-31T23:59:59Z"
        ))
        .header("Authorization", &auth_header)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
}

#[tokio::test]
async fn test_list_schedules_no_auth() {
    let (base_url, _, _) = setup().await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/tenko/schedules"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);
}

#[tokio::test]
async fn test_list_schedules_x_tenant_id() {
    let (base_url, _, _) = setup().await;
    let client = reqwest::Client::new();
    let tenant_id = uuid::Uuid::new_v4();

    let res = client
        .get(format!("{base_url}/api/tenko/schedules"))
        .header("X-Tenant-ID", tenant_id.to_string())
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
}

#[tokio::test]
async fn test_list_schedules_db_error() {
    let (base_url, auth_header) = setup_failing().await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/tenko/schedules"))
        .header("Authorization", &auth_header)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// =========================================================================
// GET /api/tenko/schedules/{id} — get_schedule
// =========================================================================

#[tokio::test]
async fn test_get_schedule_success() {
    let (base_url, auth_header, _) = setup().await;
    let client = reqwest::Client::new();
    let id = uuid::Uuid::new_v4();

    let res = client
        .get(format!("{base_url}/api/tenko/schedules/{id}"))
        .header("Authorization", &auth_header)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["id"], id.to_string());
}

#[tokio::test]
async fn test_get_schedule_not_found() {
    let (base_url, auth_header) = setup_not_found().await;
    let client = reqwest::Client::new();
    let id = uuid::Uuid::new_v4();

    let res = client
        .get(format!("{base_url}/api/tenko/schedules/{id}"))
        .header("Authorization", &auth_header)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn test_get_schedule_no_auth() {
    let (base_url, _, _) = setup().await;
    let client = reqwest::Client::new();
    let id = uuid::Uuid::new_v4();

    let res = client
        .get(format!("{base_url}/api/tenko/schedules/{id}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);
}

#[tokio::test]
async fn test_get_schedule_db_error() {
    let (base_url, auth_header) = setup_failing().await;
    let client = reqwest::Client::new();
    let id = uuid::Uuid::new_v4();

    let res = client
        .get(format!("{base_url}/api/tenko/schedules/{id}"))
        .header("Authorization", &auth_header)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// =========================================================================
// PUT /api/tenko/schedules/{id} — update_schedule
// =========================================================================

#[tokio::test]
async fn test_update_schedule_success() {
    let (base_url, auth_header, _) = setup().await;
    let client = reqwest::Client::new();
    let id = uuid::Uuid::new_v4();

    let res = client
        .put(format!("{base_url}/api/tenko/schedules/{id}"))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "responsible_manager_name": "New Manager",
            "scheduled_at": "2026-05-01T09:00:00Z",
            "instruction": "Updated instruction"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["id"], id.to_string());
}

#[tokio::test]
async fn test_update_schedule_not_found() {
    let (base_url, auth_header) = setup_not_found().await;
    let client = reqwest::Client::new();
    let id = uuid::Uuid::new_v4();

    let res = client
        .put(format!("{base_url}/api/tenko/schedules/{id}"))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "instruction": "Updated"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn test_update_schedule_no_auth() {
    let (base_url, _, _) = setup().await;
    let client = reqwest::Client::new();
    let id = uuid::Uuid::new_v4();

    let res = client
        .put(format!("{base_url}/api/tenko/schedules/{id}"))
        .json(&serde_json::json!({ "instruction": "X" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);
}

#[tokio::test]
async fn test_update_schedule_db_error() {
    let (base_url, auth_header) = setup_failing().await;
    let client = reqwest::Client::new();
    let id = uuid::Uuid::new_v4();

    let res = client
        .put(format!("{base_url}/api/tenko/schedules/{id}"))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({ "instruction": "X" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// =========================================================================
// DELETE /api/tenko/schedules/{id} — delete_schedule
// =========================================================================

#[tokio::test]
async fn test_delete_schedule_success() {
    let (base_url, auth_header, _) = setup().await;
    let client = reqwest::Client::new();
    let id = uuid::Uuid::new_v4();

    let res = client
        .delete(format!("{base_url}/api/tenko/schedules/{id}"))
        .header("Authorization", &auth_header)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 204);
}

#[tokio::test]
async fn test_delete_schedule_not_found() {
    let (base_url, auth_header) = setup_not_found().await;
    let client = reqwest::Client::new();
    let id = uuid::Uuid::new_v4();

    let res = client
        .delete(format!("{base_url}/api/tenko/schedules/{id}"))
        .header("Authorization", &auth_header)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn test_delete_schedule_no_auth() {
    let (base_url, _, _) = setup().await;
    let client = reqwest::Client::new();
    let id = uuid::Uuid::new_v4();

    let res = client
        .delete(format!("{base_url}/api/tenko/schedules/{id}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);
}

#[tokio::test]
async fn test_delete_schedule_db_error() {
    let (base_url, auth_header) = setup_failing().await;
    let client = reqwest::Client::new();
    let id = uuid::Uuid::new_v4();

    let res = client
        .delete(format!("{base_url}/api/tenko/schedules/{id}"))
        .header("Authorization", &auth_header)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// =========================================================================
// GET /api/tenko/schedules/pending/{employee_id} — get_pending_schedules
// =========================================================================

#[tokio::test]
async fn test_get_pending_schedules_success() {
    let (base_url, auth_header, _) = setup().await;
    let client = reqwest::Client::new();
    let employee_id = uuid::Uuid::new_v4();

    let res = client
        .get(format!(
            "{base_url}/api/tenko/schedules/pending/{employee_id}"
        ))
        .header("Authorization", &auth_header)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert!(body.is_array());
    assert_eq!(body.as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn test_get_pending_schedules_no_auth() {
    let (base_url, _, _) = setup().await;
    let client = reqwest::Client::new();
    let employee_id = uuid::Uuid::new_v4();

    let res = client
        .get(format!(
            "{base_url}/api/tenko/schedules/pending/{employee_id}"
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);
}

#[tokio::test]
async fn test_get_pending_schedules_x_tenant_id() {
    let (base_url, _, _) = setup().await;
    let client = reqwest::Client::new();
    let tenant_id = uuid::Uuid::new_v4();
    let employee_id = uuid::Uuid::new_v4();

    let res = client
        .get(format!(
            "{base_url}/api/tenko/schedules/pending/{employee_id}"
        ))
        .header("X-Tenant-ID", tenant_id.to_string())
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
}

#[tokio::test]
async fn test_get_pending_schedules_db_error() {
    let (base_url, auth_header) = setup_failing().await;
    let client = reqwest::Client::new();
    let employee_id = uuid::Uuid::new_v4();

    let res = client
        .get(format!(
            "{base_url}/api/tenko/schedules/pending/{employee_id}"
        ))
        .header("Authorization", &auth_header)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}
