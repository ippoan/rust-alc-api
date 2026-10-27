use std::sync::atomic::{AtomicBool, Ordering};

use chrono::{DateTime, NaiveDate, Utc};
use uuid::Uuid;

use rust_alc_api::db::models::*;
use rust_alc_api::db::repository::equipment_failures::EquipmentFailuresRepository;
use rust_alc_api::db::repository::health_baselines::HealthBaselinesRepository;
use rust_alc_api::db::repository::hub_measurements::HubMeasurementsRepository;

macro_rules! check_fail {
    ($self:expr) => {
        if $self.fail_next.swap(false, Ordering::SeqCst) {
            return Err(sqlx::Error::RowNotFound);
        }
    };
}

// =============================================================================
// MockDbError — fake sqlx::error::DatabaseError for unique-violation tests
// =============================================================================

#[derive(Debug)]
pub struct MockDbError(pub String);

impl std::fmt::Display for MockDbError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for MockDbError {}

impl sqlx::error::DatabaseError for MockDbError {
    fn message(&self) -> &str {
        &self.0
    }

    fn as_error(&self) -> &(dyn std::error::Error + Send + Sync + 'static) {
        self
    }

    fn as_error_mut(&mut self) -> &mut (dyn std::error::Error + Send + Sync + 'static) {
        self
    }

    fn into_error(self: Box<Self>) -> Box<dyn std::error::Error + Send + Sync + 'static> {
        self
    }

    fn kind(&self) -> sqlx::error::ErrorKind {
        sqlx::error::ErrorKind::UniqueViolation
    }
}

// =============================================================================
// MockEquipmentFailuresRepository
// =============================================================================

pub struct MockEquipmentFailuresRepository {
    pub fail_next: AtomicBool,
    pub get_result: std::sync::Mutex<Option<EquipmentFailure>>,
    pub resolve_result: std::sync::Mutex<Option<EquipmentFailure>>,
    pub csv_data: std::sync::Mutex<Vec<EquipmentFailure>>,
}

impl Default for MockEquipmentFailuresRepository {
    fn default() -> Self {
        Self {
            fail_next: AtomicBool::new(false),
            get_result: std::sync::Mutex::new(None),
            resolve_result: std::sync::Mutex::new(None),
            csv_data: std::sync::Mutex::new(vec![]),
        }
    }
}

#[async_trait::async_trait]
impl EquipmentFailuresRepository for MockEquipmentFailuresRepository {
    async fn create(
        &self,
        _tenant_id: Uuid,
        _input: &CreateEquipmentFailure,
    ) -> Result<EquipmentFailure, sqlx::Error> {
        check_fail!(self);
        Ok(EquipmentFailure {
            id: Uuid::new_v4(),
            tenant_id: _tenant_id,
            failure_type: _input.failure_type.clone(),
            description: _input.description.clone(),
            affected_device: _input.affected_device.clone(),
            detected_at: _input.detected_at.unwrap_or_else(Utc::now),
            detected_by: _input.detected_by.clone(),
            resolved_at: None,
            resolution_notes: None,
            session_id: _input.session_id,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        })
    }

    async fn list(
        &self,
        _tenant_id: Uuid,
        _filter: &EquipmentFailureFilter,
    ) -> Result<EquipmentFailuresResponse, sqlx::Error> {
        check_fail!(self);
        Ok(EquipmentFailuresResponse {
            failures: vec![],
            total: 0,
            page: 1,
            per_page: 50,
        })
    }

    async fn get(
        &self,
        _tenant_id: Uuid,
        _id: Uuid,
    ) -> Result<Option<EquipmentFailure>, sqlx::Error> {
        check_fail!(self);
        Ok(self.get_result.lock().unwrap().clone())
    }

    async fn resolve(
        &self,
        _tenant_id: Uuid,
        _id: Uuid,
        _input: &UpdateEquipmentFailure,
    ) -> Result<Option<EquipmentFailure>, sqlx::Error> {
        check_fail!(self);
        Ok(self.resolve_result.lock().unwrap().clone())
    }

    async fn list_for_csv(
        &self,
        _tenant_id: Uuid,
        _filter: &EquipmentFailureFilter,
    ) -> Result<Vec<EquipmentFailure>, sqlx::Error> {
        check_fail!(self);
        Ok(self.csv_data.lock().unwrap().clone())
    }
}

// =============================================================================
// MockHealthBaselinesRepository
// =============================================================================

pub struct MockHealthBaselinesRepository {
    pub fail_next: AtomicBool,
    pub upsert_result: std::sync::Mutex<Option<EmployeeHealthBaseline>>,
    pub list_result: std::sync::Mutex<Vec<EmployeeHealthBaseline>>,
    pub get_result: std::sync::Mutex<Option<EmployeeHealthBaseline>>,
    pub update_result: std::sync::Mutex<Option<EmployeeHealthBaseline>>,
    pub delete_result: std::sync::Mutex<bool>,
}

impl Default for MockHealthBaselinesRepository {
    fn default() -> Self {
        Self {
            fail_next: AtomicBool::new(false),
            upsert_result: std::sync::Mutex::new(None),
            list_result: std::sync::Mutex::new(vec![]),
            get_result: std::sync::Mutex::new(None),
            update_result: std::sync::Mutex::new(None),
            delete_result: std::sync::Mutex::new(false),
        }
    }
}

#[async_trait::async_trait]
impl HealthBaselinesRepository for MockHealthBaselinesRepository {
    async fn upsert(
        &self,
        _tenant_id: Uuid,
        _body: &CreateHealthBaseline,
    ) -> Result<EmployeeHealthBaseline, sqlx::Error> {
        check_fail!(self);
        let result = self.upsert_result.lock().unwrap().clone();
        match result {
            Some(b) => Ok(b),
            None => Ok(EmployeeHealthBaseline {
                id: Uuid::new_v4(),
                tenant_id: _tenant_id,
                employee_id: _body.employee_id,
                baseline_systolic: _body.baseline_systolic.unwrap_or(120),
                baseline_diastolic: _body.baseline_diastolic.unwrap_or(80),
                baseline_temperature: _body.baseline_temperature.unwrap_or(36.5),
                systolic_tolerance: _body.systolic_tolerance.unwrap_or(10),
                diastolic_tolerance: _body.diastolic_tolerance.unwrap_or(10),
                temperature_tolerance: _body.temperature_tolerance.unwrap_or(0.5),
                measurement_validity_minutes: _body.measurement_validity_minutes.unwrap_or(30),
                created_at: Utc::now(),
                updated_at: Utc::now(),
            }),
        }
    }

    async fn list(&self, _tenant_id: Uuid) -> Result<Vec<EmployeeHealthBaseline>, sqlx::Error> {
        check_fail!(self);
        Ok(self.list_result.lock().unwrap().clone())
    }

    async fn get(
        &self,
        _tenant_id: Uuid,
        _employee_id: Uuid,
    ) -> Result<Option<EmployeeHealthBaseline>, sqlx::Error> {
        check_fail!(self);
        Ok(self.get_result.lock().unwrap().clone())
    }

    async fn update(
        &self,
        _tenant_id: Uuid,
        _employee_id: Uuid,
        _body: &UpdateHealthBaseline,
    ) -> Result<Option<EmployeeHealthBaseline>, sqlx::Error> {
        check_fail!(self);
        Ok(self.update_result.lock().unwrap().clone())
    }

    async fn delete(&self, _tenant_id: Uuid, _employee_id: Uuid) -> Result<bool, sqlx::Error> {
        check_fail!(self);
        Ok(*self.delete_result.lock().unwrap())
    }
}

// =============================================================================
// MockHubMeasurementsRepository (Refs #564)
// =============================================================================

/// insert された item を tenant ごとに記録し、(device_id, seq) 重複は duplicates に数える。
/// 本物は `ON CONFLICT (tenant_id, device_id, seq, is_dev) DO NOTHING` で、dev端末の軸
/// (`is_dev`) が違えば別の行になる。mock は軸を持たない (= 1 つの軸の中の再送だけを写す)。
/// 軸をまたぐ振る舞いは実 DB の `tests/device_dev_axis_test.rs` が固定する。
/// `list` 用に行そのものも積む (Refs #592)。
pub struct MockHubMeasurementsRepository {
    pub fail_next: AtomicBool,
    pub inserted: std::sync::Mutex<Vec<(Uuid, HubMeasurementCreate)>>,
    /// `list` が返す行。created_at は insert 順に 1 秒ずつ進めるので、
    /// created_at DESC = 挿入の逆順になり、順序をテストで固定できる。
    rows: std::sync::Mutex<Vec<HubMeasurement>>,
    seen: std::sync::Mutex<std::collections::HashSet<(Uuid, String, i64)>>,
}

/// mock の created_at 起点 (2026-07-12T06:00:00Z 相当)。実 DB の now() と違い
/// 決定的にしたいので固定値から 1 秒刻みで進める。
fn mock_hub_created_at(nth: usize) -> DateTime<Utc> {
    DateTime::from_timestamp(1_752_300_000 + nth as i64, 0).expect("valid timestamp")
}

impl Default for MockHubMeasurementsRepository {
    fn default() -> Self {
        Self {
            fail_next: AtomicBool::new(false),
            inserted: std::sync::Mutex::new(vec![]),
            rows: std::sync::Mutex::new(vec![]),
            seen: std::sync::Mutex::new(std::collections::HashSet::new()),
        }
    }
}

#[async_trait::async_trait]
impl HubMeasurementsRepository for MockHubMeasurementsRepository {
    async fn insert_batch(
        &self,
        tenant_id: Uuid,
        items: &[HubMeasurementCreate],
    ) -> Result<HubMeasurementsIngestResponse, sqlx::Error> {
        check_fail!(self);
        let mut seen = self.seen.lock().unwrap();
        let mut inserted_log = self.inserted.lock().unwrap();
        let mut rows = self.rows.lock().unwrap();
        let mut inserted: i64 = 0;
        for item in items {
            let is_new = seen.insert((tenant_id, item.device_id.clone(), item.seq));
            if is_new {
                inserted_log.push((tenant_id, item.clone()));
                let created_at = mock_hub_created_at(rows.len());
                rows.push(HubMeasurement {
                    id: Uuid::new_v4(),
                    tenant_id,
                    device_id: item.device_id.clone(),
                    kind: item.kind.clone(),
                    payload: item.payload.clone(),
                    seq: item.seq,
                    session_id: item.session_id.clone(),
                    via_device_id: item.via_device_id.clone(),
                    machine_id: item.device_id.clone(),
                    recorded_at: item
                        .recorded_at_ms
                        .and_then(DateTime::from_timestamp_millis),
                    created_at,
                });
            }
            if is_new {
                inserted += 1;
            }
        }
        Ok(HubMeasurementsIngestResponse {
            inserted,
            duplicates: items.len() as i64 - inserted,
        })
    }

    async fn list(
        &self,
        tenant_id: Uuid,
        filter: &HubMeasurementFilter,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<HubMeasurement>, sqlx::Error> {
        check_fail!(self);
        let rows = self.rows.lock().unwrap();
        let mut matched: Vec<HubMeasurement> = rows
            .iter()
            .filter(|r| r.tenant_id == tenant_id)
            .filter(|r| filter.device_id.as_ref().is_none_or(|d| *d == r.device_id))
            .filter(|r| filter.kind.as_ref().is_none_or(|k| *k == r.kind))
            .filter(|r| {
                filter
                    .session_id
                    .as_ref()
                    .is_none_or(|sid| r.session_id.as_ref() == Some(sid))
            })
            .filter(|r| filter.from.is_none_or(|f| r.created_at >= f))
            .filter(|r| filter.to.is_none_or(|t| r.created_at <= t))
            .cloned()
            .collect();
        matched.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        // 本物の repo と同じく has_more 判定用に 1 件多く返す。
        Ok(matched
            .into_iter()
            .skip(offset.max(0) as usize)
            .take(limit as usize + 1)
            .collect())
    }
}
