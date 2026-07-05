use async_trait::async_trait;
use uuid::Uuid;

use crate::models::{
    EmployeeHealthBaseline, TenkoDashboard, TenkoRecord, TenkoSchedule, TenkoSession,
    TenkoSessionFilter,
};

/// Paginated list result
pub struct SessionListResult {
    pub sessions: Vec<TenkoSession>,
    pub total: i64,
}

#[allow(clippy::too_many_arguments)]
#[async_trait]
pub trait TenkoSessionRepository: Send + Sync {
    // --- Session CRUD ---

    async fn get(&self, tenant_id: Uuid, id: Uuid) -> Result<Option<TenkoSession>, sqlx::Error>;

    async fn list(
        &self,
        tenant_id: Uuid,
        filter: &TenkoSessionFilter,
        page: i64,
        per_page: i64,
    ) -> Result<SessionListResult, sqlx::Error>;

    // --- Schedule queries ---

    async fn get_schedule_unconsumed(
        &self,
        tenant_id: Uuid,
        schedule_id: Uuid,
    ) -> Result<Option<TenkoSchedule>, sqlx::Error>;

    async fn consume_schedule(&self, tenant_id: Uuid, schedule_id: Uuid)
        -> Result<(), sqlx::Error>;

    async fn set_consumed_by_session(
        &self,
        tenant_id: Uuid,
        schedule_id: Uuid,
        session_id: Uuid,
    ) -> Result<(), sqlx::Error>;

    async fn get_schedule_instruction(
        &self,
        tenant_id: Uuid,
        schedule_id: Option<Uuid>,
    ) -> Result<Option<String>, sqlx::Error>;

    // --- Session creation ---

    #[allow(clippy::too_many_arguments)]
    async fn create_session(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
        schedule_id: Option<Uuid>,
        tenko_type: &str,
        initial_status: &str,
        identity_face_photo_url: &Option<String>,
        location: &Option<String>,
        responsible_manager_name: &Option<String>,
    ) -> Result<TenkoSession, sqlx::Error>;

    // --- Session updates ---

    async fn update_alcohol(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        next_status: &str,
        measurement_id: Option<Uuid>,
        alcohol_result: &str,
        alcohol_value: f64,
        alcohol_face_photo_url: &Option<String>,
        cancel_reason: &Option<String>,
        completed_at: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Result<TenkoSession, sqlx::Error>;

    async fn update_medical(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        temperature: Option<f64>,
        systolic: Option<i32>,
        diastolic: Option<i32>,
        pulse: Option<i32>,
        medical_measured_at: Option<chrono::DateTime<chrono::Utc>>,
        medical_manual_input: Option<bool>,
    ) -> Result<TenkoSession, sqlx::Error>;

    async fn confirm_instruction(
        &self,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<TenkoSession, sqlx::Error>;

    #[allow(clippy::too_many_arguments)]
    async fn update_report(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        next_status: &str,
        vehicle_road_status: &str,
        driver_alternation: &str,
        vehicle_road_audio_url: &Option<String>,
        driver_alternation_audio_url: &Option<String>,
        completed_at: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Result<TenkoSession, sqlx::Error>;

    async fn cancel(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        reason: &Option<String>,
    ) -> Result<TenkoSession, sqlx::Error>;

    async fn update_self_declaration(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        declaration_json: &serde_json::Value,
    ) -> Result<TenkoSession, sqlx::Error>;

    async fn update_safety_judgment(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        next_status: &str,
        judgment_json: &serde_json::Value,
        interrupted_at: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Result<TenkoSession, sqlx::Error>;

    async fn update_daily_inspection(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        next_status: &str,
        inspection_json: &serde_json::Value,
        cancel_reason: &Option<String>,
        completed_at: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Result<TenkoSession, sqlx::Error>;

    async fn update_carrying_items(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        carrying_json: &serde_json::Value,
    ) -> Result<TenkoSession, sqlx::Error>;

    async fn interrupt(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        reason: &Option<String>,
    ) -> Result<TenkoSession, sqlx::Error>;

    async fn resume(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        resume_to: &str,
        reason: &str,
        resumed_by_user_id: Option<Uuid>,
    ) -> Result<TenkoSession, sqlx::Error>;

    // --- Carrying items helpers ---

    async fn get_carrying_item_name(
        &self,
        tenant_id: Uuid,
        item_id: Uuid,
    ) -> Result<Option<String>, sqlx::Error>;

    async fn upsert_carrying_item_check(
        &self,
        tenant_id: Uuid,
        session_id: Uuid,
        item_id: Uuid,
        item_name: &str,
        checked: bool,
        checked_at: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Result<(), sqlx::Error>;

    async fn count_carrying_items(&self, tenant_id: Uuid) -> Result<i64, sqlx::Error>;

    // --- Employee / Baseline lookups ---

    async fn get_employee_name(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
    ) -> Result<Option<String>, sqlx::Error>;

    async fn get_health_baseline(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
    ) -> Result<Option<EmployeeHealthBaseline>, sqlx::Error>;

    // --- Tenko Record ---

    #[allow(clippy::too_many_arguments)]
    async fn create_tenko_record(
        &self,
        tenant_id: Uuid,
        session: &TenkoSession,
        employee_name: &str,
        instruction: &Option<String>,
        record_data: &serde_json::Value,
        record_hash: &str,
    ) -> Result<TenkoRecord, sqlx::Error>;

    // --- Dashboard ---

    async fn dashboard(
        &self,
        tenant_id: Uuid,
        overdue_minutes: i64,
    ) -> Result<TenkoDashboard, sqlx::Error>;
}
