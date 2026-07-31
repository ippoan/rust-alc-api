use std::sync::atomic::{AtomicBool, Ordering};

use chrono::{NaiveDate, Utc};
use uuid::Uuid;

use rust_alc_api::db::models::*;
use rust_alc_api::db::repository::dtako_event_classifications::DtakoEventClassificationsRepository;
use rust_alc_api::db::repository::dtako_operations::DtakoOperationsRepository;
use rust_alc_api::db::repository::dtako_restraint_report::{
    DailyWorkHoursRow, DtakoRestraintReportRepository, OpTimesRow, SegmentRow,
};
use rust_alc_api::db::repository::dtako_restraint_report_pdf::{
    DtakoRestraintReportPdfRepository, PdfDriver,
};
use rust_alc_api::db::repository::dtako_scraper::DtakoScraperRepository;
use rust_alc_api::db::repository::dtako_tickets::DtakoTicketsRepository;
use rust_alc_api::db::repository::dtako_upload::{
    DtakoDriverOpRow, DtakoOpRow, DtakoUploadRepository, InsertDailyWorkHoursParams,
    InsertOperationParams, InsertSegmentParams, UploadHistoryRecord, UploadTenantAndKey,
};
use rust_alc_api::db::repository::dtako_vehicles::DtakoVehiclesRepository;
use rust_alc_api::db::repository::dtako_work_times::{DtakoWorkTimesRepository, WorkTimeItem};
use rust_alc_api::db::repository::dtako_y_time_export::{
    DtakoDriverOperation, DtakoDriverRef, DtakoYTimeExportRepository, UnsplitOperation,
    YTimeExportOperation,
};
use rust_alc_api::db::repository::employees::EmployeeRepository;
use rust_alc_api::db::repository::equipment_failures::EquipmentFailuresRepository;
use rust_alc_api::db::repository::guidance_records::{
    GuidanceRecordWithName, GuidanceRecordsRepository,
};
use rust_alc_api::db::repository::health_baselines::HealthBaselinesRepository;
use rust_alc_api::db::repository::hub_measurements::HubMeasurementsRepository;
use rust_alc_api::db::repository::measurements::{ListResult, MeasurementsRepository};
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
}

impl Default for MockDtakoOperationsRepository {
    fn default() -> Self {
        Self {
            fail_next: AtomicBool::new(false),
            calendar_dates_result: std::sync::Mutex::new(vec![]),
            get_result: std::sync::Mutex::new(vec![]),
            delete_rows_affected: std::sync::Mutex::new(0),
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
// MockDtakoUploadRepository
// =============================================================================

pub struct MockDtakoUploadRepository {
    pub fail_next: AtomicBool,
    /// create_upload_history を tenant FK 違反 (dtako_upload_history_tenant_id_fkey) 相当の
    /// エラーで失敗させる (staging 揮発 DB で tenant が消えたケースの再現)。
    pub fail_tenant_fk: AtomicBool,
    pub fail_update_has_kudgivt: AtomicBool,
    pub upload_history: std::sync::Mutex<Option<UploadHistoryRecord>>,
    pub tenant_and_key: std::sync::Mutex<Option<UploadTenantAndKey>>,
    pub driver_cd: std::sync::Mutex<Option<String>>,
    pub employee_id: std::sync::Mutex<Option<Uuid>>,
    pub operations: std::sync::Mutex<Vec<DtakoOpRow>>,
    pub driver_operations: std::sync::Mutex<Vec<DtakoDriverOpRow>>,
    pub zip_keys: std::sync::Mutex<Vec<String>>,
    pub uploads_needing_split: std::sync::Mutex<Vec<(Uuid, String)>>,
    pub event_classifications: std::sync::Mutex<Vec<(String, String)>>,
    /// 直近の `update_has_kudgivt` 呼び出しに渡された `unko_nos` を記録する
    /// (PUT 失敗した unko_no が除外されているかをテストから検査するため。Refs
    /// ohishi-exp/rust-ichibanboshi#205 の 31)。
    pub last_update_has_kudgivt_unko_nos: std::sync::Mutex<Option<Vec<String>>>,
    /// `update_has_kudgivt` が「当たらなかった」ことにする unko_no (突合キーのずれで
    /// 一部が更新されない mismatch ケースのテスト用)。空なら渡された unko_nos 全部が
    /// 当たったことにする (正常系)。
    pub update_has_kudgivt_missing_unko_nos: std::sync::Mutex<Vec<String>>,
}

impl Default for MockDtakoUploadRepository {
    fn default() -> Self {
        Self {
            fail_next: AtomicBool::new(false),
            fail_tenant_fk: AtomicBool::new(false),
            fail_update_has_kudgivt: AtomicBool::new(false),
            upload_history: std::sync::Mutex::new(None),
            tenant_and_key: std::sync::Mutex::new(None),
            driver_cd: std::sync::Mutex::new(None),
            employee_id: std::sync::Mutex::new(None),
            operations: std::sync::Mutex::new(vec![]),
            driver_operations: std::sync::Mutex::new(vec![]),
            zip_keys: std::sync::Mutex::new(vec![]),
            uploads_needing_split: std::sync::Mutex::new(vec![]),
            event_classifications: std::sync::Mutex::new(vec![]),
            last_update_has_kudgivt_unko_nos: std::sync::Mutex::new(None),
            update_has_kudgivt_missing_unko_nos: std::sync::Mutex::new(vec![]),
        }
    }
}

#[async_trait::async_trait]
impl DtakoUploadRepository for MockDtakoUploadRepository {
    async fn create_upload_history(
        &self,
        _tenant_id: Uuid,
        _upload_id: Uuid,
        _filename: &str,
    ) -> Result<(), sqlx::Error> {
        if self.fail_tenant_fk.swap(false, Ordering::SeqCst) {
            return Err(sqlx::Error::Protocol(
                "insert or update on table \"dtako_upload_history\" violates foreign key constraint \"dtako_upload_history_tenant_id_fkey\"".into(),
            ));
        }
        check_fail!(self);
        Ok(())
    }

    async fn update_upload_completed(
        &self,
        _tenant_id: Uuid,
        _upload_id: Uuid,
        _operations_count: i32,
    ) -> Result<(), sqlx::Error> {
        check_fail!(self);
        Ok(())
    }

    async fn update_upload_r2_key(
        &self,
        _tenant_id: Uuid,
        _upload_id: Uuid,
        _r2_zip_key: &str,
    ) -> Result<(), sqlx::Error> {
        check_fail!(self);
        Ok(())
    }

    async fn mark_upload_failed(
        &self,
        _upload_id: Uuid,
        _error_msg: &str,
    ) -> Result<(), sqlx::Error> {
        check_fail!(self);
        Ok(())
    }

    async fn get_upload_history(
        &self,
        _upload_id: Uuid,
    ) -> Result<Option<UploadHistoryRecord>, sqlx::Error> {
        check_fail!(self);
        Ok(self.upload_history.lock().unwrap().clone())
    }

    async fn get_upload_tenant_and_key(
        &self,
        _upload_id: Uuid,
    ) -> Result<Option<UploadTenantAndKey>, sqlx::Error> {
        check_fail!(self);
        Ok(self.tenant_and_key.lock().unwrap().clone())
    }

    async fn list_uploads(&self, _tenant_id: Uuid) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        check_fail!(self);
        Ok(vec![])
    }

    async fn list_pending_uploads(
        &self,
        _tenant_id: Uuid,
    ) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        check_fail!(self);
        Ok(vec![])
    }

    async fn list_uploads_needing_split(
        &self,
        _tenant_id: Uuid,
    ) -> Result<Vec<(Uuid, String)>, sqlx::Error> {
        check_fail!(self);
        Ok(self.uploads_needing_split.lock().unwrap().clone())
    }

    async fn fetch_zip_keys(
        &self,
        _tenant_id: Uuid,
        _month_start: NaiveDate,
    ) -> Result<Vec<String>, sqlx::Error> {
        check_fail!(self);
        Ok(self.zip_keys.lock().unwrap().clone())
    }

    async fn upsert_office(
        &self,
        _tenant_id: Uuid,
        _office_cd: &str,
        _office_name: &str,
    ) -> Result<Option<Uuid>, sqlx::Error> {
        check_fail!(self);
        Ok(None)
    }

    async fn upsert_vehicle(
        &self,
        _tenant_id: Uuid,
        _vehicle_cd: &str,
        _vehicle_name: &str,
    ) -> Result<Option<Uuid>, sqlx::Error> {
        check_fail!(self);
        Ok(None)
    }

    async fn upsert_driver(
        &self,
        _tenant_id: Uuid,
        _driver_cd: &str,
        _driver_name: &str,
    ) -> Result<Option<Uuid>, sqlx::Error> {
        check_fail!(self);
        Ok(None)
    }

    async fn delete_operation(
        &self,
        _tenant_id: Uuid,
        _unko_no: &str,
        _crew_role: i32,
    ) -> Result<(), sqlx::Error> {
        check_fail!(self);
        Ok(())
    }

    async fn insert_operation(
        &self,
        _tenant_id: Uuid,
        _params: &InsertOperationParams,
    ) -> Result<(), sqlx::Error> {
        check_fail!(self);
        Ok(())
    }

    async fn update_has_kudgivt(
        &self,
        _tenant_id: Uuid,
        unko_nos: &[String],
    ) -> Result<std::collections::HashSet<String>, sqlx::Error> {
        check_fail!(self);
        *self.last_update_has_kudgivt_unko_nos.lock().unwrap() = Some(unko_nos.to_vec());
        if self.fail_update_has_kudgivt.load(Ordering::SeqCst) {
            return Err(sqlx::Error::RowNotFound);
        }
        let missing = self.update_has_kudgivt_missing_unko_nos.lock().unwrap();
        Ok(unko_nos
            .iter()
            .filter(|u| !missing.contains(u))
            .cloned()
            .collect())
    }

    async fn load_event_classifications(
        &self,
        _tenant_id: Uuid,
    ) -> Result<Vec<(String, String)>, sqlx::Error> {
        check_fail!(self);
        Ok(self.event_classifications.lock().unwrap().clone())
    }

    async fn insert_event_classification(
        &self,
        _tenant_id: Uuid,
        _event_cd: &str,
        _event_name: &str,
        _classification: &str,
    ) -> Result<(), sqlx::Error> {
        check_fail!(self);
        Ok(())
    }

    async fn get_employee_id_by_driver_cd(
        &self,
        _tenant_id: Uuid,
        _driver_cd: &str,
    ) -> Result<Option<Uuid>, sqlx::Error> {
        check_fail!(self);
        Ok(*self.employee_id.lock().unwrap())
    }

    async fn get_driver_cd(
        &self,
        _tenant_id: Uuid,
        _driver_id: Uuid,
    ) -> Result<Option<String>, sqlx::Error> {
        check_fail!(self);
        Ok(self.driver_cd.lock().unwrap().clone())
    }

    async fn delete_segments_by_unko(
        &self,
        _tenant_id: Uuid,
        _driver_id: Uuid,
        _unko_no: &str,
    ) -> Result<(), sqlx::Error> {
        check_fail!(self);
        Ok(())
    }

    async fn delete_daily_hours_by_unko_nos(
        &self,
        _tenant_id: Uuid,
        _driver_id: Uuid,
        _unko_nos: &[String],
    ) -> Result<(), sqlx::Error> {
        check_fail!(self);
        Ok(())
    }

    async fn delete_daily_hours_exact(
        &self,
        _tenant_id: Uuid,
        _driver_id: Uuid,
        _work_date: NaiveDate,
        _start_time: chrono::NaiveTime,
    ) -> Result<(), sqlx::Error> {
        check_fail!(self);
        Ok(())
    }

    async fn insert_daily_work_hours(
        &self,
        _tenant_id: Uuid,
        _params: &InsertDailyWorkHoursParams,
    ) -> Result<(), sqlx::Error> {
        check_fail!(self);
        Ok(())
    }

    async fn delete_segments_by_date(
        &self,
        _tenant_id: Uuid,
        _driver_id: Uuid,
        _work_date: NaiveDate,
    ) -> Result<(), sqlx::Error> {
        check_fail!(self);
        Ok(())
    }

    async fn insert_segment(
        &self,
        _tenant_id: Uuid,
        _params: &InsertSegmentParams,
    ) -> Result<(), sqlx::Error> {
        check_fail!(self);
        Ok(())
    }

    async fn fetch_operations_for_recalc(
        &self,
        _tenant_id: Uuid,
        _month_start: NaiveDate,
        _fetch_end: NaiveDate,
    ) -> Result<Vec<DtakoOpRow>, sqlx::Error> {
        check_fail!(self);
        Ok(self.operations.lock().unwrap().clone())
    }

    async fn load_driver_operations(
        &self,
        _tenant_id: Uuid,
        _driver_id: Uuid,
        _month_start: NaiveDate,
        _fetch_end: NaiveDate,
    ) -> Result<Vec<DtakoDriverOpRow>, sqlx::Error> {
        check_fail!(self);
        Ok(self.driver_operations.lock().unwrap().clone())
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
    /// 全乗務員版 (Refs ohishi-exp/rust-ichibanboshi#205 実装計画 01) 用。
    pub drivers: std::sync::Mutex<Vec<DtakoDriverRef>>,
    pub driver_operations: std::sync::Mutex<Vec<DtakoDriverOperation>>,
    /// `list_operations` / `list_operations_for_drivers` だけを失敗させる。
    /// `fail_next` は先に `lookup_driver` / `list_drivers_with_operations` で
    /// 消費されるため、後段のエラー経路を単独で試すのに要る。
    pub fail_operations: AtomicBool,
    /// etags の `unsplit` (has_kudgivt = FALSE) 用 (Refs #205 の 36)。
    pub unsplit_operations: std::sync::Mutex<Vec<UnsplitOperation>>,
    /// `list_unsplit_operations` だけを失敗させる。
    pub fail_unsplit: AtomicBool,
}

impl Default for MockDtakoYTimeExportRepository {
    fn default() -> Self {
        Self {
            fail_next: AtomicBool::new(false),
            driver: std::sync::Mutex::new(None),
            operations: std::sync::Mutex::new(Vec::new()),
            drivers: std::sync::Mutex::new(Vec::new()),
            driver_operations: std::sync::Mutex::new(Vec::new()),
            fail_operations: AtomicBool::new(false),
            unsplit_operations: std::sync::Mutex::new(Vec::new()),
            fail_unsplit: AtomicBool::new(false),
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

    pub fn with_drivers(self, drivers: Vec<DtakoDriverRef>) -> Self {
        *self.drivers.lock().unwrap() = drivers;
        self
    }

    pub fn with_driver_operations(self, ops: Vec<DtakoDriverOperation>) -> Self {
        *self.driver_operations.lock().unwrap() = ops;
        self
    }

    pub fn with_unsplit_operations(self, ops: Vec<UnsplitOperation>) -> Self {
        *self.unsplit_operations.lock().unwrap() = ops;
        self
    }

    pub fn failing_operations(self) -> Self {
        self.fail_operations.store(true, Ordering::SeqCst);
        self
    }

    pub fn failing_unsplit(self) -> Self {
        self.fail_unsplit.store(true, Ordering::SeqCst);
        self
    }

    fn check_fail_operations(&self) -> Result<(), sqlx::Error> {
        if self.fail_operations.swap(false, Ordering::SeqCst) {
            return Err(sqlx::Error::RowNotFound);
        }
        Ok(())
    }

    fn check_fail_unsplit(&self) -> Result<(), sqlx::Error> {
        if self.fail_unsplit.swap(false, Ordering::SeqCst) {
            return Err(sqlx::Error::RowNotFound);
        }
        Ok(())
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
        self.check_fail_operations()?;
        Ok(self.operations.lock().unwrap().clone())
    }

    async fn list_drivers_with_operations(
        &self,
        _tenant_id: Uuid,
        _from: NaiveDate,
        _to: NaiveDate,
        after_driver_cd: Option<&str>,
        limit: i64,
    ) -> Result<Vec<DtakoDriverRef>, sqlx::Error> {
        check_fail!(self);
        // Pg 実装と同じ keyset 挙動 (driver_cd 昇順 / after は排他的下限 / limit 打ち切り)
        let mut rows: Vec<DtakoDriverRef> = self.drivers.lock().unwrap().clone();
        rows.sort_by(|a, b| a.driver_cd.cmp(&b.driver_cd));
        if let Some(after) = after_driver_cd {
            rows.retain(|d| d.driver_cd.as_str() > after);
        }
        rows.truncate(limit as usize);
        Ok(rows)
    }

    async fn list_operations_for_drivers(
        &self,
        _tenant_id: Uuid,
        driver_ids: &[Uuid],
        _from: NaiveDate,
        _to: NaiveDate,
    ) -> Result<Vec<DtakoDriverOperation>, sqlx::Error> {
        check_fail!(self);
        self.check_fail_operations()?;
        let rows: Vec<DtakoDriverOperation> = self
            .driver_operations
            .lock()
            .unwrap()
            .iter()
            .filter(|o| driver_ids.contains(&o.driver_id))
            .cloned()
            .collect();
        Ok(rows)
    }

    async fn list_unsplit_operations(
        &self,
        _tenant_id: Uuid,
        _from: NaiveDate,
        _to: NaiveDate,
    ) -> Result<Vec<UnsplitOperation>, sqlx::Error> {
        check_fail!(self);
        self.check_fail_unsplit()?;
        Ok(self.unsplit_operations.lock().unwrap().clone())
    }
}

// =============================================================================
// MockEmployeeRepository
// =============================================================================

pub struct MockEmployeeRepository {
    pub fail_next: AtomicBool,
    pub return_some: AtomicBool,
    pub return_deleted: AtomicBool,
    pub return_conflict: AtomicBool,
}

impl Default for MockEmployeeRepository {
    fn default() -> Self {
        Self {
            fail_next: AtomicBool::new(false),
            return_some: AtomicBool::new(false),
            return_deleted: AtomicBool::new(false),
            return_conflict: AtomicBool::new(false),
        }
    }
}

impl MockEmployeeRepository {
    fn sample_employee(&self) -> Employee {
        Employee {
            id: Uuid::new_v4(),
            tenant_id: Uuid::new_v4(),
            code: Some("EMP-001".to_string()),
            nfc_id: Some("nfc-abc".to_string()),
            name: "Test Employee".to_string(),
            face_photo_url: None,
            face_embedding: None,
            face_embedding_at: None,
            face_model_version: None,
            face_approval_status: "pending".to_string(),
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
}

#[async_trait::async_trait]
impl EmployeeRepository for MockEmployeeRepository {
    async fn create(
        &self,
        _tenant_id: Uuid,
        _input: &CreateEmployee,
    ) -> Result<Employee, sqlx::Error> {
        check_fail!(self);
        Ok(self.sample_employee())
    }

    async fn list(&self, _tenant_id: Uuid) -> Result<Vec<Employee>, sqlx::Error> {
        check_fail!(self);
        Ok(vec![])
    }

    async fn get(&self, _tenant_id: Uuid, _id: Uuid) -> Result<Option<Employee>, sqlx::Error> {
        check_fail!(self);
        if self.return_some.load(Ordering::SeqCst) {
            return Ok(Some(self.sample_employee()));
        }
        Ok(None)
    }

    async fn get_by_nfc(
        &self,
        _tenant_id: Uuid,
        _nfc_id: &str,
    ) -> Result<Option<Employee>, sqlx::Error> {
        check_fail!(self);
        if self.return_some.load(Ordering::SeqCst) {
            return Ok(Some(self.sample_employee()));
        }
        Ok(None)
    }

    async fn get_by_code(
        &self,
        _tenant_id: Uuid,
        _code: &str,
    ) -> Result<Option<Employee>, sqlx::Error> {
        check_fail!(self);
        if self.return_some.load(Ordering::SeqCst) {
            return Ok(Some(self.sample_employee()));
        }
        Ok(None)
    }

    async fn update(
        &self,
        _tenant_id: Uuid,
        _id: Uuid,
        _input: &UpdateEmployee,
    ) -> Result<Option<Employee>, sqlx::Error> {
        if self.return_conflict.load(Ordering::SeqCst) {
            return Err(sqlx::Error::Database(Box::new(MockDbError(
                "idx_employees_code".to_string(),
            ))));
        }
        check_fail!(self);
        if self.return_some.load(Ordering::SeqCst) {
            return Ok(Some(self.sample_employee()));
        }
        Ok(None)
    }

    async fn delete(&self, _tenant_id: Uuid, _id: Uuid) -> Result<bool, sqlx::Error> {
        check_fail!(self);
        if self.return_deleted.load(Ordering::SeqCst) {
            return Ok(true);
        }
        Ok(false)
    }

    async fn update_face(
        &self,
        _tenant_id: Uuid,
        _id: Uuid,
        _input: &UpdateFace,
    ) -> Result<Option<Employee>, sqlx::Error> {
        check_fail!(self);
        if self.return_some.load(Ordering::SeqCst) {
            return Ok(Some(self.sample_employee()));
        }
        Ok(None)
    }

    async fn list_face_data(&self, _tenant_id: Uuid) -> Result<Vec<FaceDataEntry>, sqlx::Error> {
        check_fail!(self);
        Ok(vec![])
    }

    async fn update_license(
        &self,
        _tenant_id: Uuid,
        _id: Uuid,
        _issue_date: Option<chrono::NaiveDate>,
        _expiry_date: Option<chrono::NaiveDate>,
    ) -> Result<Option<Employee>, sqlx::Error> {
        check_fail!(self);
        if self.return_some.load(Ordering::SeqCst) {
            return Ok(Some(self.sample_employee()));
        }
        Ok(None)
    }

    async fn update_nfc_id(
        &self,
        _tenant_id: Uuid,
        _id: Uuid,
        _nfc_id: &str,
    ) -> Result<Option<Employee>, sqlx::Error> {
        check_fail!(self);
        if self.return_some.load(Ordering::SeqCst) {
            return Ok(Some(self.sample_employee()));
        }
        Ok(None)
    }

    async fn approve_face(
        &self,
        _tenant_id: Uuid,
        _id: Uuid,
    ) -> Result<Option<Employee>, sqlx::Error> {
        check_fail!(self);
        if self.return_some.load(Ordering::SeqCst) {
            return Ok(Some(self.sample_employee()));
        }
        Ok(None)
    }

    async fn reject_face(
        &self,
        _tenant_id: Uuid,
        _id: Uuid,
    ) -> Result<Option<Employee>, sqlx::Error> {
        check_fail!(self);
        if self.return_some.load(Ordering::SeqCst) {
            return Ok(Some(self.sample_employee()));
        }
        Ok(None)
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
// MockGuidanceRecordsRepository
// =============================================================================

pub struct MockGuidanceRecordsRepository {
    pub fail_next: AtomicBool,
    pub fail_on_list_tree: AtomicBool,
    pub return_record: std::sync::Mutex<Option<GuidanceRecord>>,
    pub return_attachment: std::sync::Mutex<Option<GuidanceRecordAttachment>>,
    pub parent_depth: std::sync::Mutex<Option<i32>>,
    pub delete_rows: std::sync::Mutex<u64>,
    pub list_tree_result: std::sync::Mutex<Vec<GuidanceRecordWithName>>,
    pub list_attachments_result: std::sync::Mutex<Vec<GuidanceRecordAttachment>>,
    pub count_result: std::sync::Mutex<i64>,
}

impl Default for MockGuidanceRecordsRepository {
    fn default() -> Self {
        Self {
            fail_next: AtomicBool::new(false),
            fail_on_list_tree: AtomicBool::new(false),
            return_record: std::sync::Mutex::new(None),
            return_attachment: std::sync::Mutex::new(None),
            parent_depth: std::sync::Mutex::new(None),
            delete_rows: std::sync::Mutex::new(0),
            list_tree_result: std::sync::Mutex::new(vec![]),
            list_attachments_result: std::sync::Mutex::new(vec![]),
            count_result: std::sync::Mutex::new(0),
        }
    }
}

#[async_trait::async_trait]
impl GuidanceRecordsRepository for MockGuidanceRecordsRepository {
    async fn count_top_level(
        &self,
        _tenant_id: Uuid,
        _employee_id: Option<Uuid>,
        _guidance_type: Option<&str>,
        _date_from: Option<&str>,
        _date_to: Option<&str>,
    ) -> Result<i64, sqlx::Error> {
        check_fail!(self);
        Ok(*self.count_result.lock().unwrap())
    }

    async fn list_tree(
        &self,
        _tenant_id: Uuid,
        _employee_id: Option<Uuid>,
        _guidance_type: Option<&str>,
        _date_from: Option<&str>,
        _date_to: Option<&str>,
        _limit: i64,
        _offset: i64,
    ) -> Result<Vec<GuidanceRecordWithName>, sqlx::Error> {
        if self.fail_on_list_tree.swap(false, Ordering::SeqCst) {
            return Err(sqlx::Error::RowNotFound);
        }
        check_fail!(self);
        Ok(self.list_tree_result.lock().unwrap().clone())
    }

    async fn list_attachments_by_record_ids(
        &self,
        _tenant_id: Uuid,
        _record_ids: &[Uuid],
    ) -> Result<Vec<GuidanceRecordAttachment>, sqlx::Error> {
        check_fail!(self);
        Ok(self.list_attachments_result.lock().unwrap().clone())
    }

    async fn get(
        &self,
        _tenant_id: Uuid,
        _id: Uuid,
    ) -> Result<Option<GuidanceRecord>, sqlx::Error> {
        check_fail!(self);
        Ok(self.return_record.lock().unwrap().clone())
    }

    async fn get_parent_depth(
        &self,
        _tenant_id: Uuid,
        _parent_id: Uuid,
    ) -> Result<Option<i32>, sqlx::Error> {
        check_fail!(self);
        Ok(*self.parent_depth.lock().unwrap())
    }

    async fn create(
        &self,
        tenant_id: Uuid,
        input: &CreateGuidanceRecord,
        depth: i32,
    ) -> Result<GuidanceRecord, sqlx::Error> {
        check_fail!(self);
        Ok(GuidanceRecord {
            id: Uuid::new_v4(),
            tenant_id,
            employee_id: input.employee_id,
            guidance_type: input.guidance_type.clone().unwrap_or_default(),
            title: input.title.clone(),
            content: input.content.clone().unwrap_or_default(),
            guided_by: input.guided_by.clone(),
            guided_at: input.guided_at.unwrap_or_else(Utc::now),
            parent_id: input.parent_id,
            depth,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        })
    }

    async fn update(
        &self,
        _tenant_id: Uuid,
        _id: Uuid,
        _input: &UpdateGuidanceRecord,
    ) -> Result<Option<GuidanceRecord>, sqlx::Error> {
        check_fail!(self);
        Ok(self.return_record.lock().unwrap().clone())
    }

    async fn delete_recursive(&self, _tenant_id: Uuid, _id: Uuid) -> Result<u64, sqlx::Error> {
        check_fail!(self);
        Ok(*self.delete_rows.lock().unwrap())
    }

    async fn list_attachments(
        &self,
        _tenant_id: Uuid,
        _record_id: Uuid,
    ) -> Result<Vec<GuidanceRecordAttachment>, sqlx::Error> {
        check_fail!(self);
        Ok(self.list_attachments_result.lock().unwrap().clone())
    }

    async fn create_attachment(
        &self,
        _tenant_id: Uuid,
        record_id: Uuid,
        file_name: &str,
        file_type: &str,
        file_size: i32,
        storage_url: &str,
    ) -> Result<GuidanceRecordAttachment, sqlx::Error> {
        check_fail!(self);
        Ok(GuidanceRecordAttachment {
            id: Uuid::new_v4(),
            record_id,
            file_name: file_name.to_string(),
            file_type: file_type.to_string(),
            file_size: Some(file_size),
            storage_url: storage_url.to_string(),
            created_at: Utc::now(),
        })
    }

    async fn get_attachment(
        &self,
        _tenant_id: Uuid,
        _record_id: Uuid,
        _att_id: Uuid,
    ) -> Result<Option<GuidanceRecordAttachment>, sqlx::Error> {
        check_fail!(self);
        Ok(self.return_attachment.lock().unwrap().clone())
    }

    async fn delete_attachment(
        &self,
        _tenant_id: Uuid,
        _record_id: Uuid,
        _att_id: Uuid,
    ) -> Result<u64, sqlx::Error> {
        check_fail!(self);
        Ok(*self.delete_rows.lock().unwrap())
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
// MockMeasurementsRepository
// =============================================================================

pub struct MockMeasurementsRepository {
    pub fail_next: AtomicBool,
    pub return_some: AtomicBool,
    pub face_photo_url: std::sync::Mutex<Option<String>>,
    pub video_url: std::sync::Mutex<Option<String>>,
}

impl Default for MockMeasurementsRepository {
    fn default() -> Self {
        Self {
            fail_next: AtomicBool::new(false),
            return_some: AtomicBool::new(false),
            face_photo_url: std::sync::Mutex::new(None),
            video_url: std::sync::Mutex::new(None),
        }
    }
}

impl MockMeasurementsRepository {
    fn sample_measurement(&self, tenant_id: Uuid) -> Measurement {
        let now = Utc::now();
        Measurement {
            id: Uuid::new_v4(),
            tenant_id,
            employee_id: Uuid::new_v4(),
            alcohol_level: Some(0.0),
            result: Some("pass".to_string()),
            device_use_count: 1,
            face_photo_url: self.face_photo_url.lock().unwrap().clone(),
            video_url: self.video_url.lock().unwrap().clone(),
            measured_at: now,
            created_at: now,
            updated_at: now,
            status: "completed".to_string(),
            temperature: None,
            systolic: None,
            diastolic: None,
            pulse: None,
            medical_measured_at: None,
            face_verified: None,
            medical_manual_input: None,
        }
    }
}

#[async_trait::async_trait]
impl MeasurementsRepository for MockMeasurementsRepository {
    async fn start(
        &self,
        tenant_id: Uuid,
        _input: &StartMeasurement,
    ) -> Result<Measurement, sqlx::Error> {
        check_fail!(self);
        let mut m = self.sample_measurement(tenant_id);
        m.status = "started".to_string();
        m.alcohol_level = None;
        m.result = None;
        Ok(m)
    }

    async fn create(
        &self,
        tenant_id: Uuid,
        _input: &CreateMeasurement,
    ) -> Result<Measurement, sqlx::Error> {
        check_fail!(self);
        Ok(self.sample_measurement(tenant_id))
    }

    async fn update(
        &self,
        tenant_id: Uuid,
        _id: Uuid,
        _input: &UpdateMeasurement,
    ) -> Result<Option<Measurement>, sqlx::Error> {
        check_fail!(self);
        if self.return_some.load(Ordering::SeqCst) {
            Ok(Some(self.sample_measurement(tenant_id)))
        } else {
            Ok(None)
        }
    }

    async fn get(&self, tenant_id: Uuid, _id: Uuid) -> Result<Option<Measurement>, sqlx::Error> {
        check_fail!(self);
        if self.return_some.load(Ordering::SeqCst) {
            Ok(Some(self.sample_measurement(tenant_id)))
        } else {
            Ok(None)
        }
    }

    async fn list(
        &self,
        _tenant_id: Uuid,
        _filter: &MeasurementFilter,
        _page: i64,
        _per_page: i64,
    ) -> Result<ListResult, sqlx::Error> {
        check_fail!(self);
        Ok(ListResult {
            measurements: vec![],
            total: 0,
        })
    }
}

// =============================================================================
// MockHubMeasurementsRepository (Refs #564)
// =============================================================================

/// insert された item を tenant ごとに記録し、(device_id, seq) 重複は本物の
/// `ON CONFLICT (tenant_id, device_id, seq) DO NOTHING` と同じく duplicates に数える。
pub struct MockHubMeasurementsRepository {
    pub fail_next: AtomicBool,
    pub inserted: std::sync::Mutex<Vec<(Uuid, HubMeasurementCreate)>>,
    seen: std::sync::Mutex<std::collections::HashSet<(Uuid, String, i64)>>,
}

impl Default for MockHubMeasurementsRepository {
    fn default() -> Self {
        Self {
            fail_next: AtomicBool::new(false),
            inserted: std::sync::Mutex::new(vec![]),
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
        let mut inserted: i64 = 0;
        for item in items {
            if seen.insert((tenant_id, item.device_id.clone(), item.seq)) {
                inserted_log.push((tenant_id, item.clone()));
                inserted += 1;
            }
        }
        Ok(HubMeasurementsIngestResponse {
            inserted,
            duplicates: items.len() as i64 - inserted,
        })
    }
}
