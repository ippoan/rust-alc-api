use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::models::{
    TimePunch, TimePunchWithDevice, TimecardCard, TimecardCardConflictPolicy,
    TimecardCardDeleteResult, TimecardCardUpsertItem, TimecardCardUpsertSkipped,
    TimecardCardUpsertSummary,
};

/// 一括取り込み (`PUT /timecard/cards/bulk-by-code`) が入れた行に刻む出所
/// (Refs ippoan/rust-alc-api#644)。
///
/// **`POST /timecard/cards/delete-by-card` が消してよい範囲はこの値ちょうど。**
/// `timecard_cards` には alc 側で直接登録されたカード (`source IS NULL`) が在り得るので、
/// 無条件に消すとそれを巻き込む。
///
/// **`label` を根拠にしてはいけない** — 自由文で誰でも書けるうえ、
/// `bulk_upsert_cards_by_code` の `ON CONFLICT ... DO UPDATE` は `label` を更新しないので
/// 「同期が触った行なのに label が違う」が既に在り得る。だから**サーバ側の定数**を別列に刻む。
///
/// 値は初回移行で使った label (`timecard-cf-worker` の `IMPORT_LABEL`) と同じ文字列で、
/// migration 145 の backfill がその label の行に刻んだものと一致する。
pub const CARD_SOURCE_LEDGER_SYNC: &str = "timecard-ic-ledger-import";

/// CSV エクスポート用の行データ
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct TimePunchCsvRow {
    pub id: Uuid,
    pub punched_at: DateTime<Utc>,
    /// 未登録カードのタップでは None (CSV では空欄)。行ごと落とすと
    /// 「タップしたのに CSV に出ない」になり、登録漏れに気付けなくなる
    pub employee_name: Option<String>,
    pub employee_code: Option<String>,
    pub device_name: Option<String>,
    /// `timecard` / `license`。CSV の「区分」列になる — 混ぜたまま出すと
    /// 点呼が打刻として集計される
    pub kind: String,
    /// かざしたカードの種別 (`license` / `felica_idm` / `nfca_uid`)。CSV の
    /// 「カード」列になる。ブラウザ打刻と旧行は None (空欄)。
    /// **`kind` とは別の軸** — あちらは「打刻か点呼か」
    pub card_kind: Option<String>,
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

    /// `PUT /timecard/cards/bulk-by-code` の書き込み側 (Refs ippoan/rust-alc-api#644)。
    ///
    /// **渡すのは `prepare_bulk_cards` を通した後の items だけ** — 正規化・形の検査・
    /// バッチ内重複の除去は済んでいる前提で、ここは「社員の解決」と「書き込み」だけを見る。
    ///
    /// 1 件の不備で全体を落とさない: 解決は SELECT、書き込みは `ON CONFLICT` なので
    /// 制約違反が起きる余地が無く、savepoint も要らない。
    /// `dry_run` は判定を最後まで通してから commit しないことで実現する
    /// (判定コードを 2 本持たないため)。
    async fn bulk_upsert_cards_by_code(
        &self,
        tenant_id: Uuid,
        items: &[PreparedCardUpsert],
        on_conflict: TimecardCardConflictPolicy,
        dry_run: bool,
    ) -> Result<TimecardCardUpsertSummary, sqlx::Error>;

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

    /// `POST /timecard/cards/delete-by-card` の書き込み側 (Refs ippoan/rust-alc-api#644)。
    ///
    /// **消すのは `source = CARD_SOURCE_LEDGER_SYNC` の行だけ** — 出所を実装側の定数で
    /// 縛るため、`source` は引数に取らない (body 由来の値が届く経路を作らない)。
    ///
    /// `card_id` は `normalize_card_id` 通過後の値を渡すこと。
    /// `dry_run` は `bulk_upsert_cards_by_code` と同じ「判定は最後まで同じコードを通し、
    /// commit しない」形。
    async fn delete_card_by_card_id_from_sync(
        &self,
        tenant_id: Uuid,
        card_id: &str,
        dry_run: bool,
    ) -> Result<TimecardCardDeleteResult, sqlx::Error>;

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

    /// ブラウザ版 (キオスク / Android) の打刻を 1 件記録する。
    ///
    /// **書き込み先は `hub_measurements` ただ 1 つ**
    /// (Refs ippoan/alc-app-s3#134)。打刻の一次表を 1 つにするため — 2 つあると
    /// 「時刻がサーバ時刻になる」「端末 ID が入らない」「重複排除が要る」が
    /// そこから生まれる。端末 (NFC タイムカード端末) は同じ表に WS 経由で入れる。
    ///
    /// `employee_id` は**呼び出し側が解決済みのものを渡し、payload に凍結する**。
    /// 端末側 (`freeze_employee_id`) と同じ理由で、読み出し時に引き直すと
    /// カードを付け替えたときに過去の打刻が別人に動く。
    ///
    /// 戻り値は旧・打刻表の時代からの互換で `TimePunch` の形にして返すが、`id` は
    /// `hub_measurements.id` である。
    async fn create_punch(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
        device_id: Option<Uuid>,
        card_id: &str,
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

/// 一括取り込みで受け取った `card_id` が、カードの ID として取り得る形かを見る。
///
/// **`normalize_card_id` を通した後の値を渡すこと** (小文字・区切り無しが前提)。
///
/// 長さは **8 / 14 / 16 桁のいずれか**。FeliCa の IDm は 8 バイト = 16 桁だが、
/// Mifare の UID は 4 / 7 バイト = 8 / 14 桁もある。**16 桁固定にすると一部の
/// カードを黙って落とす。** 形が外れたものは取り込まず `invalid_card_id` で
/// skipped に載せる — 中央 DB 側に空欄や「未登録」等の文字列が混ざっていても、
/// それを card_id として登録してしまわないため。
pub fn is_valid_bulk_card_id(normalized: &str) -> bool {
    matches!(normalized.len(), 8 | 14 | 16)
        && normalized
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// 正規化・形の検査・バッチ内重複の除去まで済ませた 1 件。
///
/// `index` は**リクエストの items 内の位置**。応答の `skipped` は `card_id` を
/// 載せないので、送り手が行を特定する手掛かりがこれと `code` の 2 つしかない。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedCardUpsert {
    pub index: usize,
    pub code: String,
    /// 正規化済み (`normalize_card_id` 通過後)。
    pub card_id: String,
    pub label: Option<String>,
}

/// 一括取り込みの前処理。DB を 1 度も引かずに決まるぶんをここで決める。
///
/// * `card_id` を `normalize_card_id` で正規化する (**唯一の正規化点**。送り手は
///   大文字の生値をそのまま送ってくる)
/// * 形が外れたものを `invalid_card_id` で落とす
/// * **同一バッチ内で同じ `card_id` が 2 度出てきたら後勝ちにせず**
///   `duplicate_in_batch` で落とす — 中央 DB 側の台帳が壊れている (1 枚のカードが
///   2 人に紐づいている) 合図なので、どちらかを黙って採ると人が気付けない
pub fn prepare_bulk_cards(
    items: &[TimecardCardUpsertItem],
) -> (Vec<PreparedCardUpsert>, Vec<TimecardCardUpsertSkipped>) {
    let mut prepared: Vec<PreparedCardUpsert> = Vec::new();
    let mut skipped: Vec<TimecardCardUpsertSkipped> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

    for (index, item) in items.iter().enumerate() {
        let card_id = normalize_card_id(&item.card_id);
        if !is_valid_bulk_card_id(&card_id) {
            skipped.push(TimecardCardUpsertSkipped {
                index,
                code: item.code.clone(),
                reason: "invalid_card_id".to_string(),
            });
            continue;
        }
        if !seen.insert(card_id.clone()) {
            skipped.push(TimecardCardUpsertSkipped {
                index,
                code: item.code.clone(),
                reason: "duplicate_in_batch".to_string(),
            });
            continue;
        }
        prepared.push(PreparedCardUpsert {
            index,
            code: item.code.clone(),
            card_id,
            label: item.label.clone(),
        });
    }

    (prepared, skipped)
}

#[cfg(test)]
mod tests {
    use super::{is_valid_bulk_card_id, normalize_card_id, prepare_bulk_cards};
    use crate::models::TimecardCardUpsertItem;

    fn item(code: &str, card_id: &str) -> TimecardCardUpsertItem {
        TimecardCardUpsertItem {
            code: code.to_string(),
            card_id: card_id.to_string(),
            label: None,
        }
    }

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

    // --- 一括取り込みの形の検査 (Refs ippoan/rust-alc-api#644) ---

    #[test]
    fn accepts_4_7_8_byte_card_ids() {
        // Mifare UID 4 バイト / 7 バイト、FeliCa IDm 8 バイト。
        // **16 桁固定にすると前 2 つを黙って落とす**
        assert!(is_valid_bulk_card_id("01ab23cd"));
        assert!(is_valid_bulk_card_id("04a1b2c3d4e5f6"));
        assert!(is_valid_bulk_card_id("01401d0b1d37b660"));
    }

    #[test]
    fn rejects_other_lengths() {
        assert!(!is_valid_bulk_card_id(""));
        assert!(!is_valid_bulk_card_id("01ab23c"));
        assert!(!is_valid_bulk_card_id("01ab23cde"));
        assert!(!is_valid_bulk_card_id("01401d0b1d37b6601140"));
    }

    #[test]
    fn rejects_non_hex_and_uppercase() {
        // 大文字は正規化前の値。検査は**正規化後**に掛ける契約なので、
        // ここに大文字が来たら呼び出し順が壊れている
        assert!(!is_valid_bulk_card_id("01401D0B1D37B660"));
        assert!(!is_valid_bulk_card_id("未登録"));
        assert!(!is_valid_bulk_card_id("01401g0b1d37b660"));
    }

    #[test]
    fn prepare_normalizes_and_keeps_index() {
        let (prepared, skipped) = prepare_bulk_cards(&[
            item("E001", "  01401D0B:1D37:B660  "),
            item("E002", "04A1B2C3D4E5F6"),
        ]);
        assert!(skipped.is_empty());
        assert_eq!(prepared.len(), 2);
        assert_eq!(prepared[0].index, 0);
        assert_eq!(prepared[0].card_id, "01401d0b1d37b660");
        assert_eq!(prepared[1].card_id, "04a1b2c3d4e5f6");
    }

    #[test]
    fn prepare_skips_invalid_card_id_but_keeps_the_rest() {
        let (prepared, skipped) = prepare_bulk_cards(&[
            item("E001", "01401D0B1D37B660"),
            item("E002", "未登録"),
            item("E003", "04A1B2C3D4E5F6"),
        ]);
        assert_eq!(prepared.len(), 2, "他の行は落とさない");
        assert_eq!(skipped.len(), 1);
        assert_eq!(skipped[0].index, 1);
        assert_eq!(skipped[0].code, "E002");
        assert_eq!(skipped[0].reason, "invalid_card_id");
    }

    #[test]
    fn prepare_skips_duplicate_in_batch_without_last_write_wins() {
        // 表記が違っても正規化後が同じなら同じ 1 枚。**後勝ちにしない** —
        // 台帳が壊れている合図なので、黙ってどちらかを採ると人が気付けない
        let (prepared, skipped) = prepare_bulk_cards(&[
            item("E001", "01401D0B1D37B660"),
            item("E002", "01:40:1d:0b:1d:37:b6:60"),
        ]);
        assert_eq!(prepared.len(), 1);
        assert_eq!(prepared[0].code, "E001", "先に来た行が残る");
        assert_eq!(skipped.len(), 1);
        assert_eq!(skipped[0].index, 1);
        assert_eq!(skipped[0].code, "E002");
        assert_eq!(skipped[0].reason, "duplicate_in_batch");
    }

    #[test]
    fn prepare_keeps_label() {
        let mut with_label = item("E001", "01401D0B1D37B660");
        with_label.label = Some("入館証".to_string());
        let (prepared, _) = prepare_bulk_cards(&[with_label]);
        assert_eq!(prepared[0].label.as_deref(), Some("入館証"));
    }

    #[test]
    fn prepare_on_empty_input_is_empty() {
        let (prepared, skipped) = prepare_bulk_cards(&[]);
        assert!(prepared.is_empty());
        assert!(skipped.is_empty());
    }
}
