//! 受信メール添付 PDF から抽出された金額情報をマスクする pure 関数群。
//!
//! Phase A (ingest_key 廃止 + R2 配線) は完了済み。Phase 3 (Gemini API による
//! `extracted_*` カラム埋め) と統合する際に、`mask_amounts_in_extraction()` を
//! 1 行呼ぶだけで「金額のみ受信者に見せない」運用を実現できるよう、本モジュールは
//! 外部 I/O・DB アクセスを一切持たない pure 関数のみで構成している。
//!
//! ## 設計方針
//!
//! - **「数字 + 円」を必須** にして、郵便番号 (`〒100-0001`)・電話 (`0799-45-1688`)・
//!   時刻 (`10:00`)・型番 (`9PL`) などの誤検知を排除。
//! - 半角・全角どちらの数字にも対応 (`160,000円` / `１２７,０００円`)。
//! - `￥` / `¥` / `JPY` 前置パターンも検出。
//! - **マスクトークンは `***円`** を採用。`*` は `[0-9０-９]` にマッチしないため、
//!   二重マスク (`mask(mask(s)) == mask(s)`) が常に成立する (idempotent)。
//!
//! ## TODO (将来拡張)
//!
//! - ラベルベース検出 (`金額: 160000` のように円なしの数字も検出): `mask_amounts_with_labels()`
//! - 漢字単位 (`1.2万円` / `100万`): 別正規表現
//! - 通貨記号 (`$100` USD): スコープ外
//! - JSON Number 葉のマスク (`{"amount": 127000}`): String 葉のみ対応中。
//!   Gemini の出力が Number で返ってくる場合は 別 PR で `mask_amounts_in_json_strict()` を追加する。

use alc_core::repository::notify_documents::ExtractionResult;
use once_cell::sync::Lazy;
use regex::Regex;
use serde_json::Value;

/// マスク後トークン。`*` は `[0-9０-９]` にマッチしないので idempotent を保証する。
pub const MASK_TOKEN: &str = "***円";

/// 金額検出の正規表現。
///
/// 3 グループの union:
/// - `(?P<a>...)` : 「数字 + (カンマ + 数字)* + 円」 (e.g. `160,000円`, `１２７,０００円`)
/// - `(?P<b>...)` : `￥` / `¥` プレフィックス (e.g. `￥160,000`)
/// - `(?P<c>...)` : `JPY` プレフィックス、大文字小文字無視 (e.g. `JPY 127,000`)
static AMOUNT_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(concat!(
        r"(?P<a>[0-9\x{FF10}-\x{FF19}][0-9\x{FF10}-\x{FF19},\x{FF0C}]*\s*円)",
        r"|",
        r"(?P<b>[\x{FFE5}\x{00A5}]\s*[0-9\x{FF10}-\x{FF19}][0-9\x{FF10}-\x{FF19},\x{FF0C}]*)",
        r"|",
        r"(?P<c>(?i:JPY)\s*[0-9\x{FF10}-\x{FF19}][0-9\x{FF10}-\x{FF19},\x{FF0C}]*)"
    ))
    .expect("AMOUNT_RE regex must compile")
});

/// 1 件の金額検出結果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AmountMatch {
    /// 元文字列での開始バイトオフセット
    pub byte_start: usize,
    /// 元文字列での終了バイトオフセット (exclusive)
    pub byte_end: usize,
    /// マッチした生の文字列 (e.g. `"115,000円"`, `"￥160,000"`, `"JPY 127,000"`)
    pub raw: String,
    /// 半角化・カンマ除去後の正規化された円額 (i64)。
    /// 想定外フォーマット (overflow 等) の場合は 0 を入れる。
    pub yen: i64,
}

/// テキスト中の金額表現を全て [`MASK_TOKEN`] (`***円`) に置換した新文字列を返す。
///
/// `***円` は再度 `mask_amounts` を適用しても変化しない (idempotent)。distribute /
/// viewer 側で念のため再適用しても安全。
pub fn mask_amounts(text: &str) -> String {
    AMOUNT_RE.replace_all(text, MASK_TOKEN).into_owned()
}

/// テキスト中の金額表現を検出するのみ。置換は行わない (検査・テスト用途)。
pub fn detect_amounts(text: &str) -> Vec<AmountMatch> {
    AMOUNT_RE
        .find_iter(text)
        .map(|m| AmountMatch {
            byte_start: m.start(),
            byte_end: m.end(),
            raw: m.as_str().to_string(),
            yen: normalize_yen(m.as_str()),
        })
        .collect()
}

/// `ExtractionResult` の各文字列フィールドを破壊的にマスクする。
///
/// マスク対象:
/// - `title` (Option<String>)
/// - `summary` (Option<String>)
/// - `data` (JSONB) — 全 String 葉を再帰走査
///
/// マスク非対象:
/// - `date` (NaiveDate、構造体型)
/// - `phone_numbers` (Vec<String>、電話番号は通常 `円` を含まないので idempotent。
///   テストで「不変であること」を assert)
/// - JSON Number 葉 (`{"amount": 127000}` のような数値) — TODO で別 PR
pub fn mask_amounts_in_extraction(extraction: &mut ExtractionResult) {
    if let Some(title) = extraction.title.as_mut() {
        *title = mask_amounts(title);
    }
    if let Some(summary) = extraction.summary.as_mut() {
        *summary = mask_amounts(summary);
    }
    mask_amounts_in_json(&mut extraction.data);
}

/// `serde_json::Value` を再帰走査し、String 葉だけマスク。Number / Bool / Null は触らない。
fn mask_amounts_in_json(value: &mut Value) {
    match value {
        Value::String(s) => {
            *s = mask_amounts(s);
        }
        Value::Array(arr) => {
            for v in arr.iter_mut() {
                mask_amounts_in_json(v);
            }
        }
        Value::Object(map) => {
            for v in map.values_mut() {
                mask_amounts_in_json(v);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
}

/// マッチ文字列から円額を i64 で取り出す。
fn normalize_yen(raw: &str) -> i64 {
    let mut buf = String::with_capacity(raw.len());
    for ch in raw.chars() {
        match ch {
            '0'..='9' => buf.push(ch),
            // 全角数字 U+FF10..=U+FF19 → 半角
            '\u{FF10}'..='\u{FF19}' => {
                let half = ((ch as u32) - 0xFF10 + b'0' as u32) as u8 as char;
                buf.push(half);
            }
            _ => {} // 円, ￥, ¥, JPY, j, p, y, ',', '，', whitespace は捨てる
        }
    }
    buf.parse::<i64>().unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use proptest::prelude::*;
    use serde_json::json;

    // ---------------------------------------------------------------------
    // 1. 基本ケース: 半角・全角・記号付き
    // ---------------------------------------------------------------------

    #[test]
    fn test_mask_basic_yen() {
        assert_eq!(mask_amounts("160,000円"), "***円");
    }

    #[test]
    fn test_mask_no_comma() {
        assert_eq!(mask_amounts("5000円"), "***円");
    }

    #[test]
    fn test_mask_single_digit() {
        assert_eq!(mask_amounts("5円"), "***円");
    }

    #[test]
    fn test_mask_zero() {
        assert_eq!(mask_amounts("0円"), "***円");
    }

    #[test]
    fn test_mask_with_tax_annotation() {
        assert_eq!(mask_amounts("115,000円 (税別)"), "***円 (税別)");
    }

    #[test]
    fn test_mask_yen_symbol() {
        assert_eq!(mask_amounts("￥160,000"), "***円");
        assert_eq!(mask_amounts("¥160,000"), "***円");
        assert_eq!(mask_amounts("￥ 160,000"), "***円");
    }

    #[test]
    fn test_mask_jpy_prefix() {
        assert_eq!(mask_amounts("JPY 127,000"), "***円");
        assert_eq!(mask_amounts("jpy 127000"), "***円");
        assert_eq!(mask_amounts("Jpy127,000"), "***円");
    }

    #[test]
    fn test_mask_full_width_digits() {
        assert_eq!(mask_amounts("１２７,０００円"), "***円");
    }

    #[test]
    fn test_mask_full_width_comma() {
        assert_eq!(mask_amounts("127，000円"), "***円");
    }

    #[test]
    fn test_mask_multiple_in_one_line() {
        let input = "運賃 115,000円 消費税 11,500円 合計 126,500円";
        let expected = "運賃 ***円 消費税 ***円 合計 ***円";
        assert_eq!(mask_amounts(input), expected);
    }

    // ---------------------------------------------------------------------
    // 2. 誤検知防止: 円なし数字パターンが不変であること
    // ---------------------------------------------------------------------

    #[test]
    fn test_no_mask_naked_number() {
        assert_eq!(mask_amounts("160,000"), "160,000");
    }

    #[test]
    fn test_no_mask_time() {
        assert_eq!(mask_amounts("10:00 集合"), "10:00 集合");
    }

    #[test]
    fn test_no_mask_weight_unit() {
        assert_eq!(mask_amounts("10t 積載"), "10t 積載");
    }

    #[test]
    fn test_no_mask_pallet_unit() {
        assert_eq!(mask_amounts("9PL 前後"), "9PL 前後");
    }

    #[test]
    fn test_no_mask_phone_number() {
        // 3 PDF 全てに登場する電話・FAX 番号
        assert_eq!(mask_amounts("0799-45-1688"), "0799-45-1688");
        assert_eq!(mask_amounts("080-5805-6060"), "080-5805-6060");
        assert_eq!(mask_amounts("FAX 0799-24-7424"), "FAX 0799-24-7424");
    }

    #[test]
    fn test_no_mask_postal() {
        assert_eq!(mask_amounts("〒100-0001"), "〒100-0001");
        assert_eq!(mask_amounts("〒656-0017"), "〒656-0017");
    }

    // ---------------------------------------------------------------------
    // 3. detect_amounts: 構造化検出 (バイトオフセット + 正規化値)
    // ---------------------------------------------------------------------

    #[test]
    fn test_detect_amounts_byte_offsets() {
        let text = "運賃115,000円";
        let matches = detect_amounts(text);
        assert_eq!(matches.len(), 1);
        let m = &matches[0];
        assert_eq!(m.raw, "115,000円");
        assert_eq!(m.yen, 115_000);
        // "運賃" は UTF-8 で 6 バイト (3 バイト × 2 文字)
        assert_eq!(m.byte_start, "運賃".len());
        assert_eq!(m.byte_end, text.len());
        // text[start..end] で raw を再現できること
        assert_eq!(&text[m.byte_start..m.byte_end], m.raw);
    }

    #[test]
    fn test_detect_amounts_full_width_normalized() {
        let matches = detect_amounts("１２７,０００円");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].yen, 127_000);
    }

    #[test]
    fn test_detect_amounts_yen_symbol_normalized() {
        let matches = detect_amounts("￥160,000");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].yen, 160_000);
        assert_eq!(matches[0].raw, "￥160,000");
    }

    #[test]
    fn test_detect_amounts_jpy_normalized() {
        let matches = detect_amounts("jpy 127000");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].yen, 127_000);
    }

    #[test]
    fn test_detect_amounts_empty() {
        assert!(detect_amounts("phone 0799-45-1688").is_empty());
    }

    #[test]
    fn test_detect_amounts_overflow_returns_zero() {
        // i64 の最大は 9_223_372_036_854_775_807 (19 桁)。20 桁以上にして overflow 確認。
        let matches = detect_amounts("99999999999999999999円");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].yen, 0); // unwrap_or(0) のフォールバック
    }

    // ---------------------------------------------------------------------
    // 4. ExtractionResult / JSON 葉のマスク
    // ---------------------------------------------------------------------

    #[test]
    fn test_mask_extraction_summary_data() {
        let mut e = ExtractionResult {
            title: Some("作業依頼書".to_string()),
            date: Some(NaiveDate::from_ymd_opt(2026, 4, 28).unwrap()),
            summary: Some("代金 160,000円 です".to_string()),
            phone_numbers: vec!["0799-45-1688".to_string(), "0191-47-3131".to_string()],
            data: json!({
                "amount_text": "160,000円",
                "notes": "税抜 160,000円 で確定",
            }),
        };

        mask_amounts_in_extraction(&mut e);

        // title: 円を含まないので不変
        assert_eq!(e.title.as_deref(), Some("作業依頼書"));
        // date: 触らない (NaiveDate)
        assert_eq!(e.date, Some(NaiveDate::from_ymd_opt(2026, 4, 28).unwrap()));
        // summary: 金額部分のみ置換
        assert_eq!(e.summary.as_deref(), Some("代金 ***円 です"));
        // phone_numbers: 円を含まないので不変
        assert_eq!(
            e.phone_numbers,
            vec!["0799-45-1688".to_string(), "0191-47-3131".to_string()]
        );
        // data: String 葉が置換
        assert_eq!(e.data["amount_text"], "***円");
        assert_eq!(e.data["notes"], "税抜 ***円 で確定");
    }

    #[test]
    fn test_mask_extraction_nested_json() {
        let mut e = ExtractionResult {
            title: None,
            date: None,
            summary: None,
            phone_numbers: vec![],
            data: json!({
                "level1": {
                    "level2": {
                        "level3": "100円"
                    },
                    "array": ["50円", "200円", "数字なし"]
                }
            }),
        };
        mask_amounts_in_extraction(&mut e);
        assert_eq!(e.data["level1"]["level2"]["level3"], "***円");
        assert_eq!(e.data["level1"]["array"][0], "***円");
        assert_eq!(e.data["level1"]["array"][1], "***円");
        assert_eq!(e.data["level1"]["array"][2], "数字なし");
    }

    #[test]
    fn test_mask_extraction_json_number_leaves_unchanged() {
        // 仕様: Number 葉は今回マスクしない。Gemini が `{"amount": 127000}` で返した場合は
        // 漏れる (TODO: 別 PR で対応)。この test がその仕様を明示する。
        let mut e = ExtractionResult {
            title: None,
            date: None,
            summary: None,
            phone_numbers: vec![],
            data: json!({
                "amount_number": 127000,
                "is_paid": true,
                "memo": null,
                "amount_text": "127,000円",
            }),
        };
        mask_amounts_in_extraction(&mut e);
        assert_eq!(e.data["amount_number"], 127_000); // Number 不変
        assert_eq!(e.data["is_paid"], true); // Bool 不変
        assert!(e.data["memo"].is_null()); // Null 不変
        assert_eq!(e.data["amount_text"], "***円"); // String のみマスク
    }

    #[test]
    fn test_mask_extraction_all_none_no_panic() {
        let mut e = ExtractionResult {
            title: None,
            date: None,
            summary: None,
            phone_numbers: vec![],
            data: Value::Null,
        };
        mask_amounts_in_extraction(&mut e);
        assert!(e.title.is_none());
        assert!(e.summary.is_none());
        assert!(e.data.is_null());
    }

    // ---------------------------------------------------------------------
    // 5. Idempotency: 二重マスク安全性 (distribute / viewer での再適用に必要)
    // ---------------------------------------------------------------------

    #[test]
    fn test_idempotent() {
        let inputs = [
            "代金 160,000円 + 消費税 16,000円",
            "￥160,000 / JPY 127000 / １,２３４円",
            "phone 0799-45-1688",
            "",
            "***円", // 既にマスク済み
        ];
        for s in inputs {
            let once = mask_amounts(s);
            let twice = mask_amounts(&once);
            assert_eq!(once, twice, "idempotency violated for {s:?}");
        }
    }

    // ---------------------------------------------------------------------
    // 6. Property-based: 任意 N について円付き → ***円, 円なし → 不変
    // ---------------------------------------------------------------------

    proptest! {
        #[test]
        fn prop_amount_with_yen_always_masked(n in 0u64..1_000_000_000u64) {
            let with_yen = format!("{n}円");
            prop_assert_eq!(mask_amounts(&with_yen), "***円".to_string());
        }

        #[test]
        fn prop_naked_number_no_mask(n in 0u64..1_000_000_000u64) {
            // 「円」もない、円記号もない、JPY も無い裸数字は不変
            let bare = format!("{n}");
            prop_assert_eq!(mask_amounts(&bare).clone(), bare);
        }

        #[test]
        fn prop_jpy_prefix_always_masked(n in 0u64..1_000_000u64) {
            let jpy = format!("JPY {n}");
            prop_assert_eq!(mask_amounts(&jpy), "***円".to_string());
        }
    }

    // ---------------------------------------------------------------------
    // 7. Fixture-based: 3 PDF (3163/3164/3165) の再現テキスト・JSON
    // ---------------------------------------------------------------------

    #[test]
    fn test_fixture_3163_text() {
        let input = include_str!("../tests/fixtures/mask/3163_text.input.txt");
        let expected = include_str!("../tests/fixtures/mask/3163_text.expected.txt");
        assert_eq!(mask_amounts(input), expected);
    }

    #[test]
    fn test_fixture_3164_text() {
        let input = include_str!("../tests/fixtures/mask/3164_text.input.txt");
        let expected = include_str!("../tests/fixtures/mask/3164_text.expected.txt");
        assert_eq!(mask_amounts(input), expected);
    }

    #[test]
    fn test_fixture_3165_text() {
        let input = include_str!("../tests/fixtures/mask/3165_text.input.txt");
        let expected = include_str!("../tests/fixtures/mask/3165_text.expected.txt");
        assert_eq!(mask_amounts(input), expected);
    }

    fn run_json_fixture(input_str: &str, expected_str: &str) {
        let input: Value = serde_json::from_str(input_str).expect("input json");
        let expected: Value = serde_json::from_str(expected_str).expect("expected json");

        let mut e = ExtractionResult {
            title: input
                .get("title")
                .and_then(|v| v.as_str())
                .map(String::from),
            date: input
                .get("date")
                .and_then(|v| v.as_str())
                .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()),
            summary: input
                .get("summary")
                .and_then(|v| v.as_str())
                .map(String::from),
            phone_numbers: input
                .get("phone_numbers")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|x| x.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default(),
            data: input.get("data").cloned().unwrap_or(Value::Null),
        };

        mask_amounts_in_extraction(&mut e);

        assert_eq!(
            e.title.as_deref(),
            expected.get("title").and_then(|v| v.as_str())
        );
        assert_eq!(
            e.summary.as_deref(),
            expected.get("summary").and_then(|v| v.as_str())
        );
        let expected_phones: Vec<String> = expected
            .get("phone_numbers")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|x| x.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        assert_eq!(e.phone_numbers, expected_phones);
        assert_eq!(&e.data, expected.get("data").unwrap_or(&Value::Null));
    }

    #[test]
    fn test_fixture_3163_extraction_json() {
        run_json_fixture(
            include_str!("../tests/fixtures/mask/3163_extraction.input.json"),
            include_str!("../tests/fixtures/mask/3163_extraction.expected.json"),
        );
    }

    #[test]
    fn test_fixture_3164_extraction_json() {
        run_json_fixture(
            include_str!("../tests/fixtures/mask/3164_extraction.input.json"),
            include_str!("../tests/fixtures/mask/3164_extraction.expected.json"),
        );
    }

    #[test]
    fn test_fixture_3165_extraction_json() {
        run_json_fixture(
            include_str!("../tests/fixtures/mask/3165_extraction.input.json"),
            include_str!("../tests/fixtures/mask/3165_extraction.expected.json"),
        );
    }

    // ---------------------------------------------------------------------
    // 8. 定数・正規表現コンパイルの sanity check (lazy 初期化を強制トリガー)
    // ---------------------------------------------------------------------

    #[test]
    fn test_mask_token_constant() {
        assert_eq!(MASK_TOKEN, "***円");
        // MASK_TOKEN 自身は再マスクで不変であること
        assert_eq!(mask_amounts(MASK_TOKEN), MASK_TOKEN);
    }

    #[test]
    fn test_amount_match_debug_clone_eq() {
        // AmountMatch derive(Debug, Clone, PartialEq, Eq) のカバレッジ確保
        let m1 = AmountMatch {
            byte_start: 0,
            byte_end: 5,
            raw: "100円".to_string(),
            yen: 100,
        };
        let m2 = m1.clone();
        assert_eq!(m1, m2);
        assert!(format!("{m1:?}").contains("100円"));
    }
}
