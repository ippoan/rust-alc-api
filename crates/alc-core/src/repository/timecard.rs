use async_trait::async_trait;
use uuid::Uuid;

use crate::models::TimecardCard;

/// カードの照合だけを持つ repository (Refs ippoan/rust-alc-api#680)。
///
/// タイムカードの口 (`/api/timecard/...`) は ippoan/alc-people-worker へ移った (W4d-a)。
/// ここに残すのは NFC タイムカード端末の打刻中継 (`hub_measurements` の `kind="timecard"`、
/// alc-devices) が社員を解決するための照合だけ。
#[async_trait]
pub trait TimecardRepository: Send + Sync {
    /// 台帳 (`timecard_cards`) から card_id (正規化済み) のカード。
    async fn find_card_by_card_id(
        &self,
        tenant_id: Uuid,
        card_id: &str,
    ) -> Result<Option<TimecardCard>, sqlx::Error>;

    /// 免許証番号 (`employees.nfc_id`) から社員 (カードが外れたときのフォールバック)。
    async fn find_employee_id_by_nfc(
        &self,
        tenant_id: Uuid,
        nfc_id: &str,
    ) -> Result<Option<Uuid>, sqlx::Error>;
}

/// カード ID から社員を特定する。`timecard_cards` を引き、外れたら
/// `employees.nfc_id` (免許証の交付日 8 桁 + 有効期限 8 桁) へフォールバックする。
///
/// 照合は**完全一致**だが、その手前で `normalize_card_id` を 1 回だけ通す。
/// **呼び出し側は読み取った生値をそのまま渡すこと** — 呼び出し側ごとに別の加工を挟むと、
/// 経路ごとに違う値で引くことになる。alc-people-worker のカードの照会 (`POST /timecard/cards/lookup`)
/// も同じ規則 (`timecard::normalize_card_id`) で引く。
pub async fn resolve_employee_by_card(
    repo: &dyn TimecardRepository,
    tenant_id: Uuid,
    card_id: &str,
) -> Result<Option<Uuid>, sqlx::Error> {
    let card_id = normalize_card_id(card_id);
    // ★ **カードが在っても持ち主が居ないこと**がある (Refs ippoan/rust-alc-api#644)。
    // 社員マスタがまだ同期されていないカードは `employee_id` が NULL のまま台帳に
    // 載っている。そこで打ち切らず `employees.nfc_id` のフォールバックへ落とす。
    if let Some(card) = repo.find_card_by_card_id(tenant_id, &card_id).await? {
        if let Some(employee_id) = card.employee_id {
            return Ok(Some(employee_id));
        }
    }
    repo.find_employee_id_by_nfc(tenant_id, &card_id).await
}

/// カード ID の正規化 (trim + 小文字 + `':'` 除去)。
///
/// 同じ物理カードでも読み取り側で表記が揺れる (IDm を大文字で出す端末、`AA:BB:..` と区切る実装)
/// ため、生値のまま完全一致で引くと同じカードが別カードとして扱われる。台帳への登録は
/// alc-people-worker が同じ規則で行い、DB 側の CHECK 制約 (`timecard_cards_card_id_normalized`、
/// migration 134) が書き忘れを loud fail させる。免許証番号 (数字 16 桁) に対しては no-op。
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
    fn license_nfc_id_is_untouched() {
        assert_eq!(normalize_card_id("2023040120280331"), "2023040120280331");
    }

    #[test]
    fn empty_input_stays_empty() {
        assert_eq!(normalize_card_id("   "), "");
    }
}
