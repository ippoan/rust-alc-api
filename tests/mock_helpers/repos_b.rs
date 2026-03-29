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
use rust_alc_api::db::repository::dtako_upload::{
    DtakoDriverOpRow, DtakoOpRow, DtakoUploadRepository, InsertDailyWorkHoursParams,
    InsertOperationParams, InsertSegmentParams, UploadHistoryRecord, UploadTenantAndKey,
};
use rust_alc_api::db::repository::dtako_vehicles::DtakoVehiclesRepository;
use rust_alc_api::db::repository::dtako_work_times::{DtakoWorkTimesRepository, WorkTimeItem};
use rust_alc_api::db::repository::employees::EmployeeRepository;
use rust_alc_api::db::repository::equipment_failures::EquipmentFailuresRepository;
use rust_alc_api::db::repository::guidance_records::{
    GuidanceRecordWithName, GuidanceRecordsRepository,
};
use rust_alc_api::db::repository::health_baselines::HealthBaselinesRepository;
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
}

impl Default for MockDtakoRestraintReportRepository {
    fn default() -> Self {
        Self {
            fail_next: AtomicBool::new(false),
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
        Ok(None)
    }

    async fn get_segments(
        &self,
        _tenant_id: Uuid,
        _driver_id: Uuid,
        _month_start: NaiveDate,
        _month_end: NaiveDate,
    ) -> Result<Vec<SegmentRow>, sqlx::Error> {
        check_fail!(self);
        Ok(vec![])
    }

    async fn get_daily_work_hours(
        &self,
        _tenant_id: Uuid,
        _driver_id: Uuid,
        _month_start: NaiveDate,
        _month_end: NaiveDate,
    ) -> Result<Vec<DailyWorkHoursRow>, sqlx::Error> {
        check_fail!(self);
        Ok(vec![])
    }

    async fn get_prev_day_drive(
        &self,
        _tenant_id: Uuid,
        _driver_id: Uuid,
        _prev_day: NaiveDate,
    ) -> Result<Option<i32>, sqlx::Error> {
        check_fail!(self);
        Ok(None)
    }

    async fn get_fiscal_cumulative(
        &self,
        _tenant_id: Uuid,
        _driver_id: Uuid,
        _fiscal_year_start: NaiveDate,
        _prev_month_end: NaiveDate,
    ) -> Result<i32, sqlx::Error> {
        check_fail!(self);
        Ok(0)
    }

    async fn get_operation_times(
        &self,
        _tenant_id: Uuid,
        _driver_id: Uuid,
        _month_start: NaiveDate,
        _month_end: NaiveDate,
    ) -> Result<Vec<OpTimesRow>, sqlx::Error> {
        check_fail!(self);
        Ok(vec![])
    }

    async fn list_drivers_with_cd(
        &self,
        _tenant_id: Uuid,
    ) -> Result<Vec<(Uuid, Option<String>, String)>, sqlx::Error> {
        check_fail!(self);
        Ok(vec![])
    }
}

// =============================================================================
// MockDtakoRestraintReportPdfRepository
// =============================================================================

pub struct MockDtakoRestraintReportPdfRepository {
    pub fail_next: AtomicBool,
}

impl Default for MockDtakoRestraintReportPdfRepository {
    fn default() -> Self {
        Self {
            fail_next: AtomicBool::new(false),
        }
    }
}

#[async_trait::async_trait]
impl DtakoRestraintReportPdfRepository for MockDtakoRestraintReportPdfRepository {
    async fn list_drivers(&self, _tenant_id: Uuid) -> Result<Vec<PdfDriver>, sqlx::Error> {
        check_fail!(self);
        Ok(vec![])
    }

    async fn get_driver(
        &self,
        _tenant_id: Uuid,
        _driver_id: Uuid,
    ) -> Result<Vec<PdfDriver>, sqlx::Error> {
        check_fail!(self);
        Ok(vec![])
    }
}

// =============================================================================
// MockDtakoScraperRepository
// =============================================================================

pub struct MockDtakoScraperRepository {
    pub fail_next: AtomicBool,
}

impl Default for MockDtakoScraperRepository {
    fn default() -> Self {
        Self {
            fail_next: AtomicBool::new(false),
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
        Ok(())
    }

    async fn list_scrape_history(
        &self,
        _tenant_id: Uuid,
        _limit: i64,
        _offset: i64,
    ) -> Result<Vec<ScrapeHistoryItem>, sqlx::Error> {
        check_fail!(self);
        Ok(vec![])
    }
}

// =============================================================================
// MockDtakoUploadRepository
// =============================================================================

pub struct MockDtakoUploadRepository {
    pub fail_next: AtomicBool,
}

impl Default for MockDtakoUploadRepository {
    fn default() -> Self {
        Self {
            fail_next: AtomicBool::new(false),
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
        Ok(None)
    }

    async fn get_upload_tenant_and_key(
        &self,
        _upload_id: Uuid,
    ) -> Result<Option<UploadTenantAndKey>, sqlx::Error> {
        check_fail!(self);
        Ok(None)
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
        Ok(vec![])
    }

    async fn fetch_zip_keys(
        &self,
        _tenant_id: Uuid,
        _month_start: NaiveDate,
    ) -> Result<Vec<String>, sqlx::Error> {
        check_fail!(self);
        Ok(vec![])
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
        _unko_nos: &[String],
    ) -> Result<(), sqlx::Error> {
        check_fail!(self);
        Ok(())
    }

    async fn load_event_classifications(
        &self,
        _tenant_id: Uuid,
    ) -> Result<Vec<(String, String)>, sqlx::Error> {
        check_fail!(self);
        Ok(vec![])
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
        Ok(None)
    }

    async fn get_driver_cd(
        &self,
        _tenant_id: Uuid,
        _driver_id: Uuid,
    ) -> Result<Option<String>, sqlx::Error> {
        check_fail!(self);
        Ok(None)
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
        Ok(vec![])
    }

    async fn load_driver_operations(
        &self,
        _tenant_id: Uuid,
        _driver_id: Uuid,
        _month_start: NaiveDate,
        _fetch_end: NaiveDate,
    ) -> Result<Vec<DtakoDriverOpRow>, sqlx::Error> {
        check_fail!(self);
        Ok(vec![])
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
// MockEmployeeRepository
// =============================================================================

pub struct MockEmployeeRepository {
    pub fail_next: AtomicBool,
}

impl Default for MockEmployeeRepository {
    fn default() -> Self {
        Self {
            fail_next: AtomicBool::new(false),
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
        todo!("MockEmployeeRepository::create")
    }

    async fn list(&self, _tenant_id: Uuid) -> Result<Vec<Employee>, sqlx::Error> {
        check_fail!(self);
        Ok(vec![])
    }

    async fn get(&self, _tenant_id: Uuid, _id: Uuid) -> Result<Option<Employee>, sqlx::Error> {
        check_fail!(self);
        Ok(None)
    }

    async fn get_by_nfc(
        &self,
        _tenant_id: Uuid,
        _nfc_id: &str,
    ) -> Result<Option<Employee>, sqlx::Error> {
        check_fail!(self);
        Ok(None)
    }

    async fn get_by_code(
        &self,
        _tenant_id: Uuid,
        _code: &str,
    ) -> Result<Option<Employee>, sqlx::Error> {
        check_fail!(self);
        Ok(None)
    }

    async fn update(
        &self,
        _tenant_id: Uuid,
        _id: Uuid,
        _input: &UpdateEmployee,
    ) -> Result<Option<Employee>, sqlx::Error> {
        check_fail!(self);
        Ok(None)
    }

    async fn delete(&self, _tenant_id: Uuid, _id: Uuid) -> Result<bool, sqlx::Error> {
        check_fail!(self);
        Ok(false)
    }

    async fn update_face(
        &self,
        _tenant_id: Uuid,
        _id: Uuid,
        _input: &UpdateFace,
    ) -> Result<Option<Employee>, sqlx::Error> {
        check_fail!(self);
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
        Ok(None)
    }

    async fn update_nfc_id(
        &self,
        _tenant_id: Uuid,
        _id: Uuid,
        _nfc_id: &str,
    ) -> Result<Option<Employee>, sqlx::Error> {
        check_fail!(self);
        Ok(None)
    }

    async fn approve_face(
        &self,
        _tenant_id: Uuid,
        _id: Uuid,
    ) -> Result<Option<Employee>, sqlx::Error> {
        check_fail!(self);
        Ok(None)
    }

    async fn reject_face(
        &self,
        _tenant_id: Uuid,
        _id: Uuid,
    ) -> Result<Option<Employee>, sqlx::Error> {
        check_fail!(self);
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
}

impl Default for MockGuidanceRecordsRepository {
    fn default() -> Self {
        Self {
            fail_next: AtomicBool::new(false),
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
        Ok(0)
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
        check_fail!(self);
        Ok(vec![])
    }

    async fn list_attachments_by_record_ids(
        &self,
        _tenant_id: Uuid,
        _record_ids: &[Uuid],
    ) -> Result<Vec<GuidanceRecordAttachment>, sqlx::Error> {
        check_fail!(self);
        Ok(vec![])
    }

    async fn get(
        &self,
        _tenant_id: Uuid,
        _id: Uuid,
    ) -> Result<Option<GuidanceRecord>, sqlx::Error> {
        check_fail!(self);
        Ok(None)
    }

    async fn get_parent_depth(
        &self,
        _tenant_id: Uuid,
        _parent_id: Uuid,
    ) -> Result<Option<i32>, sqlx::Error> {
        check_fail!(self);
        Ok(None)
    }

    async fn create(
        &self,
        _tenant_id: Uuid,
        _input: &CreateGuidanceRecord,
        _depth: i32,
    ) -> Result<GuidanceRecord, sqlx::Error> {
        check_fail!(self);
        todo!("MockGuidanceRecordsRepository::create")
    }

    async fn update(
        &self,
        _tenant_id: Uuid,
        _id: Uuid,
        _input: &UpdateGuidanceRecord,
    ) -> Result<Option<GuidanceRecord>, sqlx::Error> {
        check_fail!(self);
        Ok(None)
    }

    async fn delete_recursive(&self, _tenant_id: Uuid, _id: Uuid) -> Result<u64, sqlx::Error> {
        check_fail!(self);
        Ok(0)
    }

    async fn list_attachments(
        &self,
        _tenant_id: Uuid,
        _record_id: Uuid,
    ) -> Result<Vec<GuidanceRecordAttachment>, sqlx::Error> {
        check_fail!(self);
        Ok(vec![])
    }

    async fn create_attachment(
        &self,
        _tenant_id: Uuid,
        _record_id: Uuid,
        _file_name: &str,
        _file_type: &str,
        _file_size: i32,
        _storage_url: &str,
    ) -> Result<GuidanceRecordAttachment, sqlx::Error> {
        check_fail!(self);
        todo!("MockGuidanceRecordsRepository::create_attachment")
    }

    async fn get_attachment(
        &self,
        _tenant_id: Uuid,
        _record_id: Uuid,
        _att_id: Uuid,
    ) -> Result<Option<GuidanceRecordAttachment>, sqlx::Error> {
        check_fail!(self);
        Ok(None)
    }

    async fn delete_attachment(
        &self,
        _tenant_id: Uuid,
        _record_id: Uuid,
        _att_id: Uuid,
    ) -> Result<u64, sqlx::Error> {
        check_fail!(self);
        Ok(0)
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
}

impl Default for MockMeasurementsRepository {
    fn default() -> Self {
        Self {
            fail_next: AtomicBool::new(false),
        }
    }
}

#[async_trait::async_trait]
impl MeasurementsRepository for MockMeasurementsRepository {
    async fn start(
        &self,
        _tenant_id: Uuid,
        _input: &StartMeasurement,
    ) -> Result<Measurement, sqlx::Error> {
        check_fail!(self);
        todo!("MockMeasurementsRepository::start")
    }

    async fn create(
        &self,
        _tenant_id: Uuid,
        _input: &CreateMeasurement,
    ) -> Result<Measurement, sqlx::Error> {
        check_fail!(self);
        todo!("MockMeasurementsRepository::create")
    }

    async fn update(
        &self,
        _tenant_id: Uuid,
        _id: Uuid,
        _input: &UpdateMeasurement,
    ) -> Result<Option<Measurement>, sqlx::Error> {
        check_fail!(self);
        Ok(None)
    }

    async fn get(&self, _tenant_id: Uuid, _id: Uuid) -> Result<Option<Measurement>, sqlx::Error> {
        check_fail!(self);
        Ok(None)
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
