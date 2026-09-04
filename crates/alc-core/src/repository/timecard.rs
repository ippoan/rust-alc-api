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
/// 照合は**完全一致**だが、その手前で `normalize_card_id` を 1 回だけ通す。
/// **呼び出し側は読み取った生値をそのまま渡すこと** — 接頭辞を付けたり、
/// 呼び出し側ごとに別の加工を挟んだりすると、経路ごとに違う値で引くことになる。
pub async fn resolve_employee_by_card(
    repo: &dyn TimecardRepository,
    tenant_id: Uuid,
    card_id: &str,
) -> Result<Option<Uuid>, sqlx::Error> {
    let card_id = normalize_card_id(card_id);
    if let Some(card) = repo.find_card_by_card_id(tenant_id, &card_id).await? {
        return Ok(Some(card.employee_id));
    }
    repo.find_employee_id_by_nfc(tenant_id, &card_id).await
}

/// カード ID の正規化。**`timecard_cards` の登録も照合もこの結果で行う。**
///
/// 同じ物理カードでも読み取り側で表記が揺れる (IDm を大文字で出す端末、小文字で
/// 出すローカル NFC ブリッジ、`AA:BB:..` と区切る実装) ため、生値のまま完全一致で
/// 引くと同じカードが別カードとして扱われる。**小文字**なのは `alc-carins` の
/// `normalize_nfc_uuid` (車検証 NFC タグ) と規約を揃えるため — 1 つの repo に
/// NFC ID の正規化規約を 2 つ並べない。
///
/// **読み側だけ正規化してはいけない。** `ABC` と `abc` の 2 行が同時に存在し得ると
/// 打刻が別人に着く。登録側も同じ関数を通し、DB 側の CHECK 制約
/// (`timecard_cards_card_id_normalized`、migration 134) で書き忘れを loud fail させる。
///
/// `employees.nfc_id` フォールバック (免許証の交付日 8 桁 + 有効期限 8 桁 = 16 桁の
/// 数字) に対しては no-op なので、本番で動いている免許証経路の挙動は変わらない。
pub fn normalize_card_id(card_id: &str) -> String {
    card_id.trim().to_lowercase().replace(':', "")
}

#[cfg(test)]
mod tests {
    use super::normalize_card_id;

    #[test]
    fn felica_idm_uppercase_becomes_lowercase() {
        // 端末は %02X (大文字) で IDm を送る
        assert_eq!(normalize_card_id("0123456789ABCDEF"), "0123456789abcdef");
    }

    #[test]
    fn separators_and_surrounding_space_are_dropped() {
        assert_eq!(normalize_card_id("  AA:BB:CC:DD  "), "aabbccdd");
    }

    #[test]
    fn already_normalized_value_is_unchanged() {
        assert_eq!(normalize_card_id("0123456789abcdef"), "0123456789abcdef");
    }

    #[test]
    fn license_nfc_id_is_untouched() {
        // employees.nfc_id は交付日 8 桁 + 有効期限 8 桁の数字。
        // 本番で動いている免許証経路の挙動を変えないことを固定する
        assert_eq!(normalize_card_id("2023040120280331"), "2023040120280331");
    }

    #[test]
    fn empty_input_stays_empty() {
        assert_eq!(normalize_card_id("   "), "");
    }
}
