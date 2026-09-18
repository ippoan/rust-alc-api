use uuid::Uuid;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use chrono::Utc;

use crate::mock_helpers::app_state::setup_mock_app_state;
use rust_alc_api::db::models::*;
use rust_alc_api::db::repository::driver_info::DriverInfoRepository;
use rust_alc_api::db::repository::driver_info::{
    DailyInspectionSummary, InstructionSummary, MaintenanceRecordSummary, MeasurementSummary,
};

// ============================================================
// Mock that returns a valid Employee (success path)
// ============================================================

fn make_employee(tenant_id: Uuid, employee_id: Uuid) -> Employee {
    Employee {
        id: employee_id,
        tenant_id,
        code: Some("E001".to_string()),
        nfc_id: None,
        name: "Test Driver".to_string(),
        face_photo_url: None,
        face_embedding: None,
        face_embedding_at: None,
        face_model_version: None,
        face_approval_status: "none".to_string(),
        face_approved_by: None,
        face_approved_at: None,
        license_issue_date: None,
        license_expiry_date: None,
        role: vec!["driver".to_string()],
        created_at: Utc::now(),
        updated_at: Utc::now(),
        deleted_at: None,
    }
}

/// Mock that returns an employee (success case) for get_employee,
/// and can optionally fail on a specific method.
struct MockDriverInfoWithEmployee {
    tenant_id: Uuid,
    employee_id: Uuid,
    fail_get_employee: AtomicBool,
    fail_get_health_baseline: AtomicBool,
    fail_get_recent_measurements: AtomicBool,
    maintenance_records: std::sync::Mutex<Vec<MaintenanceRecordSummary>>,
}

impl MockDriverInfoWithEmployee {
    fn new(tenant_id: Uuid, employee_id: Uuid) -> Self {
        Self {
            tenant_id,
            employee_id,
            fail_get_employee: AtomicBool::new(false),
            fail_get_health_baseline: AtomicBool::new(false),
            fail_get_recent_measurements: AtomicBool::new(false),
            maintenance_records: std::sync::Mutex::new(vec![]),
        }
    }
}

#[async_trait::async_trait]
impl DriverInfoRepository for MockDriverInfoWithEmployee {
    async fn get_employee(
        &self,
        _tenant_id: Uuid,
        employee_id: Uuid,
    ) -> Result<Option<Employee>, sqlx::Error> {
        if self.fail_get_employee.swap(false, Ordering::SeqCst) {
            return Err(sqlx::Error::RowNotFound);
        }
        if employee_id == self.employee_id {
            Ok(Some(make_employee(self.tenant_id, self.employee_id)))
        } else {
            Ok(None)
        }
    }

    async fn get_health_baseline(
        &self,
        _tenant_id: Uuid,
        _employee_id: Uuid,
    ) -> Result<Option<EmployeeHealthBaseline>, sqlx::Error> {
        if self.fail_get_health_baseline.swap(false, Ordering::SeqCst) {
            return Err(sqlx::Error::RowNotFound);
        }
        Ok(None)
    }

    async fn get_recent_measurements(
        &self,
        _tenant_id: Uuid,
        _employee_id: Uuid,
    ) -> Result<Vec<MeasurementSummary>, sqlx::Error> {
        if self
            .fail_get_recent_measurements
            .swap(false, Ordering::SeqCst)
        {
            return Err(sqlx::Error::RowNotFound);
        }
        Ok(vec![])
    }

    async fn get_working_hours(
        &self,
        _tenant_id: Uuid,
        _employee_id: Uuid,
    ) -> Result<Vec<DtakoDailyWorkHours>, sqlx::Error> {
        Ok(vec![])
    }

    async fn get_past_instructions(
        &self,
        _tenant_id: Uuid,
        _employee_id: Uuid,
    ) -> Result<Vec<InstructionSummary>, sqlx::Error> {
        Ok(vec![])
    }

    async fn get_carrying_items(&self, _tenant_id: Uuid) -> Result<Vec<CarryingItem>, sqlx::Error> {
        Ok(vec![])
    }

    async fn get_past_tenko_records(
        &self,
        _tenant_id: Uuid,
        _employee_id: Uuid,
    ) -> Result<Vec<TenkoRecord>, sqlx::Error> {
        Ok(vec![])
    }

    async fn get_recent_daily_inspections(
        &self,
        _tenant_id: Uuid,
        _employee_id: Uuid,
    ) -> Result<Vec<DailyInspectionSummary>, sqlx::Error> {
        Ok(vec![])
    }

    async fn get_equipment_failures(
        &self,
        _tenant_id: Uuid,
    ) -> Result<Vec<EquipmentFailure>, sqlx::Error> {
        Ok(vec![])
    }

    async fn get_recent_maintenance_records(
        &self,
        _tenant_id: Uuid,
        _employee_id: Uuid,
    ) -> Result<Vec<MaintenanceRecordSummary>, sqlx::Error> {
        Ok(self.maintenance_records.lock().unwrap().clone())
    }
}

// ============================================================
// Helper: build AppState with custom driver_info mock
// ============================================================

async fn setup_state_with_employee(
    tenant_id: Uuid,
    employee_id: Uuid,
) -> (rust_alc_api::AppState, alc_tenko::TenkoState) {
    let state = setup_mock_app_state();
    let mut tenko_state = crate::mock_helpers::app_state::setup_mock_tenko_state();
    tenko_state.driver_info = Arc::new(MockDriverInfoWithEmployee::new(tenant_id, employee_id));
    (state, tenko_state)
}

// ============================================================
// Tests
// ============================================================

/// GET /api/tenko/driver-info/{employee_id} — success: returns 200 with DriverInfo
#[tokio::test]
async fn test_get_driver_info_success() {
    let tenant_id = Uuid::new_v4();
    let employee_id = Uuid::new_v4();
    let (state, tenko_state) = setup_state_with_employee(tenant_id, employee_id).await;
    let base_url =
        crate::mock_helpers::app_state::spawn_mock_server_with_tenko(state, tenko_state.clone())
            .await;

    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();
    let res = client
        .get(format!("{base_url}/api/tenko/driver-info/{employee_id}"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["employee"]["name"], "Test Driver");
    assert!(body["recent_measurements"].as_array().unwrap().is_empty());
    assert!(body["working_hours"].as_array().unwrap().is_empty());
    assert!(body["past_instructions"].as_array().unwrap().is_empty());
    assert!(body["carrying_items"].as_array().unwrap().is_empty());
    assert!(body["past_tenko_records"].as_array().unwrap().is_empty());
    assert!(body["recent_daily_inspections"]
        .as_array()
        .unwrap()
        .is_empty());
    assert!(body["equipment_failures"].as_array().unwrap().is_empty());
    assert!(body["recent_maintenance_records"]
        .as_array()
        .unwrap()
        .is_empty());
    assert!(body["health_baseline"].is_null());
}

/// GET /api/tenko/driver-info/{employee_id} — carins 未紐づけ等で整備記録が
/// 解決しない場合、404/500 にならず recent_maintenance_records が空配列で返る
/// (repository 側は unwrap_or_default() の作法。Refs #651)
#[tokio::test]
async fn test_get_driver_info_maintenance_unresolved_returns_empty() {
    let tenant_id = Uuid::new_v4();
    let employee_id = Uuid::new_v4();
    let (state, tenko_state) = setup_state_with_employee(tenant_id, employee_id).await;
    let base_url =
        crate::mock_helpers::app_state::spawn_mock_server_with_tenko(state, tenko_state.clone())
            .await;

    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();
    let res = client
        .get(format!("{base_url}/api/tenko/driver-info/{employee_id}"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert!(body["recent_maintenance_records"]
        .as_array()
        .unwrap()
        .is_empty());
}

/// GET /api/tenko/driver-info/{employee_id} — 整備記録が解決した場合、
/// recent_maintenance_records に内容がそのまま乗る (JSON へのフィールド名・型の
/// マッピングを固定する。Refs #651)
#[tokio::test]
async fn test_get_driver_info_maintenance_records_present() {
    let tenant_id = Uuid::new_v4();
    let employee_id = Uuid::new_v4();
    let mock = Arc::new(MockDriverInfoWithEmployee::new(tenant_id, employee_id));
    *mock.maintenance_records.lock().unwrap() = vec![MaintenanceRecordSummary {
        id: Uuid::new_v4(),
        vehicle_id: Uuid::new_v4(),
        category_name: "定期点検".to_string(),
        performed_on: chrono::NaiveDate::from_ymd_opt(2026, 9, 1).unwrap(),
        odometer_km: Some(12345),
        vendor: Some("Test Motors".to_string()),
        description: Some("オイル交換".to_string()),
        cost: Some("5500.00".to_string()),
        next_due_on: chrono::NaiveDate::from_ymd_opt(2026, 12, 1),
    }];

    let state = setup_mock_app_state();
    let mut tenko_state = crate::mock_helpers::app_state::setup_mock_tenko_state();
    tenko_state.driver_info = mock;
    let base_url =
        crate::mock_helpers::app_state::spawn_mock_server_with_tenko(state, tenko_state.clone())
            .await;

    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();
    let res = client
        .get(format!("{base_url}/api/tenko/driver-info/{employee_id}"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    let records = body["recent_maintenance_records"].as_array().unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["category_name"], "定期点検");
    assert_eq!(records[0]["vendor"], "Test Motors");
    assert_eq!(records[0]["cost"], "5500.00");
    assert_eq!(records[0]["performed_on"], "2026-09-01");
}

/// GET /api/tenko/driver-info/{employee_id} — employee not found → 404
#[tokio::test]
async fn test_get_driver_info_employee_not_found() {
    let tenant_id = Uuid::new_v4();
    let known_employee_id = Uuid::new_v4();
    let unknown_employee_id = Uuid::new_v4();
    let (state, tenko_state) = setup_state_with_employee(tenant_id, known_employee_id).await;
    let base_url =
        crate::mock_helpers::app_state::spawn_mock_server_with_tenko(state, tenko_state.clone())
            .await;

    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();
    let res = client
        .get(format!(
            "{base_url}/api/tenko/driver-info/{unknown_employee_id}"
        ))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 404);
}

/// GET /api/tenko/driver-info/{employee_id} — no auth → 401
#[tokio::test]
async fn test_get_driver_info_no_auth() {
    let state = setup_mock_app_state();
    let tenko_state = crate::mock_helpers::app_state::setup_mock_tenko_state();
    let base_url =
        crate::mock_helpers::app_state::spawn_mock_server_with_tenko(state, tenko_state.clone())
            .await;

    let employee_id = Uuid::new_v4();
    let client = reqwest::Client::new();
    let res = client
        .get(format!("{base_url}/api/tenko/driver-info/{employee_id}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 401);
}

/// GET /api/tenko/driver-info/{employee_id} — X-Tenant-ID header (kiosk mode) → 200
#[tokio::test]
async fn test_get_driver_info_with_tenant_header() {
    let tenant_id = Uuid::new_v4();
    let employee_id = Uuid::new_v4();
    let (state, tenko_state) = setup_state_with_employee(tenant_id, employee_id).await;
    let base_url =
        crate::mock_helpers::app_state::spawn_mock_server_with_tenko(state, tenko_state.clone())
            .await;

    let client = reqwest::Client::new();
    let res = client
        .get(format!("{base_url}/api/tenko/driver-info/{employee_id}"))
        .header("X-Tenant-ID", tenant_id.to_string())
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["employee"]["name"], "Test Driver");
}

/// GET /api/tenko/driver-info/{employee_id} — DB error on get_employee → 500
#[tokio::test]
async fn test_get_driver_info_db_error_get_employee() {
    let tenant_id = Uuid::new_v4();
    let employee_id = Uuid::new_v4();
    let mock = Arc::new(MockDriverInfoWithEmployee::new(tenant_id, employee_id));
    mock.fail_get_employee.store(true, Ordering::SeqCst);

    let state = setup_mock_app_state();
    let mut tenko_state = crate::mock_helpers::app_state::setup_mock_tenko_state();
    tenko_state.driver_info = mock;
    let base_url =
        crate::mock_helpers::app_state::spawn_mock_server_with_tenko(state, tenko_state.clone())
            .await;

    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();
    let res = client
        .get(format!("{base_url}/api/tenko/driver-info/{employee_id}"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

/// GET /api/tenko/driver-info/{employee_id} — DB error on get_health_baseline → 500
#[tokio::test]
async fn test_get_driver_info_db_error_get_health_baseline() {
    let tenant_id = Uuid::new_v4();
    let employee_id = Uuid::new_v4();
    let mock = Arc::new(MockDriverInfoWithEmployee::new(tenant_id, employee_id));
    mock.fail_get_health_baseline.store(true, Ordering::SeqCst);

    let state = setup_mock_app_state();
    let mut tenko_state = crate::mock_helpers::app_state::setup_mock_tenko_state();
    tenko_state.driver_info = mock;
    let base_url =
        crate::mock_helpers::app_state::spawn_mock_server_with_tenko(state, tenko_state.clone())
            .await;

    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();
    let res = client
        .get(format!("{base_url}/api/tenko/driver-info/{employee_id}"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

/// GET /api/tenko/driver-info/{employee_id} — DB error on get_recent_measurements → 500
#[tokio::test]
async fn test_get_driver_info_db_error_get_recent_measurements() {
    let tenant_id = Uuid::new_v4();
    let employee_id = Uuid::new_v4();
    let mock = Arc::new(MockDriverInfoWithEmployee::new(tenant_id, employee_id));
    mock.fail_get_recent_measurements
        .store(true, Ordering::SeqCst);

    let state = setup_mock_app_state();
    let mut tenko_state = crate::mock_helpers::app_state::setup_mock_tenko_state();
    tenko_state.driver_info = mock;
    let base_url =
        crate::mock_helpers::app_state::spawn_mock_server_with_tenko(state, tenko_state.clone())
            .await;

    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();
    let res = client
        .get(format!("{base_url}/api/tenko/driver-info/{employee_id}"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

/// GET /api/tenko/driver-info/{invalid_uuid} — invalid UUID path → 400
#[tokio::test]
async fn test_get_driver_info_invalid_uuid() {
    let tenant_id = Uuid::new_v4();
    let state = setup_mock_app_state();
    let tenko_state = crate::mock_helpers::app_state::setup_mock_tenko_state();
    let base_url =
        crate::mock_helpers::app_state::spawn_mock_server_with_tenko(state, tenko_state.clone())
            .await;

    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();
    let res = client
        .get(format!("{base_url}/api/tenko/driver-info/not-a-uuid"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 400);
}

/// GET /api/tenko/driver-info/{employee_id} — viewer role can access (tenant-scoped, not admin-only)
#[tokio::test]
async fn test_get_driver_info_viewer_role() {
    let tenant_id = Uuid::new_v4();
    let employee_id = Uuid::new_v4();
    let (state, tenko_state) = setup_state_with_employee(tenant_id, employee_id).await;
    let base_url =
        crate::mock_helpers::app_state::spawn_mock_server_with_tenko(state, tenko_state.clone())
            .await;

    let jwt = crate::common::create_test_jwt(tenant_id, "viewer");
    let client = reqwest::Client::new();
    let res = client
        .get(format!("{base_url}/api/tenko/driver-info/{employee_id}"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
}
