use std::sync::atomic::{AtomicBool, Ordering};

use chrono::{DateTime, NaiveDate, Utc};
use uuid::Uuid;

use rust_alc_api::db::models::*;
use rust_alc_api::db::repository::dtako_event_classifications::DtakoEventClassificationsRepository;
use rust_alc_api::db::repository::dtako_operations::{
    DtakoOperationsRepository, OperationChangeRow,
};
use rust_alc_api::db::repository::dtako_restraint_report::{
    DailyWorkHoursRow, DtakoRestraintReportRepository, OpTimesRow, SegmentRow,
};
use rust_alc_api::db::repository::dtako_restraint_report_pdf::{
    DtakoRestraintReportPdfRepository, PdfDriver,
};
use rust_alc_api::db::repository::dtako_scraper::DtakoScraperRepository;
use rust_alc_api::db::repository::dtako_tickets::DtakoTicketsRepository;
use rust_alc_api::db::repository::dtako_vehicles::DtakoVehiclesRepository;
use rust_alc_api::db::repository::dtako_work_times::{DtakoWorkTimesRepository, WorkTimeItem};
use rust_alc_api::db::repository::dtako_y_time_export::{
    DtakoYTimeExportRepository, YTimeExportOperation,
};
use rust_alc_api::db::repository::equipment_failures::EquipmentFailuresRepository;
use rust_alc_api::db::repository::health_baselines::HealthBaselinesRepository;
use rust_alc_api::db::repository::hub_measurements::HubMeasurementsRepository;
use rust_alc_api::routes::dtako_scraper::ScrapeHistoryItem;

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
// MockDtakoEventClassificationsRepository
// =============================================================================

pub struct MockDtakoEventClassificationsRepository {
    pub fail_next: AtomicBool,
    pub update_result: std::sync::Mutex<Option<DtakoEventClassification>>,
}

impl Default for MockDtakoEventClassificationsRepository {
    fn default() -> Self {
        Self {
            fail_next: AtomicBool::new(false),
            update_result: std::sync::Mutex::new(None),
        }
    }
}

#[async_trait::async_trait]
impl DtakoEventClassificationsRepository for MockDtakoEventClassificationsRepository {
    async fn list(&self, _tenant_id: Uuid) -> Result<Vec<DtakoEventClassification>, sqlx::Error> {
        check_fail!(self);
        Ok(vec![])
    }

    async fn update(
        &self,
        _tenant_id: Uuid,
        _id: Uuid,
        _classification: &str,
    ) -> Result<Option<DtakoEventClassification>, sqlx::Error> {
        check_fail!(self);
        Ok(self.update_result.lock().unwrap().clone())
    }
}

// =============================================================================
// MockDtakoOperationsRepository
// =============================================================================

pub struct MockDtakoOperationsRepository {
    pub fail_next: AtomicBool,
    pub calendar_dates_result: std::sync::Mutex<Vec<(NaiveDate, i64)>>,
    pub get_result: std::sync::Mutex<Vec<DtakoOperation>>,
    pub delete_rows_affected: std::sync::Mutex<u64>,
    pub operation_changes: std::sync::Mutex<Vec<OperationChangeRow>>,
    pub recording_since: std::sync::Mutex<Option<DateTime<Utc>>>,
    /// `operation_changes_recording_since` だけを失敗させる (list の後段のエラー経路用)
    pub fail_recording_since: AtomicBool,
}

impl Default for MockDtakoOperationsRepository {
    fn default() -> Self {
        Self {
            fail_next: AtomicBool::new(false),
            calendar_dates_result: std::sync::Mutex::new(vec![]),
            get_result: std::sync::Mutex::new(vec![]),
            delete_rows_affected: std::sync::Mutex::new(0),
            operation_changes: std::sync::Mutex::new(vec![]),
            recording_since: std::sync::Mutex::new(None),
            fail_recording_since: AtomicBool::new(false),
        }
    }
}

#[async_trait::async_trait]
impl DtakoOperationsRepository for MockDtakoOperationsRepository {
    async fn calendar_dates(
        &self,
        _tenant_id: Uuid,
        _date_from: NaiveDate,
        _date_to: NaiveDate,
    ) -> Result<Vec<(NaiveDate, i64)>, sqlx::Error> {
        check_fail!(self);
        Ok(self.calendar_dates_result.lock().unwrap().clone())
    }

    async fn list(
        &self,
        _tenant_id: Uuid,
        _filter: &DtakoOperationFilter,
    ) -> Result<DtakoOperationsResponse, sqlx::Error> {
        check_fail!(self);
        Ok(DtakoOperationsResponse {
            operations: vec![],
            total: 0,
            page: 1,
            per_page: 50,
        })
    }

    async fn get_by_unko_no(
        &self,
        _tenant_id: Uuid,
        _unko_no: &str,
    ) -> Result<Vec<DtakoOperation>, sqlx::Error> {
        check_fail!(self);
        Ok(self.get_result.lock().unwrap().clone())
    }

    async fn delete_by_unko_no(
        &self,
        _tenant_id: Uuid,
        _unko_no: &str,
    ) -> Result<u64, sqlx::Error> {
        check_fail!(self);
        Ok(*self.delete_rows_affected.lock().unwrap())
    }

    async fn list_operation_changes(
        &self,
        _tenant_id: Uuid,
        _driver_cd: &str,
    ) -> Result<Vec<OperationChangeRow>, sqlx::Error> {
        check_fail!(self);
        Ok(self.operation_changes.lock().unwrap().clone())
    }

    async fn operation_changes_recording_since(
        &self,
        _tenant_id: Uuid,
    ) -> Result<Option<DateTime<Utc>>, sqlx::Error> {
        if self.fail_recording_since.load(Ordering::SeqCst) {
            return Err(sqlx::Error::RowNotFound);
        }
        Ok(*self.recording_since.lock().unwrap())
    }
}

// =============================================================================
// MockDtakoRestraintReportRepository
// =============================================================================

pub struct MockDtakoRestraintReportRepository {
    pub fail_next: AtomicBool,
    pub return_driver_name: AtomicBool,
    pub drivers_with_cd: std::sync::Mutex<Vec<(Uuid, Option<String>, String)>>,
    pub segments: std::sync::Mutex<Vec<SegmentRow>>,
    pub daily_work_hours: std::sync::Mutex<Vec<DailyWorkHoursRow>>,
    pub op_times: std::sync::Mutex<Vec<OpTimesRow>>,
    pub prev_day_drive: std::sync::Mutex<Option<i32>>,
    pub fiscal_cumulative: std::sync::Mutex<i32>,
}

impl Default for MockDtakoRestraintReportRepository {
    fn default() -> Self {
        Self {
            fail_next: AtomicBool::new(false),
            return_driver_name: AtomicBool::new(false),
            drivers_with_cd: std::sync::Mutex::new(vec![]),
            segments: std::sync::Mutex::new(vec![]),
            daily_work_hours: std::sync::Mutex::new(vec![]),
            op_times: std::sync::Mutex::new(vec![]),
            prev_day_drive: std::sync::Mutex::new(None),
            fiscal_cumulative: std::sync::Mutex::new(0),
        }
    }
}

#[async_trait::async_trait]
impl DtakoRestraintReportRepository for MockDtakoRestraintReportRepository {
    async fn get_driver_name(
        &self,
        _tenant_id: Uuid,
        _driver_id: Uuid,
    ) -> Result<Option<String>, sqlx::Error> {
        check_fail!(self);
        if self.return_driver_name.load(Ordering::SeqCst) {
            Ok(Some("テスト太郎".to_string()))
        } else {
            Ok(None)
        }
    }

    async fn get_segments(
        &self,
        _tenant_id: Uuid,
        _driver_id: Uuid,
        _month_start: NaiveDate,
        _month_end: NaiveDate,
    ) -> Result<Vec<SegmentRow>, sqlx::Error> {
        check_fail!(self);
        Ok(self.segments.lock().unwrap().clone())
    }

    async fn get_daily_work_hours(
        &self,
        _tenant_id: Uuid,
        _driver_id: Uuid,
        _month_start: NaiveDate,
        _month_end: NaiveDate,
    ) -> Result<Vec<DailyWorkHoursRow>, sqlx::Error> {
        check_fail!(self);
        Ok(self.daily_work_hours.lock().unwrap().clone())
    }

    async fn get_prev_day_drive(
        &self,
        _tenant_id: Uuid,
        _driver_id: Uuid,
        _prev_day: NaiveDate,
    ) -> Result<Option<i32>, sqlx::Error> {
        check_fail!(self);
        Ok(*self.prev_day_drive.lock().unwrap())
    }

    async fn get_fiscal_cumulative(
        &self,
        _tenant_id: Uuid,
        _driver_id: Uuid,
        _fiscal_year_start: NaiveDate,
        _prev_month_end: NaiveDate,
    ) -> Result<i32, sqlx::Error> {
        check_fail!(self);
        Ok(*self.fiscal_cumulative.lock().unwrap())
    }

    async fn get_operation_times(
        &self,
        _tenant_id: Uuid,
        _driver_id: Uuid,
        _month_start: NaiveDate,
        _month_end: NaiveDate,
    ) -> Result<Vec<OpTimesRow>, sqlx::Error> {
        check_fail!(self);
        Ok(self.op_times.lock().unwrap().clone())
    }

    async fn list_drivers_with_cd(
        &self,
        _tenant_id: Uuid,
    ) -> Result<Vec<(Uuid, Option<String>, String)>, sqlx::Error> {
        check_fail!(self);
        Ok(self.drivers_with_cd.lock().unwrap().clone())
    }
}

// =============================================================================
// MockDtakoRestraintReportPdfRepository
// =============================================================================

pub struct MockDtakoRestraintReportPdfRepository {
    pub fail_next: AtomicBool,
    pub drivers: std::sync::Mutex<Vec<PdfDriver>>,
}

impl Default for MockDtakoRestraintReportPdfRepository {
    fn default() -> Self {
        Self {
            fail_next: AtomicBool::new(false),
            drivers: std::sync::Mutex::new(vec![]),
        }
    }
}

#[async_trait::async_trait]
impl DtakoRestraintReportPdfRepository for MockDtakoRestraintReportPdfRepository {
    async fn list_drivers(&self, _tenant_id: Uuid) -> Result<Vec<PdfDriver>, sqlx::Error> {
        check_fail!(self);
        Ok(self.drivers.lock().unwrap().clone())
    }

    async fn get_driver(
        &self,
        _tenant_id: Uuid,
        driver_id: Uuid,
    ) -> Result<Vec<PdfDriver>, sqlx::Error> {
        check_fail!(self);
        let all = self.drivers.lock().unwrap();
        Ok(all.iter().filter(|d| d.id == driver_id).cloned().collect())
    }
}

// =============================================================================
// MockDtakoScraperRepository
// =============================================================================

pub struct MockDtakoScraperRepository {
    pub fail_next: AtomicBool,
    pub history_data: std::sync::Mutex<Vec<ScrapeHistoryItem>>,
    pub insert_count: std::sync::atomic::AtomicUsize,
    pub inserted_comp_ids: std::sync::Mutex<Vec<String>>,
}

impl Default for MockDtakoScraperRepository {
    fn default() -> Self {
        Self {
            fail_next: AtomicBool::new(false),
            history_data: std::sync::Mutex::new(vec![]),
            insert_count: std::sync::atomic::AtomicUsize::new(0),
            inserted_comp_ids: std::sync::Mutex::new(vec![]),
        }
    }
}

#[async_trait::async_trait]
impl DtakoScraperRepository for MockDtakoScraperRepository {
    async fn insert_scrape_history(
        &self,
        _tenant_id: Uuid,
        _target_date: NaiveDate,
        _comp_id: &str,
        _status: &str,
        _message: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        check_fail!(self);
        self.insert_count
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.inserted_comp_ids
            .lock()
            .unwrap()
            .push(_comp_id.to_string());
        Ok(())
    }

    async fn list_scrape_history(
        &self,
        _tenant_id: Uuid,
        _limit: i64,
        _offset: i64,
    ) -> Result<Vec<ScrapeHistoryItem>, sqlx::Error> {
        check_fail!(self);
        Ok(self.history_data.lock().unwrap().clone())
    }
}

// =============================================================================
// MockDtakoTicketsRepository
// =============================================================================

pub struct MockDtakoTicketsRepository {
    pub fail_next: AtomicBool,
}

impl Default for MockDtakoTicketsRepository {
    fn default() -> Self {
        Self {
            fail_next: AtomicBool::new(false),
        }
    }
}

#[async_trait::async_trait]
impl DtakoTicketsRepository for MockDtakoTicketsRepository {
    async fn create(
        &self,
        _tenant_id: Uuid,
        _input: &DtakoTicketCreate,
    ) -> Result<DtakoTicket, sqlx::Error> {
        check_fail!(self);
        Err(sqlx::Error::RowNotFound)
    }

    async fn patch_scraped(
        &self,
        _tenant_id: Uuid,
        _id: Uuid,
        _input: &DtakoTicketScrapedPatch,
    ) -> Result<Option<DtakoTicket>, sqlx::Error> {
        check_fail!(self);
        Ok(None)
    }

    async fn list(
        &self,
        _tenant_id: Uuid,
        filter: &DtakoTicketFilter,
    ) -> Result<DtakoTicketsResponse, sqlx::Error> {
        check_fail!(self);
        Ok(DtakoTicketsResponse {
            tickets: vec![],
            total: 0,
            page: filter.page.unwrap_or(1),
            per_page: filter.per_page.unwrap_or(50),
        })
    }

    async fn get(&self, _tenant_id: Uuid, _id: Uuid) -> Result<Option<DtakoTicket>, sqlx::Error> {
        check_fail!(self);
        Ok(None)
    }

    async fn close_by_token(
        &self,
        _close_token: &str,
        _closed_by: Option<&str>,
    ) -> Result<Option<Uuid>, sqlx::Error> {
        check_fail!(self);
        Ok(None)
    }
}

// =============================================================================
// MockDtakoVehiclesRepository
// =============================================================================

pub struct MockDtakoVehiclesRepository {
    pub fail_next: AtomicBool,
}

impl Default for MockDtakoVehiclesRepository {
    fn default() -> Self {
        Self {
            fail_next: AtomicBool::new(false),
        }
    }
}

#[async_trait::async_trait]
impl DtakoVehiclesRepository for MockDtakoVehiclesRepository {
    async fn list(&self, _tenant_id: Uuid) -> Result<Vec<DtakoVehicle>, sqlx::Error> {
        check_fail!(self);
        Ok(vec![])
    }
}

// =============================================================================
// MockDtakoWorkTimesRepository
// =============================================================================

pub struct MockDtakoWorkTimesRepository {
    pub fail_next: AtomicBool,
}

impl Default for MockDtakoWorkTimesRepository {
    fn default() -> Self {
        Self {
            fail_next: AtomicBool::new(false),
        }
    }
}

#[async_trait::async_trait]
impl DtakoWorkTimesRepository for MockDtakoWorkTimesRepository {
    async fn count(
        &self,
        _tenant_id: Uuid,
        _driver_id: Option<Uuid>,
        _date_from: Option<NaiveDate>,
        _date_to: Option<NaiveDate>,
    ) -> Result<i64, sqlx::Error> {
        check_fail!(self);
        Ok(0)
    }

    async fn list(
        &self,
        _tenant_id: Uuid,
        _driver_id: Option<Uuid>,
        _date_from: Option<NaiveDate>,
        _date_to: Option<NaiveDate>,
        _limit: i64,
        _offset: i64,
    ) -> Result<Vec<WorkTimeItem>, sqlx::Error> {
        check_fail!(self);
        Ok(vec![])
    }
}

// =============================================================================
// MockDtakoYTimeExportRepository
// =============================================================================

pub struct MockDtakoYTimeExportRepository {
    pub fail_next: AtomicBool,
    pub driver: std::sync::Mutex<Option<(Uuid, String)>>,
    pub operations: std::sync::Mutex<Vec<YTimeExportOperation>>,
}

impl Default for MockDtakoYTimeExportRepository {
    fn default() -> Self {
        Self {
            fail_next: AtomicBool::new(false),
            driver: std::sync::Mutex::new(None),
            operations: std::sync::Mutex::new(Vec::new()),
        }
    }
}

impl MockDtakoYTimeExportRepository {
    pub fn with_driver(self, id: Uuid, name: impl Into<String>) -> Self {
        *self.driver.lock().unwrap() = Some((id, name.into()));
        self
    }

    pub fn with_operations(self, ops: Vec<YTimeExportOperation>) -> Self {
        *self.operations.lock().unwrap() = ops;
        self
    }
}

#[async_trait::async_trait]
impl DtakoYTimeExportRepository for MockDtakoYTimeExportRepository {
    async fn lookup_driver(
        &self,
        _tenant_id: Uuid,
        _driver_cd: &str,
    ) -> Result<Option<(Uuid, String)>, sqlx::Error> {
        check_fail!(self);
        Ok(self.driver.lock().unwrap().clone())
    }

    async fn list_operations(
        &self,
        _tenant_id: Uuid,
        _driver_id: Uuid,
        _from: NaiveDate,
        _to: NaiveDate,
    ) -> Result<Vec<YTimeExportOperation>, sqlx::Error> {
        check_fail!(self);
        Ok(self.operations.lock().unwrap().clone())
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
