use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::models::{TimePunch, TimePunchWithDevice, TimecardCard};

/// CSV エクスポート用の行データ
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct TimePunchCsvRow {
    pub id: Uuid,
    pub punched_at: DateTime<Utc>,
    pub employee_name: String,
    pub employee_code: Option<String>,
    pub device_name: Option<String>,
}

#[async_trait]
pub trait TimecardRepository: Send + Sync {
    async fn create_card(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
        card_id: &str,
        label: Option<&str>,
    ) -> Result<TimecardCard, sqlx::Error>;

    async fn list_cards(
        &self,
        tenant_id: Uuid,
        employee_id: Option<Uuid>,
    ) -> Result<Vec<TimecardCard>, sqlx::Error>;

    async fn get_card(
        &self,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<Option<TimecardCard>, sqlx::Error>;

    async fn get_card_by_card_id(
        &self,
        tenant_id: Uuid,
        card_id: &str,
    ) -> Result<Option<TimecardCard>, sqlx::Error>;

    /// Delete a card. Returns true if a row was affected.
    async fn delete_card(&self, tenant_id: Uuid, id: Uuid) -> Result<bool, sqlx::Error>;

    /// Find a card by card_id (for punch lookup).
    async fn find_card_by_card_id(
        &self,
        tenant_id: Uuid,
        card_id: &str,
    ) -> Result<Option<TimecardCard>, sqlx::Error>;

    /// Find employee by nfc_id (fallback for punch).
    async fn find_employee_id_by_nfc(
        &self,
        tenant_id: Uuid,
        nfc_id: &str,
    ) -> Result<Option<Uuid>, sqlx::Error>;

    /// Create a time punch record.
    async fn create_punch(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
        device_id: Option<Uuid>,
    ) -> Result<TimePunch, sqlx::Error>;

    /// Get employee name by id.
    async fn get_employee_name(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
    ) -> Result<String, sqlx::Error>;

    /// List today's punches for an employee.
    async fn list_today_punches(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
    ) -> Result<Vec<TimePunch>, sqlx::Error>;

    /// Count punches with filters.
    async fn count_punches(
        &self,
        tenant_id: Uuid,
        employee_id: Option<Uuid>,
        date_from: Option<DateTime<Utc>>,
        date_to: Option<DateTime<Utc>>,
    ) -> Result<i64, sqlx::Error>;

    /// List punches with filters, pagination, and JOINed device/employee names.
    async fn list_punches(
        &self,
        tenant_id: Uuid,
        employee_id: Option<Uuid>,
        date_from: Option<DateTime<Utc>>,
        date_to: Option<DateTime<Utc>>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<TimePunchWithDevice>, sqlx::Error>;

    /// List punches for CSV export (with employee code, no pagination).
    async fn list_punches_for_csv(
        &self,
        tenant_id: Uuid,
        employee_id: Option<Uuid>,
        date_from: Option<DateTime<Utc>>,
        date_to: Option<DateTime<Utc>>,
    ) -> Result<Vec<TimePunchCsvRow>, sqlx::Error>;
}
