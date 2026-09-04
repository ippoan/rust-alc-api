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

/// カード ID から社員を特定する。`timecard_cards` を引き、外れたら
/// `employees.nfc_id` (免許証の交付日 8 桁 + 有効期限 8 桁) へフォールバックする。
///
/// **打刻の入口はブラウザ版 (`POST /api/timecard/punch`) と NFC タイムカード端末
/// (`hub_measurements` の `kind="timecard"` 中継、Refs ippoan/alc-app-s3#134) の
/// 2 つあるが、照合はこの 1 か所に閉じる。** 2 実装目を作ると、どちらか片方だけに
/// フォールバックを足す/外すといったズレが必ず出る。
///
/// 照合は**完全一致**なので、呼び出し側は `card_id` を加工せずに渡すこと
/// (端末は読み取った生値を送る。接頭辞や正規化を挟むと必ず外れる)。
pub async fn resolve_employee_by_card(
    repo: &dyn TimecardRepository,
    tenant_id: Uuid,
    card_id: &str,
) -> Result<Option<Uuid>, sqlx::Error> {
    if let Some(card) = repo.find_card_by_card_id(tenant_id, card_id).await? {
        return Ok(Some(card.employee_id));
    }
    repo.find_employee_id_by_nfc(tenant_id, card_id).await
}
