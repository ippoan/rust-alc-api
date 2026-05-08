//! 配車系 PDF から Gemini で 8 フィールドを抽出する:
//! 「積地・卸地・積み日時・卸し日時・注意事項・連絡先会社名・担当者・電話番号」
//!
//! 受信した FAX/PDF が配車手配票だった場合、配信時の LINE 本文に要点を埋め込めるように
//! `notify_documents.extracted_data` JSONB の `logistics` キー配下に 8 フィールドを保存する。
//! 該当情報がない PDF (請求書・報告書等) は全フィールド `null` で確定し、配信本文は
//! 既存テンプレ (タイトル + summary + URL) にフォールバックする。
//!
//! ## 「自社」と「相手先」の区別
//!
//! 配車手配票には自社の連絡先 (受注側) と相手先の連絡先 (依頼元・荷主) の両方が記載される。
//! LINE 受信者にとって有用なのは **相手先** の連絡先なので、`self_company_hint` (テナント名)
//! を Gemini プロンプトに渡して自社情報を除外させる。
//!
//! 自社判定の手がかり:
//!   - テナント名 (DB `tenants.name`) と一致する会社名 → 自社
//!   - 「依頼元」「お客様」「荷主」「発注元」ラベル付き → 相手先
//!   - 「受注」「ドライバー」「配車」ラベル付き → 自社
//!
//! 区別がつかない時は null (誤抽出より under-extract を選ぶ)。
//!
//! 設計方針は `crates/alc-notify/src/redact.rs` と同形:
//!   - `LOGISTICS_PROMPT` でプロンプト本体を const 化
//!   - `logistics_response_schema()` で Structured Output schema を pin
//!     (`responseMimeType=application/json` だけだと Gemini が markdown wrap してきて
//!     parse 失敗するため。PR #318 の教訓 / `feedback_gemini_response_schema_required.md`)
//!   - `endpoint` 引数を `Option<&str>` で受け取り wiremock 化
//!   - pure 関数 (`build_extract_request_body`, `parse_extract_response`) はテストで直接叩く

use base64::Engine;

const GEMINI_DEFAULT_ENDPOINT: &str = "https://generativelanguage.googleapis.com/v1beta";
// redact と同じ。stable suffix が付いたら更新。
const GEMINI_DEFAULT_MODEL: &str = "gemini-3.1-flash-lite-preview";

/// プロンプト本文を組み立てる。`self_company_hint` (テナント名) があれば自社除外
/// セクションを注入する。
///
/// PDF が配車系でない (例: 請求書) 場合に「無理にどこかから値を埋める」のを防ぐため、
/// 「該当情報がなければ null を返す」を明示する。日付は ISO 正規化せず原文表記をそのまま
/// 保つ — フロントで表示するだけなので、表記のばらつきを受け入れる方が正確。
pub(crate) fn build_logistics_prompt(self_company_hint: Option<&str>) -> String {
    let self_section = match self_company_hint {
        Some(name) if !name.trim().is_empty() => format!(
            "\n\n## 自社情報 (除外対象)\n\
             この PDF を処理するテナント (= 自社) は **「{}」** です。\n\
             連絡先 3 フィールド (contact_company / contact_person / contact_phone) は\n\
             **自社ではなく相手先 (依頼元・発注元・荷主・お客様) のものを抽出**してください。\n\
             - 自社名 (上記) が文書ヘッダ/フッタ/印鑑欄/受注欄にある場合は除外\n\
             - 自社の電話番号・FAX 番号・担当者名と一致するものは除外\n\
             - 自社しか書かれていない場合は連絡先 3 フィールドを全て null にする",
            name.trim()
        ),
        _ => String::from(
            "\n\n## 連絡先の判定\n\
             連絡先 3 フィールドは「相手先 (依頼元・発注元・荷主・お客様)」のものを抽出する。\n\
             ラベル例: 「依頼元」「お客様」「荷主」「発注元」「ご依頼元」「お得意様」「貴社」「申込元」。\n\
             逆に「受注」「ドライバー」「業務担当」「配車」「運転手」のラベル、文書ヘッダ/印鑑/\n\
             差出人欄の会社名は **自社側** なので除外する。\n\
             相手先と自社の区別がつかない場合は 3 フィールドとも null にする (誤抽出より null が良い)。",
        ),
    };

    format!(
        r#"この PDF は運送業務で受信した文書です。配車手配票・運行依頼書であれば
以下 8 フィールドを抽出してください:

  - loading_place    : 積地 (積み込み場所、住所か会社名のうち主要なもの 1 件)
  - unloading_place  : 卸地 (荷卸し場所、住所か会社名のうち主要なもの 1 件)
  - loading_at       : 積み日時 (PDF の表記をそのまま、例: "5/9 10:00" / "5月9日 午前10時" / "2026-05-09 10:00")
  - unloading_at     : 卸し日時 (同上)
  - notes            : 注意事項 (冷凍便、要時間厳守、要連絡など配送上の留意点。複数あれば改行で結合)
  - contact_company  : 連絡先の会社名 (相手先 = 依頼元・発注元・お客様の会社名)
  - contact_person   : 担当者氏名 (相手先の担当者)
  - contact_phone    : 電話番号 (相手先の TEL/連絡先電話、PDF の表記そのまま)

## ルール

  - PDF が配車手配票でない (請求書・報告書・通知文など) 場合、または該当情報がない場合は
    そのフィールドを null にする。**推測で埋めない、書かれていないものは null**。
  - 全フィールド null になっても OK (配信本文は既存テンプレにフォールバックする)。
  - 日付は ISO 8601 に正規化しない。PDF の表記を**そのまま**返す。
  - 積地/卸地は住所と会社名を改行で連結したり全部入れたりせず、最も識別性の高い 1 行を選ぶ。
  - notes は複数項目を改行 (\n) で連結する。
  - 電話番号はハイフンの有無や市外局番形式を変更せず、PDF の表記をそのまま。
  - 出力はこの schema に厳密に従う JSON 1 個 (前後に余分なテキスト・markdown 装飾を一切付けない)。{self_section}"#,
        self_section = self_section
    )
}

/// 抽出結果。`Option<String>` フィールドのみで構成し、どれが取れたかをキー存在で表現する。
/// `notify_documents.extracted_data.logistics` 配下に serde_json で保存する。
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize, Default)]
pub struct LogisticsFields {
    /// 積地。例: "東京都港区" / "○○倉庫"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loading_place: Option<String>,
    /// 卸地。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unloading_place: Option<String>,
    /// 積み日時。例: "5/9 10:00"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loading_at: Option<String>,
    /// 卸し日時。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unloading_at: Option<String>,
    /// 注意事項。複数行可 (改行で連結)。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    /// 連絡先 (相手先) 会社名。**自社は含めない**。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contact_company: Option<String>,
    /// 連絡先 (相手先) 担当者氏名。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contact_person: Option<String>,
    /// 連絡先 (相手先) 電話番号。PDF の表記そのまま。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contact_phone: Option<String>,
}

impl LogisticsFields {
    /// 1 つでも非空の値があれば true。配信本文の物流テンプレ分岐に使う。
    pub fn has_any(&self) -> bool {
        [
            &self.loading_place,
            &self.unloading_place,
            &self.loading_at,
            &self.unloading_at,
            &self.notes,
            &self.contact_company,
            &self.contact_person,
            &self.contact_phone,
        ]
        .iter()
        .any(|v| v.as_ref().is_some_and(|s| !s.trim().is_empty()))
    }
}

#[derive(thiserror::Error, Debug)]
pub enum ExtractError {
    #[error("gemini http: {0}")]
    GeminiHttp(#[from] reqwest::Error),
    #[error("gemini status {0}: {1}")]
    GeminiStatus(reqwest::StatusCode, String),
    #[error("gemini empty response")]
    GeminiEmpty,
    #[error("gemini json parse: {0}")]
    GeminiParse(serde_json::Error),
    #[error("logistics json parse: {0}")]
    LogisticsParse(serde_json::Error),
}

/// Structured Output schema。Gemini が schema に合致する JSON を保証する。
///
/// `responseMimeType: application/json` だけだと markdown wrap で parse 壊れる
/// (PR #318 / `feedback_gemini_response_schema_required.md`)。schema で構造を pin する。
///
/// 8 フィールドは全部 nullable な STRING。`required` には入れない (Gemini が
/// 該当情報なしと判断した時に null にできるように)。
fn logistics_response_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "OBJECT",
        "properties": {
            "loading_place":   { "type": "STRING", "nullable": true },
            "unloading_place": { "type": "STRING", "nullable": true },
            "loading_at":      { "type": "STRING", "nullable": true },
            "unloading_at":    { "type": "STRING", "nullable": true },
            "notes":           { "type": "STRING", "nullable": true },
            "contact_company": { "type": "STRING", "nullable": true },
            "contact_person":  { "type": "STRING", "nullable": true },
            "contact_phone":   { "type": "STRING", "nullable": true }
        },
        "propertyOrdering": [
            "loading_place", "unloading_place",
            "loading_at", "unloading_at", "notes",
            "contact_company", "contact_person", "contact_phone"
        ]
    })
}

/// Gemini の generateContent リクエスト body を組み立てる pure 関数。
///
/// `pdf_b64` は base64 (no-pad もしくは standard) 済み PDF。
/// `self_company_hint` は自社除外用テナント名 (例: tenants.name)。None / 空文字なら
/// プロンプトはラベル推論のみで自社判定する。
///
/// 実 API 呼び出しを伴わないので単体テストで request body の中身 (prompt 本文 /
/// responseSchema の properties / temperature 等) を直接アサートできる。
pub(crate) fn build_extract_request_body(
    pdf_b64: &str,
    self_company_hint: Option<&str>,
) -> serde_json::Value {
    serde_json::json!({
        "contents": [{
            "role": "user",
            "parts": [
                { "inlineData": { "mimeType": "application/pdf", "data": pdf_b64 } },
                { "text": build_logistics_prompt(self_company_hint) }
            ]
        }],
        "generationConfig": {
            "temperature": 0.0,
            "responseMimeType": "application/json",
            "responseSchema": logistics_response_schema(),
            // 8 フィールド × 短いテキストなので 768 で十分余裕。
            "maxOutputTokens": 768
        }
    })
}

/// Gemini の generateContent レスポンス JSON から `LogisticsFields` を取り出す。
///
/// 期待構造:
///   `candidates[0].content.parts[0].text` に schema 準拠の JSON 文字列が入っている。
///   それを `LogisticsFields` として deserialize する。
///
/// `text` 不在 → `GeminiEmpty`、JSON parse 失敗 → `LogisticsParse`。
pub(crate) fn parse_extract_response(
    parsed: &serde_json::Value,
) -> Result<LogisticsFields, ExtractError> {
    let text = parsed
        .pointer("/candidates/0/content/parts/0/text")
        .and_then(|v| v.as_str())
        .ok_or(ExtractError::GeminiEmpty)?;

    serde_json::from_str::<LogisticsFields>(text).map_err(|e| {
        tracing::warn!(
            "extract: parse failed: {e}; raw response (first 500 chars): {}",
            text.chars().take(500).collect::<String>()
        );
        ExtractError::LogisticsParse(e)
    })
}

/// Gemini API を叩いて配車情報 8 フィールドを抽出する。
///
/// `endpoint` には prod では `GEMINI_DEFAULT_ENDPOINT` / `model` には `GEMINI_DEFAULT_MODEL` を、
/// テストでは `wiremock::MockServer::uri()` と任意 model を渡す。
/// `self_company_hint` には tenants.name を渡す。None / 空文字でも動くがラベル推論のみ。
pub async fn extract_logistics_fields_with_endpoint(
    endpoint: &str,
    model: &str,
    pdf_bytes: &[u8],
    api_key: &str,
    self_company_hint: Option<&str>,
) -> Result<LogisticsFields, ExtractError> {
    let client = reqwest::Client::new();
    let url = format!("{endpoint}/models/{model}:generateContent?key={api_key}");
    let pdf_b64 = base64::engine::general_purpose::STANDARD.encode(pdf_bytes);

    let body = build_extract_request_body(&pdf_b64, self_company_hint);
    let resp = client.post(&url).json(&body).send().await?;
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(ExtractError::GeminiStatus(status, body));
    }
    let raw = resp.text().await?;
    let parsed: serde_json::Value =
        serde_json::from_str(&raw).map_err(ExtractError::GeminiParse)?;
    parse_extract_response(&parsed)
}

/// prod 用ショートカット (`GEMINI_DEFAULT_ENDPOINT` + `GEMINI_DEFAULT_MODEL`)。
pub async fn extract_logistics_fields(
    pdf_bytes: &[u8],
    api_key: &str,
    self_company_hint: Option<&str>,
) -> Result<LogisticsFields, ExtractError> {
    extract_logistics_fields_with_endpoint(
        GEMINI_DEFAULT_ENDPOINT,
        GEMINI_DEFAULT_MODEL,
        pdf_bytes,
        api_key,
        self_company_hint,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    // ============================================================
    // pure 関数テスト
    // ============================================================

    #[test]
    fn schema_includes_all_eight_fields() {
        let schema = logistics_response_schema();
        let props = schema.pointer("/properties").unwrap().as_object().unwrap();
        for k in [
            "loading_place",
            "unloading_place",
            "loading_at",
            "unloading_at",
            "notes",
            "contact_company",
            "contact_person",
            "contact_phone",
        ] {
            assert!(props.contains_key(k), "schema missing field: {k}");
            // 全フィールド nullable
            let prop = &props[k];
            assert_eq!(prop["type"], "STRING");
            assert_eq!(prop["nullable"], true);
        }
        // ordering も 8 件揃っている
        let ordering = schema
            .pointer("/propertyOrdering")
            .unwrap()
            .as_array()
            .unwrap();
        assert_eq!(ordering.len(), 8);
    }

    #[test]
    fn build_request_body_has_inline_pdf_and_prompt() {
        let body = build_extract_request_body("AAAA", None);
        // PDF が inlineData として埋め込まれている
        let inline = body.pointer("/contents/0/parts/0/inlineData").unwrap();
        assert_eq!(inline["mimeType"], "application/pdf");
        assert_eq!(inline["data"], "AAAA");
        // プロンプトが text part に乗っている
        let prompt = body
            .pointer("/contents/0/parts/1/text")
            .and_then(|v| v.as_str())
            .unwrap();
        // 8 フィールド全て prompt 内に出現
        for k in [
            "loading_place",
            "unloading_place",
            "loading_at",
            "unloading_at",
            "notes",
            "contact_company",
            "contact_person",
            "contact_phone",
        ] {
            assert!(prompt.contains(k), "prompt missing field: {k}");
        }
        // Structured Output 強制 (regression guard, PR #318)
        assert_eq!(
            body.pointer("/generationConfig/responseMimeType").unwrap(),
            "application/json"
        );
        assert!(body.pointer("/generationConfig/responseSchema").is_some());
        // temperature=0 で deterministic
        assert_eq!(body.pointer("/generationConfig/temperature").unwrap(), 0.0);
    }

    #[test]
    fn build_request_body_includes_self_company_hint_when_provided() {
        let body = build_extract_request_body("AAAA", Some("テスト運輸株式会社"));
        let prompt = body
            .pointer("/contents/0/parts/1/text")
            .and_then(|v| v.as_str())
            .unwrap();
        assert!(prompt.contains("テスト運輸株式会社"));
        assert!(prompt.contains("自社情報 (除外対象)"));
        // 「依頼元」「お客様」等のラベル推論セクションは hint なし側
        assert!(!prompt.contains("## 連絡先の判定\n"));
    }

    #[test]
    fn build_request_body_uses_label_inference_when_hint_empty() {
        let body = build_extract_request_body("AAAA", Some("   "));
        let prompt = body
            .pointer("/contents/0/parts/1/text")
            .and_then(|v| v.as_str())
            .unwrap();
        // 空白だけの hint は None と同じ扱い
        assert!(!prompt.contains("自社情報 (除外対象)"));
        assert!(prompt.contains("## 連絡先の判定"));
        assert!(prompt.contains("依頼元"));
    }

    #[test]
    fn build_logistics_prompt_explicit_none_uses_label_inference() {
        let p = build_logistics_prompt(None);
        assert!(!p.contains("自社情報 (除外対象)"));
        assert!(p.contains("依頼元"));
        assert!(p.contains("ドライバー"));
    }

    #[test]
    fn parse_response_happy_path() {
        let resp = serde_json::json!({
            "candidates": [{
                "content": { "parts": [{ "text":
                    "{\"loading_place\":\"東京都港区\",\
                     \"unloading_place\":\"大阪市\",\
                     \"loading_at\":\"5/9 10:00\",\
                     \"unloading_at\":\"5/10 14:00\",\
                     \"notes\":\"冷凍便\\n要時間厳守\",\
                     \"contact_company\":\"ABC運送\",\
                     \"contact_person\":\"田中太郎\",\
                     \"contact_phone\":\"03-1234-5678\"}"
                }]}
            }]
        });
        let fields = parse_extract_response(&resp).unwrap();
        assert_eq!(fields.loading_place.as_deref(), Some("東京都港区"));
        assert_eq!(fields.unloading_place.as_deref(), Some("大阪市"));
        assert_eq!(fields.loading_at.as_deref(), Some("5/9 10:00"));
        assert_eq!(fields.unloading_at.as_deref(), Some("5/10 14:00"));
        assert_eq!(fields.notes.as_deref(), Some("冷凍便\n要時間厳守"));
        assert_eq!(fields.contact_company.as_deref(), Some("ABC運送"));
        assert_eq!(fields.contact_person.as_deref(), Some("田中太郎"));
        assert_eq!(fields.contact_phone.as_deref(), Some("03-1234-5678"));
        assert!(fields.has_any());
    }

    #[test]
    fn parse_response_all_nulls_is_valid() {
        let resp = serde_json::json!({
            "candidates": [{
                "content": { "parts": [{ "text":
                    "{\"loading_place\":null,\"unloading_place\":null,\
                     \"loading_at\":null,\"unloading_at\":null,\"notes\":null,\
                     \"contact_company\":null,\"contact_person\":null,\
                     \"contact_phone\":null}"
                }]}
            }]
        });
        let fields = parse_extract_response(&resp).unwrap();
        assert!(fields.loading_place.is_none());
        assert!(fields.unloading_place.is_none());
        assert!(fields.loading_at.is_none());
        assert!(fields.unloading_at.is_none());
        assert!(fields.notes.is_none());
        assert!(fields.contact_company.is_none());
        assert!(fields.contact_person.is_none());
        assert!(fields.contact_phone.is_none());
        assert!(!fields.has_any());
    }

    #[test]
    fn parse_response_only_contact_fields() {
        // 配車情報なし、連絡先 3 つのみある PDF (例: 取引先からのお知らせ)
        let resp = serde_json::json!({
            "candidates": [{
                "content": { "parts": [{ "text":
                    "{\"contact_company\":\"取引先株式会社\",\
                     \"contact_person\":\"山田\",\
                     \"contact_phone\":\"06-1234-5678\"}"
                }]}
            }]
        });
        let fields = parse_extract_response(&resp).unwrap();
        assert_eq!(fields.contact_company.as_deref(), Some("取引先株式会社"));
        assert_eq!(fields.contact_person.as_deref(), Some("山田"));
        assert_eq!(fields.contact_phone.as_deref(), Some("06-1234-5678"));
        assert!(fields.loading_place.is_none());
        // contact だけでも has_any = true
        assert!(fields.has_any());
    }

    #[test]
    fn parse_response_partial_fields() {
        // schema は required なし → 一部キー欠落でも default で埋まる
        let resp = serde_json::json!({
            "candidates": [{
                "content": { "parts": [{ "text":
                    "{\"loading_place\":\"東京\"}"
                }]}
            }]
        });
        let fields = parse_extract_response(&resp).unwrap();
        assert_eq!(fields.loading_place.as_deref(), Some("東京"));
        assert!(fields.unloading_place.is_none());
        assert!(fields.has_any());
    }

    #[test]
    fn parse_response_missing_text_is_empty() {
        let resp = serde_json::json!({"candidates": []});
        match parse_extract_response(&resp) {
            Err(ExtractError::GeminiEmpty) => {}
            other => panic!("expected GeminiEmpty, got {other:?}"),
        }
    }

    #[test]
    fn parse_response_invalid_json_is_logistics_parse_err() {
        let resp = serde_json::json!({
            "candidates": [{
                "content": { "parts": [{ "text": "not a json" }]}
            }]
        });
        match parse_extract_response(&resp) {
            Err(ExtractError::LogisticsParse(_)) => {}
            other => panic!("expected LogisticsParse, got {other:?}"),
        }
    }

    #[test]
    fn has_any_treats_whitespace_as_empty() {
        let f = LogisticsFields {
            loading_place: Some("  ".into()),
            ..Default::default()
        };
        assert!(!f.has_any());
        let f = LogisticsFields {
            notes: Some("\n".into()),
            ..Default::default()
        };
        assert!(!f.has_any());
        let f = LogisticsFields {
            unloading_at: Some("a".into()),
            ..Default::default()
        };
        assert!(f.has_any());
    }

    #[test]
    fn logistics_fields_serializes_skipping_none() {
        let f = LogisticsFields {
            loading_place: Some("東京".into()),
            ..Default::default()
        };
        let json = serde_json::to_value(&f).unwrap();
        let obj = json.as_object().unwrap();
        // None フィールドは skip される (extracted_data に noise を残さない)
        assert!(obj.contains_key("loading_place"));
        assert!(!obj.contains_key("unloading_place"));
        assert!(!obj.contains_key("notes"));
    }

    // ============================================================
    // wiremock テスト (extract_logistics_fields_with_endpoint)
    // ============================================================

    async fn start_mock_with_text(text: &str) -> wiremock::MockServer {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .respond_with(
                wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "candidates": [{
                        "content": {"parts": [{"text": text}]}
                    }]
                })),
            )
            .mount(&server)
            .await;
        server
    }

    async fn start_mock_with_status(status: u16) -> wiremock::MockServer {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .respond_with(wiremock::ResponseTemplate::new(status).set_body_string("upstream error"))
            .mount(&server)
            .await;
        server
    }

    async fn start_mock_with_invalid_json() -> wiremock::MockServer {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .respond_with(wiremock::ResponseTemplate::new(200).set_body_string("not a json {{"))
            .mount(&server)
            .await;
        server
    }

    #[tokio::test]
    async fn extract_with_endpoint_success() {
        let server = start_mock_with_text(
            "{\"loading_place\":\"成田\",\"unloading_place\":\"福岡\",\
             \"loading_at\":\"5/9\",\"unloading_at\":\"5/10\",\"notes\":\"急ぎ\",\
             \"contact_company\":\"ABC運送\",\"contact_person\":\"田中\",\
             \"contact_phone\":\"03-1234-5678\"}",
        )
        .await;

        let fields = extract_logistics_fields_with_endpoint(
            &server.uri(),
            "test-model",
            b"%PDF-1.4",
            "test-key",
            None,
        )
        .await
        .unwrap();
        assert_eq!(fields.loading_place.as_deref(), Some("成田"));
        assert_eq!(fields.notes.as_deref(), Some("急ぎ"));
        assert_eq!(fields.contact_company.as_deref(), Some("ABC運送"));
        assert_eq!(fields.contact_person.as_deref(), Some("田中"));
        assert_eq!(fields.contact_phone.as_deref(), Some("03-1234-5678"));
    }

    #[tokio::test]
    async fn extract_with_endpoint_4xx_status() {
        let server = start_mock_with_status(400).await;
        let err = extract_logistics_fields_with_endpoint(
            &server.uri(),
            "test-model",
            b"x",
            "test-key",
            None,
        )
        .await
        .unwrap_err();
        match err {
            ExtractError::GeminiStatus(s, body) => {
                assert_eq!(s.as_u16(), 400);
                assert!(body.contains("upstream error"));
            }
            other => panic!("expected GeminiStatus, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn extract_with_endpoint_5xx_status() {
        let server = start_mock_with_status(500).await;
        let err = extract_logistics_fields_with_endpoint(
            &server.uri(),
            "test-model",
            b"x",
            "test-key",
            None,
        )
        .await
        .unwrap_err();
        assert!(matches!(err, ExtractError::GeminiStatus(_, _)));
    }

    #[tokio::test]
    async fn extract_with_endpoint_invalid_outer_json() {
        // candidates 構造ではない素文字列 → GeminiParse
        let server = start_mock_with_invalid_json().await;
        let err = extract_logistics_fields_with_endpoint(
            &server.uri(),
            "test-model",
            b"x",
            "test-key",
            None,
        )
        .await
        .unwrap_err();
        assert!(matches!(err, ExtractError::GeminiParse(_)));
    }

    #[tokio::test]
    async fn extract_with_endpoint_inner_text_unparseable() {
        // candidates は正しいが parts[0].text が JSON ではない → LogisticsParse
        let server = start_mock_with_text("this is not json").await;
        let err = extract_logistics_fields_with_endpoint(
            &server.uri(),
            "test-model",
            b"x",
            "test-key",
            None,
        )
        .await
        .unwrap_err();
        assert!(matches!(err, ExtractError::LogisticsParse(_)));
    }

    #[tokio::test]
    async fn extract_with_endpoint_empty_candidates() {
        // candidates 空 → text pointer 取れない → GeminiEmpty
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .respond_with(
                wiremock::ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"candidates": []})),
            )
            .mount(&server)
            .await;
        let err = extract_logistics_fields_with_endpoint(
            &server.uri(),
            "test-model",
            b"x",
            "test-key",
            None,
        )
        .await
        .unwrap_err();
        assert!(matches!(err, ExtractError::GeminiEmpty));
    }

    #[tokio::test]
    async fn extract_with_endpoint_unreachable_url() {
        // ポート 1 は unreachable な定義済み拒否ポート → reqwest::Error
        let err = extract_logistics_fields_with_endpoint(
            "http://127.0.0.1:1",
            "test-model",
            b"x",
            "test-key",
            None,
        )
        .await
        .unwrap_err();
        assert!(matches!(err, ExtractError::GeminiHttp(_)));
    }

    #[tokio::test]
    async fn extract_with_endpoint_passes_self_hint_in_request_body() {
        // mock 経由で self_company_hint がプロンプトに乗ることを確認 (request body inspection)
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::body_string_contains(
                "テスト運輸株式会社",
            ))
            .and(wiremock::matchers::body_string_contains("自社情報"))
            .respond_with(
                wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "candidates": [{
                        "content": {"parts": [{"text": "{\"contact_company\":\"相手先\"}"}]}
                    }]
                })),
            )
            .expect(1)
            .mount(&server)
            .await;

        let fields = extract_logistics_fields_with_endpoint(
            &server.uri(),
            "test-model",
            b"x",
            "test-key",
            Some("テスト運輸株式会社"),
        )
        .await
        .unwrap();
        assert_eq!(fields.contact_company.as_deref(), Some("相手先"));
    }

    // `extract_logistics_fields()` (prod ショートカット) は本物の Gemini エンドポイントを叩くので
    // unit test では呼び出さない (anti-pattern: 本番 API に直接 unit test を叩かせない)。
    // `extract_logistics_fields_with_endpoint` 経由で全分岐がカバーされている。

    #[test]
    fn default_endpoint_and_model_constants_point_to_prod() {
        // 定数値が prod を指していることのリグレッション検出。
        // 値が変わったら明示的に更新する (= レビュー時に気づける)。
        assert_eq!(
            GEMINI_DEFAULT_ENDPOINT,
            "https://generativelanguage.googleapis.com/v1beta"
        );
        assert_eq!(GEMINI_DEFAULT_MODEL, "gemini-3.1-flash-lite-preview");
    }
}
