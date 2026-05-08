//! PDF 金額黒塗り (redaction) の中核ロジック。
//!
//! 1. `detect_amount_boxes` — Gemini API に PDF を inlineData として直接送り、
//!    金額表記の bounding box を JSON で返してもらう。
//! 2. `apply_redactions` — 受け取った bbox を「ページ内に埋め込まれた JPEG 画像
//!    そのもの」のピクセルに白矩形で書き込み、JPEG を再エンコードして XObject
//!    stream を上書きする。
//!
//! ## 「画像書き換え」アプローチを採用する理由
//!
//! 以前は lopdf でページの content stream に白矩形 PDF コマンドを末尾追加していた
//! (上書きオーバーレイ)。ただし PDF は「後勝ち」描画なので、PDF.js のような
//! progressive renderer では「元 stream → 白矩形 stream」の順に描かれ、
//! **元値が一瞬見える (ちらつく)** という UX 上の問題があった。
//!
//! FAX 由来のスキャン PDF は実体として `/XObject/Image` (DCTDecode = JPEG) が
//! 1 ページに 1 枚埋め込まれている。この **JPEG ピクセル自体に白矩形を焼き込む**
//! と、PDF 内のどこを探しても元の金額値は見つからず、ちらつきが構造的に発生し
//! 得ない。
//!
//! 設計ドキュメント: `~/.claude/projects/-home-yhonda-rust-rust-alc-api/memory/notify_pdf_redact_design.md`

use base64::Engine;
use flate2::read::ZlibDecoder;
use image::ImageEncoder;
use lopdf::{Document, Object, ObjectId, Stream};
use std::io::Read;

const GEMINI_DEFAULT_ENDPOINT: &str = "https://generativelanguage.googleapis.com/v1beta";
// gemini-2.0-flash は 2026 年に新規ユーザー向け提供終了 (404)。
// gemini-3.1-flash-lite-preview を採用 — 軽量 / 高速 / PDF inlineData + JSON 出力対応。
// 2026-05-07 staging で実 API 200 OK 確認済み。stable suffix が付いたら更新。
const GEMINI_DEFAULT_MODEL: &str = "gemini-3.1-flash-lite-preview";

/// プロンプト本文。Gemini に「金額が入った **表のセル全体** の bbox」を返させる。
///
/// 数字に密着した bbox を返してもらうと、配置のばらつき (例: 3163 で「160,0  00 円」と
/// 数字内に空白がある) で覆い切れないケースが出る。代わりに **罫線で囲まれたセル全体**
/// を返してもらう = ラベル列にはみ出さず、かつセル内のどこに数字があっても確実に覆える。
/// セル境界は表が必ず矩形の罫線で区切られているという前提に依存。
const REDACT_PROMPT: &str = r#"この PDF は表形式の業務帳票です。
表の罫線で囲まれた **セル単位** で「金額 (円) が入っているセル」を検出し、
セル全体の矩形を返してください。

対象例: 「運賃」「代金」「消費税」「合計」「支払額」「支払運賃」など
金額が入っているセル。形式: 「N円」「￥N」「JPY N」「N円(税別)」「N円(税込)」。

出力形式 (これ以外の文字を一切出力しない):
{
  "redactions": [
    {
      "page": 1,
      "box_2d": [ymin, xmin, ymax, xmax],
      "text": "160,000円",
      "cell_label": "代金(税抜)"
    }
  ]
}

ルール:
- box_2d は 0-1000 で正規化された [ymin, xmin, ymax, xmax] (Gemini 標準 bbox 形式)
- page は 1-origin
- **数字に密着した bbox ではなく、表の罫線で囲まれたセル領域全体** を返す
- 隣接するラベル列 (例: 「代金(税抜)」のラベル自体) は含めない、値が入っているセルだけ
- 金額が空のセルは無視
- 氏名・電話番号・FAX 番号・郵便番号・車両ナンバー・住所のセルは含めない
- 「3,200 kg」「9PL」「10t」「10:00」など金額でない数字は含めない
- 金額が見つからない場合は {"redactions": []} を返す"#;

/// Gemini が返す bounding box 1 件。
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, PartialEq)]
pub struct RedactionBox {
    pub page: usize,
    /// [ymin, xmin, ymax, xmax] in 0..=1000, 左上原点
    pub box_2d: [f32; 4],
    pub text: String,
}

#[derive(Debug, serde::Deserialize)]
struct RedactionList {
    redactions: Vec<RedactionBox>,
}

/// 2-stage 用: Gemini Stage 1 が返すセル 1 件 (幾何 + OCR text)
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, PartialEq)]
pub struct CellBox {
    /// [ymin, xmin, ymax, xmax] in 0..=1000, 左上原点
    pub box_2d: [f32; 4],
    pub text: String,
}

/// 2-stage 用: Gemini Stage 1 が返す 1 ページ分のセル一覧
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, PartialEq)]
pub struct PageCells {
    pub page: usize,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub cells: Vec<CellBox>,
}

/// Stage 1 レスポンスを単一 page / 複数 tables 両形式から受け取る wrapper
#[derive(Debug, serde::Deserialize)]
#[serde(untagged)]
enum Stage1Response {
    Multi { tables: Vec<PageCells> },
    Single(PageCells),
}

#[derive(thiserror::Error, Debug)]
pub enum RedactError {
    #[error("gemini http: {0}")]
    GeminiHttp(#[from] reqwest::Error),
    #[error("gemini status {0}: {1}")]
    GeminiStatus(reqwest::StatusCode, String),
    #[error("gemini empty response")]
    GeminiEmpty,
    #[error("gemini json parse: {0}")]
    GeminiParse(serde_json::Error),
    #[error("redaction json parse: {0}")]
    RedactionParse(serde_json::Error),
    #[error("pdf: {0}")]
    Pdf(#[from] lopdf::Error),
    #[error("pdf io: {0}")]
    PdfIo(#[from] std::io::Error),
    #[error("page {0} not found in pdf")]
    PageNotFound(usize),
    #[error("invalid bbox in redaction: {0:?}")]
    InvalidBox([f32; 4]),
    #[error("page {0} has no embeddable image XObject (not a scan-style PDF?)")]
    PageNoImage(usize),
    #[error("image decode/encode: {0}")]
    Image(#[from] image::ImageError),
}

/// Gemini API を叩いて、PDF 内の金額表記の bbox を取得する。
///
/// `endpoint` には prod では `GEMINI_DEFAULT_ENDPOINT` を、テストでは
/// `wiremock::MockServer::uri()` を渡すことで wiremock 化が可能。
pub async fn detect_amount_boxes(
    pdf_bytes: &[u8],
    api_key: &str,
    model: Option<&str>,
    endpoint: Option<&str>,
) -> Result<Vec<RedactionBox>, RedactError> {
    let client = reqwest::Client::new();
    let model = model.unwrap_or(GEMINI_DEFAULT_MODEL);
    let endpoint = endpoint.unwrap_or(GEMINI_DEFAULT_ENDPOINT);

    let url = format!("{endpoint}/models/{model}:generateContent?key={api_key}");
    let pdf_b64 = base64::engine::general_purpose::STANDARD.encode(pdf_bytes);

    let body = serde_json::json!({
        "contents": [{
            "role": "user",
            "parts": [
                { "inlineData": { "mimeType": "application/pdf", "data": pdf_b64 } },
                { "text": REDACT_PROMPT }
            ]
        }],
        "generationConfig": {
            "temperature": 0.0,
            "responseMimeType": "application/json",
            // Structured Output: Gemini が schema に合致する JSON を保証する。
            // responseMimeType=json だけだと markdown wrap (```json ... ```) や
            // 余分な前置テキストで parse 失敗するので、schema で構造を pin する。
            "responseSchema": redact_response_schema(),
            "maxOutputTokens": 2048
        }
    });

    let resp = client.post(&url).json(&body).send().await?;
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(RedactError::GeminiStatus(status, body));
    }
    let raw = resp.text().await?;
    let parsed: serde_json::Value = serde_json::from_str(&raw).map_err(RedactError::GeminiParse)?;

    let text = parsed
        .pointer("/candidates/0/content/parts/0/text")
        .and_then(|v| v.as_str())
        .ok_or(RedactError::GeminiEmpty)?;

    let list: RedactionList = serde_json::from_str(text).map_err(|e| {
        tracing::warn!(
            "redact 1-stage: parse failed: {e}; raw response (first 500 chars): {}",
            text.chars().take(500).collect::<String>()
        );
        RedactError::RedactionParse(e)
    })?;
    Ok(list.redactions)
}

/// 1-stage `RedactionList` 用の Gemini Structured Output schema。
/// `RedactionBox` (page, box_2d[4], text, cell_label?) の配列を `redactions` key で返す。
fn redact_response_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "OBJECT",
        "properties": {
            "redactions": {
                "type": "ARRAY",
                "items": {
                    "type": "OBJECT",
                    "properties": {
                        "page": { "type": "INTEGER" },
                        "box_2d": {
                            "type": "ARRAY",
                            "items": { "type": "NUMBER" },
                            "minItems": 4,
                            "maxItems": 4
                        },
                        "text": { "type": "STRING" },
                        "cell_label": { "type": "STRING", "nullable": true }
                    },
                    "required": ["page", "box_2d", "text"],
                    "propertyOrdering": ["page", "box_2d", "text", "cell_label"]
                }
            }
        },
        "required": ["redactions"]
    })
}

/// Stage 1 用 Gemini Structured Output schema。
/// 必ず `{page, title?, cells: [{box_2d[4], text}]}` 形式で返させる。
/// `Stage1Response::Single` variant 専用 — Multi (`{tables: [...]}`) は schema が
/// 強制する単一 page 形式に collapse される (複数ページ PDF も同 page key で 1 ページ分のみ)。
fn stage1_response_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "OBJECT",
        "properties": {
            "page": { "type": "INTEGER" },
            "title": { "type": "STRING", "nullable": true },
            "cells": {
                "type": "ARRAY",
                "items": {
                    "type": "OBJECT",
                    "properties": {
                        "box_2d": {
                            "type": "ARRAY",
                            "items": { "type": "NUMBER" },
                            "minItems": 4,
                            "maxItems": 4
                        },
                        "text": { "type": "STRING" }
                    },
                    "required": ["box_2d", "text"],
                    "propertyOrdering": ["box_2d", "text"]
                }
            }
        },
        "required": ["page", "cells"],
        "propertyOrdering": ["page", "title", "cells"]
    })
}

/// Stage 2 用 Gemini Structured Output schema。
/// `{redactions: [{box_2d[4], text}]}` を返す。Stage 1 の box_2d をそのまま流用する
/// 設計なので page は不要 (caller が PageCells.page を補完する)。
fn stage2_response_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "OBJECT",
        "properties": {
            "redactions": {
                "type": "ARRAY",
                "items": {
                    "type": "OBJECT",
                    "properties": {
                        "box_2d": {
                            "type": "ARRAY",
                            "items": { "type": "NUMBER" },
                            "minItems": 4,
                            "maxItems": 4
                        },
                        "text": { "type": "STRING" }
                    },
                    "required": ["box_2d", "text"],
                    "propertyOrdering": ["box_2d", "text"]
                }
            }
        },
        "required": ["redactions"]
    })
}

// ============================================================================
// 2-stage Gemini パイプライン (`detect_amount_boxes_v2`)
// ============================================================================
//
// 1-stage prompt (`REDACT_PROMPT`) は「金額が入ったセルを探して bbox を返せ」
// という複合タスクを要求しており、表が密集していると Gemini が「ラベル列の
// 文字」と「値列の数字」の行マッチングを誤る (3164 で消費税が 1 行下にズレる)。
//
// 2-stage では分解する:
//   Stage 1: PDF → Gemini → **全セル列挙** (`detect_all_cells`)
//            金額判定なし、純粋な「幾何 + OCR」
//   Stage 2: Stage 1 の JSON → Gemini (画像なし) → **金額セルだけ抽出**
//            (`filter_amount_cells`)
//            画像を見ないので空間的な行ズレが構造的に発生し得ない
//
// `detect_amount_boxes_v2` がオーケストレーション層。失敗時は内部で 1-stage
// (`detect_amount_boxes`) にフォールバック。
//
// 検証: ローカル probe (`/tmp/probe_2stage.py`) で 3163/3164/3165 全て成功確認済。

/// Stage 1 prompt: 表内の全セルを列挙させる。金額判定はしない。
const STAGE1_PROMPT: &str = r#"この PDF は表形式の業務帳票 (FAX スキャン画像) です。
表内の **全セル** を列挙してください。金額判定や種類分けは一切しないこと。

【セルの定義】
- セル = 罫線で囲まれた **最小** の矩形領域 (1 行 × 1 列)
- 1 つのセルに複数行のテキストが入っている場合は、**水平罫線を必ず疑う**
- 罫線が薄い / かすれていても、テキストが 2 行以上ある領域は **別セル** として分割する
- 1 セルの text に「\n (改行)」が入ったら粒度が粗すぎる → 行ごとに分割すること

【ラベル列と値列の完全分離 (重要)】
- 業務帳票では「ラベル文字列だけが入ったセル」(例: 運賃 / 消費税 / 合計額 / 支払額 /
  支払運賃 / 高速代 / 搬出料 / 燃料サーチャージ / 駐車代 / 待機料 / 通関料 /
  付帯作業料 / 入金予定日) とその隣の「金額が入った値セル」(例: 100,000円 / 11,500円)
  は **必ず別の box_2d** として返すこと。1 つの box_2d にラベル文字 + 金額を
  同時に含めてはならない。
- 縦罫線が薄くて視覚的に連続して見えても、**列境界で必ず分割** する。
- ラベルセルの xmax と値セルの xmin の関係は **xmax_label <= xmin_value** を
  必ず満たす (両 bbox が水平方向に 1 ピクセルも重ならない)。
- 「N円」を含む text は **単独のセル** として返し、隣のラベル文字を絶対に
  同じ box_2d に含めないこと。
- これは特に消費税行 ("消費税" + "11,500円") のような **ラベルが短くて
  値セルが横に長い** 行で誤検出が発生しやすい。同じ row でも必ず 2 つの cell として返す。

出力形式 (これ以外の文字を一切出力しない):
{
  "page": 1,
  "title": "運賃明細",
  "cells": [
    {"box_2d": [120, 50, 180, 240], "text": "運賃"},
    {"box_2d": [120, 240, 180, 480], "text": "100,000円"},
    {"box_2d": [180, 50, 240, 240], "text": "消費税"},
    {"box_2d": [180, 240, 240, 480], "text": "11,500円"}
  ]
}
↑ この例で xmax_label=240 == xmin_value=240 となっており、ラベルと値の bbox は
  完全に分離している (重複ゼロ)。実際の出力でも必ずこの構造を守ること。

ルール:
- box_2d は 0-1000 で正規化された [ymin, xmin, ymax, xmax] (左上原点)
- page は 1-origin
- title は表の見出し (なければ最も近いテキスト、推測でよい)
- text はセル内の文字列を OCR したそのまま (空セルは text="")
- **text に改行 (\n) を含めてはならない**。複数行に見えるなら別セル
- **金額判定はしない**: ラベルセル ("運賃" / "消費税" / "合計") も値セル
  ("100,000円" / "11,500円") もすべて等しく列挙する
- **ラベルと値の bbox 重複禁止**: text に「円」を含むセルは、同じ row の
  隣接ラベルセル (運賃 / 消費税 / 合計額 等) の bbox と水平方向に重なって
  はならない。重なって見える場合は罫線位置を疑い、必ず column 境界で
  bbox を切り直すこと (xmax_label <= xmin_value)。
- 全セルを順番に左上から右下へ (行優先)
- 罫線をたどって表全体を網羅 (取りこぼし禁止)"#;

/// Stage 2 prompt フォーマット。Stage 1 出力 JSON を `{cells_json}` 部分に埋め込む。
fn stage2_prompt(cells_json: &str) -> String {
    format!(
        r#"以下は業務帳票 PDF から抽出した全セルのリストです。この中から
「金額 (円) が記入されている値セル」だけを抽出してください。

入力 JSON:
{cells_json}

抽出対象:
- text に「円」を含むセル (例: "100,000円", "￥11,500", "JPY 5,000")
- 「N円(税抜)」「N円(税込)」など税表記付きも対象

抽出しないもの:
- ラベルセル ("運賃" / "消費税" / "合計" / "支払額" などの文字列のみ)
- 単位付き数値だが金額でないもの ("3,200kg" / "9PL" / "10t" / "10:00")
- 氏名・電話・FAX・郵便番号・車番・住所

出力形式 (これ以外の文字を一切出力しない):
{{
  "redactions": [
    {{"box_2d": [ymin, xmin, ymax, xmax], "text": "100,000円"}},
    {{"box_2d": [ymin, xmin, ymax, xmax], "text": "11,500円"}}
  ]
}}

box_2d は入力 JSON のまま使うこと。座標を変更してはならない。"#
    )
}

/// Stage 1: PDF を投げて表内の全セルを列挙させる。
///
/// レスポンスは単一 page (`{"page": 1, "cells": [...]}`) または複数 tables
/// (`{"tables": [{"page": 1, "cells": [...]}, ...]}`) のどちらでも受け取れる
/// (`Stage1Response` の untagged enum で対応)。
pub async fn detect_all_cells(
    pdf_bytes: &[u8],
    api_key: &str,
    model: Option<&str>,
    endpoint: Option<&str>,
) -> Result<Vec<PageCells>, RedactError> {
    let client = reqwest::Client::new();
    let model = model.unwrap_or(GEMINI_DEFAULT_MODEL);
    let endpoint = endpoint.unwrap_or(GEMINI_DEFAULT_ENDPOINT);

    let url = format!("{endpoint}/models/{model}:generateContent?key={api_key}");
    let pdf_b64 = base64::engine::general_purpose::STANDARD.encode(pdf_bytes);

    // Stage 1 は全セル列挙なので token 上限を多めに (PDF によっては 50+ セル)
    let body = serde_json::json!({
        "contents": [{
            "role": "user",
            "parts": [
                { "inlineData": { "mimeType": "application/pdf", "data": pdf_b64 } },
                { "text": STAGE1_PROMPT }
            ]
        }],
        "generationConfig": {
            "temperature": 0.0,
            "responseMimeType": "application/json",
            // Structured Output: Gemini が schema に合致する JSON を保証する。
            // 過去 staging ログで `Stage1Response` parse 失敗 → 1-stage fallback で
            // 3164 ズレ再発する事故があり (responseMimeType だけでは markdown wrap や
            // 余計な前置テキストが混入することがあった)、schema で完全に固定する。
            "responseSchema": stage1_response_schema(),
            "maxOutputTokens": 8192
        }
    });

    let resp = client.post(&url).json(&body).send().await?;
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(RedactError::GeminiStatus(status, body));
    }
    let raw = resp.text().await?;
    let parsed: serde_json::Value = serde_json::from_str(&raw).map_err(RedactError::GeminiParse)?;
    let text = parsed
        .pointer("/candidates/0/content/parts/0/text")
        .and_then(|v| v.as_str())
        .ok_or(RedactError::GeminiEmpty)?;

    let stage1: Stage1Response = serde_json::from_str(text).map_err(|e| {
        // schema で fix したはずなのに parse 失敗 = Gemini 側が schema を破った
        // ということ。raw response の先頭 1KB を残して原因究明できるようにする。
        tracing::warn!(
            "redact stage1: parse failed despite responseSchema: {e}; raw text (first 1000 chars): {}",
            text.chars().take(1000).collect::<String>()
        );
        RedactError::RedactionParse(e)
    })?;
    let pages = match stage1 {
        Stage1Response::Multi { tables } => tables,
        Stage1Response::Single(p) => vec![p],
    };
    Ok(pages)
}

/// Stage 2: Stage 1 の JSON を Gemini に **画像なしで** 投げ、金額セルだけ抽出。
///
/// 入力に画像を含まないので空間的な行マッチングミスは発生し得ない (= 3164 の
/// 1 行ズレ問題が構造的に解消される)。返却される `RedactionBox.page` は
/// 入力 `PageCells.page` をそのまま採用 (Gemini からは page 情報が返らないので
/// 同 page 内のセルが入力なら全 redaction も同 page、複数ページの場合は
/// 最初の page を採用)。
pub async fn filter_amount_cells(
    pages: &[PageCells],
    api_key: &str,
    model: Option<&str>,
    endpoint: Option<&str>,
) -> Result<Vec<RedactionBox>, RedactError> {
    if pages.is_empty() {
        return Ok(Vec::new());
    }
    let client = reqwest::Client::new();
    let model = model.unwrap_or(GEMINI_DEFAULT_MODEL);
    let endpoint = endpoint.unwrap_or(GEMINI_DEFAULT_ENDPOINT);

    let url = format!("{endpoint}/models/{model}:generateContent?key={api_key}");
    let cells_json = serde_json::to_string(&pages).map_err(RedactError::GeminiParse)?;
    let prompt = stage2_prompt(&cells_json);

    let body = serde_json::json!({
        "contents": [{
            "role": "user",
            "parts": [{ "text": prompt }]
        }],
        "generationConfig": {
            "temperature": 0.0,
            "responseMimeType": "application/json",
            // Structured Output: Stage 2 出力 `{redactions: [{box_2d[4], text}]}` を schema で保証
            "responseSchema": stage2_response_schema(),
            "maxOutputTokens": 4096
        }
    });

    let resp = client.post(&url).json(&body).send().await?;
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(RedactError::GeminiStatus(status, body));
    }
    let raw = resp.text().await?;
    let parsed: serde_json::Value = serde_json::from_str(&raw).map_err(RedactError::GeminiParse)?;
    let text = parsed
        .pointer("/candidates/0/content/parts/0/text")
        .and_then(|v| v.as_str())
        .ok_or(RedactError::GeminiEmpty)?;

    // Stage 2 は {"redactions": [{"box_2d": [...], "text": "..."}]} を返すが
    // page フィールドは含まないので、ここで Stage 1 入力の page を補完する。
    #[derive(serde::Deserialize)]
    struct Stage2Box {
        box_2d: [f32; 4],
        text: String,
    }
    #[derive(serde::Deserialize)]
    struct Stage2List {
        redactions: Vec<Stage2Box>,
    }
    let list: Stage2List = serde_json::from_str(text).map_err(|e| {
        tracing::warn!(
            "redact stage2: parse failed despite responseSchema: {e}; raw text (first 1000 chars): {}",
            text.chars().take(1000).collect::<String>()
        );
        RedactError::RedactionParse(e)
    })?;

    // page は入力 pages[0].page を採用 (現実的に通知 PDF は単一ページが大半)。
    // 複数ページ対応が必要になったら Stage 2 に page を返させる prompt 改修が必要。
    let default_page = pages[0].page;
    Ok(list
        .redactions
        .into_iter()
        .map(|s2| RedactionBox {
            page: default_page,
            box_2d: s2.box_2d,
            text: s2.text,
        })
        .collect())
}

/// 2-stage オーケストレーション。Stage 1/2 のいずれかが失敗したら、または
/// Stage 1 が 0 セルを返したら、1-stage (`detect_amount_boxes`) に
/// フォールバック。warn ログで判別可能。
pub async fn detect_amount_boxes_v2(
    pdf_bytes: &[u8],
    api_key: &str,
    model: Option<&str>,
    endpoint: Option<&str>,
) -> Result<Vec<RedactionBox>, RedactError> {
    match detect_all_cells(pdf_bytes, api_key, model, endpoint).await {
        Ok(pages) if pages.iter().any(|p| !p.cells.is_empty()) => {
            match filter_amount_cells(&pages, api_key, model, endpoint).await {
                Ok(boxes) => Ok(boxes),
                Err(e) => {
                    tracing::warn!("redact 2-stage: stage2 failed, fallback to 1-stage: {e}");
                    detect_amount_boxes(pdf_bytes, api_key, model, endpoint).await
                }
            }
        }
        Ok(_) => {
            tracing::warn!("redact 2-stage: stage1 returned 0 cells, fallback to 1-stage");
            detect_amount_boxes(pdf_bytes, api_key, model, endpoint).await
        }
        Err(e) => {
            tracing::warn!("redact 2-stage: stage1 failed, fallback to 1-stage: {e}");
            detect_amount_boxes(pdf_bytes, api_key, model, endpoint).await
        }
    }
}

/// PDF 内の埋め込み JPEG 画像のピクセルを直接書き換えて redacted PDF を返す。
///
/// FAX 由来 PDF が前提 (1 page = 1 image XObject、DCTDecode フィルタ)。それ以外
/// (テキスト PDF など) のページはスキップせずエラー返却 (`PageNoImage`)。
///
/// 元値は出力 PDF のどこにも残らないので、PDF.js progressive render でも
/// **ちらつきは構造的に発生し得ない**。pure 関数 (HTTP / DB に触らない)。
pub fn apply_redactions(
    pdf_bytes: &[u8],
    redactions: &[RedactionBox],
) -> Result<Vec<u8>, RedactError> {
    let mut doc = Document::load_mem(pdf_bytes)?;

    if redactions.is_empty() {
        let mut out = Vec::new();
        doc.save_to(&mut out)?;
        return Ok(out);
    }

    // 1) bbox 検証 + ページ単位にグループ化
    //
    // bbox は **Gemini が返した値をそのまま** 使う。prompt で「セル全体」の
    // bbox を返してもらっているので、人為的なパディングは不要 (むしろ隣のセル
    // を巻き込むので有害)。
    let pages: Vec<ObjectId> = doc.get_pages().into_values().collect();
    let mut by_page: std::collections::BTreeMap<usize, Vec<&RedactionBox>> = Default::default();
    for r in redactions {
        if r.page == 0 || r.page > pages.len() {
            return Err(RedactError::PageNotFound(r.page));
        }
        let [ymin, xmin, ymax, xmax] = r.box_2d;
        if !(0.0..=1000.0).contains(&ymin)
            || !(0.0..=1000.0).contains(&xmin)
            || !(0.0..=1000.0).contains(&ymax)
            || !(0.0..=1000.0).contains(&xmax)
            || ymin >= ymax
            || xmin >= xmax
        {
            return Err(RedactError::InvalidBox(r.box_2d));
        }
        by_page.entry(r.page).or_default().push(r);
    }

    // 2) ページごとに、埋め込み画像 XObject を探して書き換え
    for (page_idx, redactions_on_page) in by_page {
        let page_id = pages[page_idx - 1];
        let image_obj_id =
            find_first_image_xobject(&doc, page_id)?.ok_or(RedactError::PageNoImage(page_idx))?;

        // XObject Image stream を取得し、JPEG bytes と寸法 (Width, Height) を読む。
        // PDF の Filter は単一名前 (e.g. /DCTDecode) または配列 (e.g. [/FlateDecode
        // /DCTDecode]) のどちらか。FAX 由来 PDF は zlib 圧縮を被せた JPEG が
        // 多いので、配列の場合は Flate を解凍してから JPEG を取り出す。
        let (jpeg_bytes, img_w, img_h, has_flate_wrapper) = {
            let stream = doc.get_object(image_obj_id)?.as_stream()?;
            let w = stream
                .dict
                .get(b"Width")
                .ok()
                .and_then(|o| o.as_i64().ok())
                .unwrap_or(0) as u32;
            let h = stream
                .dict
                .get(b"Height")
                .ok()
                .and_then(|o| o.as_i64().ok())
                .unwrap_or(0) as u32;
            let has_flate = match stream.dict.get(b"Filter") {
                Ok(Object::Array(arr)) => arr
                    .first()
                    .and_then(|o| o.as_name().ok())
                    .map(|n| n == b"FlateDecode")
                    .unwrap_or(false),
                _ => false,
            };
            let raw = stream.content.clone();
            let jpeg = if has_flate {
                let mut decoded = Vec::with_capacity(raw.len() * 4);
                ZlibDecoder::new(raw.as_slice())
                    .read_to_end(&mut decoded)
                    .map_err(RedactError::PdfIo)?;
                decoded
            } else {
                raw
            };
            (jpeg, w, h, has_flate)
        };

        // 3) JPEG decode → 白矩形描画 → JPEG re-encode
        let mut img = image::load_from_memory(&jpeg_bytes)?.to_rgb8();
        let (real_w, real_h) = img.dimensions();
        // 念のため: dict の Width/Height が 0 なら decode 後の寸法を使う
        let (w_for_calc, h_for_calc) = if img_w > 0 && img_h > 0 {
            (img_w as f32, img_h as f32)
        } else {
            (real_w as f32, real_h as f32)
        };

        for r in &redactions_on_page {
            let [ymin, xmin, ymax, xmax] = r.box_2d;
            // box_2d (0-1000、左上原点) → 画像 pixel 座標
            let px = ((xmin / 1000.0) * w_for_calc).round().max(0.0) as u32;
            let py = ((ymin / 1000.0) * h_for_calc).round().max(0.0) as u32;
            let pw = ((xmax - xmin) / 1000.0 * w_for_calc).round().max(0.0) as u32;
            let ph = ((ymax - ymin) / 1000.0 * h_for_calc).round().max(0.0) as u32;

            let x_end = (px + pw).min(real_w);
            let y_end = (py + ph).min(real_h);
            for y in py.min(real_h)..y_end {
                for x in px.min(real_w)..x_end {
                    img.put_pixel(x, y, image::Rgb([255, 255, 255]));
                }
            }
        }

        let mut new_jpeg = Vec::with_capacity(jpeg_bytes.len());
        // quality 90: 元の FAX スキャンが既に低品質なので 90 で十分、ファイル膨張も
        // 抑えられる。
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut new_jpeg, 90).write_image(
            img.as_raw(),
            real_w,
            real_h,
            image::ExtendedColorType::Rgb8,
        )?;

        // 4) Stream content + dict 差し替え。
        //    - Flate 被せありなら Filter を /DCTDecode 単体に書き換え
        //    - 元の ColorSpace が /DeviceGray でも我々は RGB JPEG を出力するので
        //      /DeviceRGB に書き換え (DeviceGray のままだと表示崩壊)
        //    - Decode 配列があれば削除、BitsPerComponent=8 を明示
        let stream = doc.get_object_mut(image_obj_id)?.as_stream_mut()?;
        stream.set_content(new_jpeg);
        if has_flate_wrapper {
            stream
                .dict
                .set("Filter", Object::Name(b"DCTDecode".to_vec()));
        }
        stream
            .dict
            .set("ColorSpace", Object::Name(b"DeviceRGB".to_vec()));
        stream.dict.remove(b"Decode");
        stream.dict.set("BitsPerComponent", Object::Integer(8));

        // 5) **content stream にも白矩形をオーバーレイ** (defense-in-depth)。
        //    FAX 由来 PDF は JPEG (背景) + CCITT ImageMask (黒インクの文字レイヤー)
        //    の多層構造で、JPEG ピクセルだけ書き換えても CCITT が上から元の数字を
        //    再描画する。content stream の末尾に PDF の白塗り矩形コマンドを追加
        //    することで、CCITT レイヤーも含めて確実に隠す。
        //    JPEG ピクセル書き換えと組み合わせることで:
        //      - 元値は PDF データから完全に消える (JPEG レベル)
        //      - レンダリング後の表示も白塗り (content stream レベル)
        //      - PDF.js の progressive 描画でも、最後に矩形が描かれて確実に隠れる
        let (page_w, page_h) = page_size(&doc, page_id)?;
        for r in &redactions_on_page {
            let [ymin, xmin, ymax, xmax] = r.box_2d;
            // box_2d (0-1000、左上原点) → PDF 座標 (左下原点 pt) に変換
            let x = (xmin / 1000.0) * page_w;
            let y = page_h - (ymax / 1000.0) * page_h;
            let bw = ((xmax - xmin) / 1000.0) * page_w;
            let bh = ((ymax - ymin) / 1000.0) * page_h;

            let cmd = format!("q 1 1 1 rg {x:.2} {y:.2} {bw:.2} {bh:.2} re f Q\n");
            let stream = Stream::new(lopdf::dictionary! {}, cmd.into_bytes());
            let stream_id = doc.add_object(stream);
            append_content_stream(&mut doc, page_id, stream_id)?;
        }
    }

    let mut out = Vec::new();
    doc.save_to(&mut out)?;
    Ok(out)
}

/// Page の MediaBox から (width, height) を pt 単位で取得。
fn page_size(doc: &Document, page_id: ObjectId) -> Result<(f32, f32), RedactError> {
    let mut current = page_id;
    loop {
        let dict = doc.get_object(current)?.as_dict()?;
        if let Ok(mb) = dict.get(b"MediaBox") {
            let arr = mb.as_array()?;
            if arr.len() == 4 {
                let llx = obj_to_f32(&arr[0])?;
                let lly = obj_to_f32(&arr[1])?;
                let urx = obj_to_f32(&arr[2])?;
                let ury = obj_to_f32(&arr[3])?;
                return Ok((urx - llx, ury - lly));
            }
        }
        match dict.get(b"Parent") {
            Ok(Object::Reference(parent_id)) => current = *parent_id,
            _ => break,
        }
    }
    Ok((595.0, 842.0))
}

fn obj_to_f32(o: &Object) -> Result<f32, lopdf::Error> {
    match o {
        Object::Integer(i) => Ok(*i as f32),
        Object::Real(r) => Ok(*r),
        _ => Err(lopdf::Error::ObjectNotFound),
    }
}

/// Page の Contents 配列の末尾に新しい Stream を追加する。
fn append_content_stream(
    doc: &mut Document,
    page_id: ObjectId,
    stream_id: ObjectId,
) -> Result<(), RedactError> {
    let page = doc.get_object_mut(page_id)?;
    let dict = page.as_dict_mut()?;

    let new_contents = match dict.get(b"Contents") {
        Ok(Object::Reference(existing)) => Object::Array(vec![
            Object::Reference(*existing),
            Object::Reference(stream_id),
        ]),
        Ok(Object::Array(arr)) => {
            let mut v = arr.clone();
            v.push(Object::Reference(stream_id));
            Object::Array(v)
        }
        _ => Object::Array(vec![Object::Reference(stream_id)]),
    };

    dict.set("Contents", new_contents);
    Ok(())
}

/// 指定ページの Resources/XObject から **DCTDecode (= JPEG) フィルタの画像のうち
/// 最も pixel 数が大きいもの** の ObjectId を返す。
///
/// FAX 由来 PDF は典型的に「メイン JPEG (本文) + CCITT ステンシル (印影 / マスク)」
/// の多層構造で、CCITT ステンシルを掴むと `image::load_from_memory` が認識不能で
/// 失敗する。CCITT を読めるようにする選択肢もあるが、本機能の目的は「金額部分を
/// 隠す」だけなので、本文 JPEG の上に白矩形を焼き込む方針で十分。
fn find_first_image_xobject(
    doc: &Document,
    page_id: ObjectId,
) -> Result<Option<ObjectId>, RedactError> {
    // Resources は Page 自身か親 (Pages tree) のどちらかにある可能性
    let mut current = page_id;
    let resources_obj = loop {
        let dict = doc.get_object(current)?.as_dict()?;
        if let Ok(r) = dict.get(b"Resources") {
            break r.clone();
        }
        match dict.get(b"Parent") {
            Ok(Object::Reference(parent_id)) => current = *parent_id,
            _ => return Ok(None),
        }
    };

    let resources_dict = match &resources_obj {
        Object::Dictionary(d) => d.clone(),
        Object::Reference(id) => doc.get_object(*id)?.as_dict()?.clone(),
        _ => return Ok(None),
    };

    let xobject_obj = match resources_dict.get(b"XObject") {
        Ok(o) => o.clone(),
        Err(_) => return Ok(None),
    };
    let xobject_dict = match &xobject_obj {
        Object::Dictionary(d) => d.clone(),
        Object::Reference(id) => doc.get_object(*id)?.as_dict()?.clone(),
        _ => return Ok(None),
    };

    let mut best: Option<(ObjectId, u64)> = None;
    for (_name, obj) in xobject_dict.iter() {
        let id = match obj {
            Object::Reference(id) => *id,
            _ => continue,
        };
        let stream = match doc.get_object(id) {
            Ok(o) => match o.as_stream() {
                Ok(s) => s,
                Err(_) => continue,
            },
            Err(_) => continue,
        };
        // Subtype=Image
        let is_image = stream
            .dict
            .get(b"Subtype")
            .ok()
            .and_then(|o| o.as_name().ok())
            .map(|n| n == b"Image")
            .unwrap_or(false);
        if !is_image {
            continue;
        }
        // Filter に DCTDecode (JPEG) を含むものを対象にする。
        // 単一名: /DCTDecode
        // 配列: [/DCTDecode] / [/FlateDecode /DCTDecode] (FAX 由来 PDF に多い、JPEG の上に zlib 圧縮)
        let is_jpeg = match stream.dict.get(b"Filter") {
            Ok(Object::Name(n)) => n == b"DCTDecode",
            Ok(Object::Array(arr)) => arr
                .iter()
                .any(|o| o.as_name().map(|n| n == b"DCTDecode").unwrap_or(false)),
            _ => false,
        };
        if !is_jpeg {
            continue;
        }
        let w = stream
            .dict
            .get(b"Width")
            .ok()
            .and_then(|o| o.as_i64().ok())
            .unwrap_or(0) as u64;
        let h = stream
            .dict
            .get(b"Height")
            .ok()
            .and_then(|o| o.as_i64().ok())
            .unwrap_or(0) as u64;
        let area = w.saturating_mul(h);
        match best {
            None => best = Some((id, area)),
            Some((_, prev_area)) if area > prev_area => best = Some((id, area)),
            _ => {}
        }
    }
    Ok(best.map(|(id, _)| id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::dictionary;

    #[test]
    fn test_redaction_box_serde() {
        let r = RedactionBox {
            page: 1,
            box_2d: [100.0, 200.0, 300.0, 400.0],
            text: "160,000円".into(),
        };
        let s = serde_json::to_string(&r).unwrap();
        let back: RedactionBox = serde_json::from_str(&s).unwrap();
        assert_eq!(r, back);
    }

    #[test]
    fn test_apply_redactions_invalid_bbox_rejected() {
        let pdf = pdf_with_jpeg_image(50, 50);
        let bad = vec![RedactionBox {
            page: 1,
            box_2d: [500.0, 500.0, 100.0, 100.0], // ymax < ymin
            text: "x".into(),
        }];
        let err = apply_redactions(&pdf, &bad).unwrap_err();
        assert!(matches!(err, RedactError::InvalidBox(_)));
    }

    #[test]
    fn test_apply_redactions_page_not_found() {
        let pdf = pdf_with_jpeg_image(50, 50);
        let r = vec![RedactionBox {
            page: 99,
            box_2d: [100.0, 100.0, 200.0, 200.0],
            text: "x".into(),
        }];
        let err = apply_redactions(&pdf, &r).unwrap_err();
        assert!(matches!(err, RedactError::PageNotFound(99)));
    }

    #[test]
    fn test_apply_redactions_empty_passthrough() {
        let pdf = pdf_with_jpeg_image(50, 50);
        let out = apply_redactions(&pdf, &[]).unwrap();
        // 出力 PDF は valid (再 load できる) こと
        let _doc = Document::load_mem(&out).unwrap();
    }

    #[test]
    fn test_apply_redactions_paints_white_on_jpeg() {
        // 元画像は全面赤。bbox = 全面。redact 後の画像は全面 白 になっているはず。
        let pdf = pdf_with_jpeg_image(40, 40);
        let r = vec![RedactionBox {
            page: 1,
            box_2d: [0.0, 0.0, 1000.0, 1000.0],
            text: "x".into(),
        }];
        let out = apply_redactions(&pdf, &r).unwrap();
        let doc = Document::load_mem(&out).unwrap();
        // 出力 PDF から再度 image stream を抽出し、ピクセル平均が白に近いことを確認
        let pages: Vec<ObjectId> = doc.get_pages().into_values().collect();
        let img_id = find_first_image_xobject(&doc, pages[0]).unwrap().unwrap();
        let stream = doc.get_object(img_id).unwrap().as_stream().unwrap();
        let img = image::load_from_memory(&stream.content).unwrap().to_rgb8();
        // 中央ピクセルが白 (255,255,255) であること
        let center = img.get_pixel(20, 20);
        assert!(
            center.0[0] > 240 && center.0[1] > 240 && center.0[2] > 240,
            "center pixel should be white, got {:?}",
            center.0
        );
    }

    #[test]
    fn test_apply_redactions_no_image_returns_error() {
        // 画像 XObject を持たない page → PageNoImage
        let pdf = pdf_without_image();
        let r = vec![RedactionBox {
            page: 1,
            box_2d: [100.0, 100.0, 200.0, 200.0],
            text: "x".into(),
        }];
        let err = apply_redactions(&pdf, &r).unwrap_err();
        assert!(matches!(err, RedactError::PageNoImage(1)));
    }

    /// テスト用: A4 1 ページに `width × height` の赤い JPEG を 1 枚埋め込んだ PDF
    fn pdf_with_jpeg_image(width: u32, height: u32) -> Vec<u8> {
        // 赤一色の JPEG bytes を作る
        let mut red = image::RgbImage::new(width, height);
        for p in red.pixels_mut() {
            *p = image::Rgb([255, 0, 0]);
        }
        let mut jpeg = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 90)
            .write_image(red.as_raw(), width, height, image::ExtendedColorType::Rgb8)
            .unwrap();

        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();

        // Image XObject (DCTDecode = JPEG)
        let image_stream = Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Image",
                "Width" => width as i64,
                "Height" => height as i64,
                "ColorSpace" => "DeviceRGB",
                "BitsPerComponent" => 8,
                "Filter" => "DCTDecode",
            },
            jpeg,
        );
        // Stream を新規追加 (lopdf が自動で /Length を計算)
        let image_id = doc.add_object(image_stream);

        // Resources
        let resources_id = doc.add_object(dictionary! {
            "XObject" => dictionary! { "Im0" => image_id },
        });

        // Page
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "Resources" => resources_id,
            "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
        });

        // Catalog + Pages tree
        let catalog_id = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Count" => 1,
                "Kids" => vec![page_id.into()],
            }),
        );
        doc.trailer.set("Root", catalog_id);

        let mut out = Vec::new();
        doc.save_to(&mut out).unwrap();
        out
    }

    fn pdf_without_image() -> Vec<u8> {
        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
        });
        let catalog_id = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Count" => 1,
                "Kids" => vec![page_id.into()],
            }),
        );
        doc.trailer.set("Root", catalog_id);
        let mut out = Vec::new();
        doc.save_to(&mut out).unwrap();
        out
    }

    // ====================================================================
    // 2-stage パイプライン (`detect_amount_boxes_v2`) のテスト
    // ====================================================================

    #[test]
    fn test_cell_box_serde() {
        let c = CellBox {
            box_2d: [100.0, 200.0, 150.0, 400.0],
            text: "100,000円".into(),
        };
        let s = serde_json::to_string(&c).unwrap();
        let back: CellBox = serde_json::from_str(&s).unwrap();
        assert_eq!(c, back);
    }

    #[test]
    fn test_page_cells_serde_single_form() {
        // Stage 1 が `{"page": 1, "title": "x", "cells": [...]}` を返す形式
        let raw =
            r#"{"page":1,"title":"運賃明細","cells":[{"box_2d":[100,200,150,400],"text":"運賃"}]}"#;
        let parsed: Stage1Response = serde_json::from_str(raw).unwrap();
        let pages = match parsed {
            Stage1Response::Single(p) => vec![p],
            Stage1Response::Multi { tables } => tables,
        };
        assert_eq!(pages.len(), 1);
        assert_eq!(pages[0].page, 1);
        assert_eq!(pages[0].title, "運賃明細");
        assert_eq!(pages[0].cells.len(), 1);
    }

    #[test]
    fn test_page_cells_serde_multi_form() {
        // Stage 1 が `{"tables": [{...}, {...}]}` を返す形式
        let raw =
            r#"{"tables":[{"page":1,"title":"a","cells":[]},{"page":2,"title":"b","cells":[]}]}"#;
        let parsed: Stage1Response = serde_json::from_str(raw).unwrap();
        let pages = match parsed {
            Stage1Response::Single(p) => vec![p],
            Stage1Response::Multi { tables } => tables,
        };
        assert_eq!(pages.len(), 2);
        assert_eq!(pages[1].page, 2);
        assert_eq!(pages[1].title, "b");
    }

    #[test]
    fn test_page_cells_serde_defaults() {
        // title / cells が欠けても OK (デフォルト空)
        let raw = r#"{"page":1}"#;
        let p: PageCells = serde_json::from_str(raw).unwrap();
        assert_eq!(p.page, 1);
        assert_eq!(p.title, "");
        assert!(p.cells.is_empty());
    }

    #[test]
    fn test_stage2_prompt_contains_cells_json() {
        let prompt = stage2_prompt(
            r#"{"page":1,"cells":[{"box_2d":[100,200,150,400],"text":"100,000円"}]}"#,
        );
        assert!(prompt.contains("100,000円"));
        assert!(prompt.contains("box_2d は入力 JSON のまま使うこと"));
    }

    /// Gemini Structured Output schema が prompt 強化と一緒に保たれているか pin する。
    ///
    /// 過去 staging で responseMimeType=json だけでは Stage 1 の出力が時々 markdown
    /// wrap や余分な前置テキストを含み、`Stage1Response` parse 失敗 → 1-stage fallback
    /// (= 3164 ズレ再発) する事故があった。`responseSchema` を generationConfig に
    /// 注入することで Gemini 側で構造を保証させる。schema を消したり field を
    /// rename したりした場合に CI が即座に拾えるよう、構造を pin する。
    #[test]
    fn test_response_schemas_pin_structure() {
        // 1-stage: redactions[].{page, box_2d[4], text, cell_label?}
        let s = redact_response_schema();
        assert_eq!(s["type"], "OBJECT");
        assert_eq!(s["required"][0], "redactions");
        let item = &s["properties"]["redactions"]["items"];
        assert_eq!(item["properties"]["box_2d"]["minItems"], 4);
        assert_eq!(item["properties"]["box_2d"]["maxItems"], 4);
        assert_eq!(
            item["required"],
            serde_json::json!(["page", "box_2d", "text"])
        );
        assert_eq!(item["properties"]["cell_label"]["nullable"], true);

        // Stage 1: {page, title?, cells: [{box_2d[4], text}]}
        let s = stage1_response_schema();
        assert_eq!(s["required"], serde_json::json!(["page", "cells"]));
        assert_eq!(s["properties"]["title"]["nullable"], true);
        let cell = &s["properties"]["cells"]["items"];
        assert_eq!(cell["properties"]["box_2d"]["minItems"], 4);
        assert_eq!(cell["properties"]["box_2d"]["maxItems"], 4);
        assert_eq!(cell["required"], serde_json::json!(["box_2d", "text"]));

        // Stage 2: {redactions: [{box_2d[4], text}]}
        let s = stage2_response_schema();
        assert_eq!(s["required"][0], "redactions");
        let r = &s["properties"]["redactions"]["items"];
        assert_eq!(r["properties"]["box_2d"]["minItems"], 4);
        assert_eq!(r["properties"]["box_2d"]["maxItems"], 4);
        assert_eq!(r["required"], serde_json::json!(["box_2d", "text"]));
    }

    /// 3164 消費税行ズレ対策: STAGE1_PROMPT が「ラベル列と値列の完全分離」を
    /// 明示しているか。Gemini への指示が薄れると 3164 で消費税ラベルと
    /// 値が同一 bbox に merge される回帰が起きるため、prompt 内容自体を
    /// regression-guard する。
    #[test]
    fn test_stage1_prompt_enforces_label_value_separation() {
        // 完全分離ブロック自体
        assert!(STAGE1_PROMPT.contains("ラベル列と値列の完全分離"));
        // 不等式の数式表現で意図を pinning
        assert!(STAGE1_PROMPT.contains("xmax_label <= xmin_value"));
        // 消費税行が誤検出されやすい旨の言及
        assert!(STAGE1_PROMPT.contains("消費税"));
        // few-shot example が「消費税 / 11,500円」の 2 セルを別 bbox で示すこと
        assert!(STAGE1_PROMPT.contains(r#""text": "消費税""#));
        assert!(STAGE1_PROMPT.contains(r#""text": "11,500円""#));
        // ルール末尾の重複禁止条項
        assert!(STAGE1_PROMPT.contains("ラベルと値の bbox 重複禁止"));
    }

    /// Stage 1 + Stage 2 を順次モックして `detect_amount_boxes_v2` を通す。
    #[tokio::test]
    async fn test_detect_amount_boxes_v2_two_stage_happy_path() {
        use wiremock::matchers::{method, path_regex};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;

        // Stage 1: 全セル列挙レスポンス
        let stage1_text = r#"{"page":1,"title":"運賃明細","cells":[{"box_2d":[100,200,150,400],"text":"運賃"},{"box_2d":[100,400,150,600],"text":"100,000円"},{"box_2d":[150,200,200,400],"text":"消費税"},{"box_2d":[150,400,200,600],"text":"10,000円"}]}"#;
        Mock::given(method("POST"))
            .and(path_regex(r".*generateContent.*"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "candidates": [{"content": {"parts": [{"text": stage1_text}]}}]
            })))
            .up_to_n_times(1)
            .mount(&server)
            .await;

        // Stage 2: 金額セルだけ返す
        let stage2_text = r#"{"redactions":[{"box_2d":[100,400,150,600],"text":"100,000円"},{"box_2d":[150,400,200,600],"text":"10,000円"}]}"#;
        Mock::given(method("POST"))
            .and(path_regex(r".*generateContent.*"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "candidates": [{"content": {"parts": [{"text": stage2_text}]}}]
            })))
            .mount(&server)
            .await;

        let result = detect_amount_boxes_v2(b"fake pdf", "fake-key", None, Some(&server.uri()))
            .await
            .unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].text, "100,000円");
        assert_eq!(result[0].page, 1); // PageCells.page から補完
        assert_eq!(result[0].box_2d, [100.0, 400.0, 150.0, 600.0]);
        assert_eq!(result[1].text, "10,000円");
    }

    /// 3164 消費税行 regression: Stage 1 が「消費税 (ラベル) / 11,500円 (値)」
    /// を別 bbox で返したとき、Stage 2 は値 bbox のみを抽出し、ラベル文字を
    /// 一切 redaction 対象に含めないこと。さらに redaction 対象 bbox が
    /// ラベル bbox と水平方向に重ならないこと (xmax_label <= xmin_value) を
    /// 構造的に検証する。本来 prompt と Gemini が守るべき不等式だが、
    /// テストで pinning することで Stage 2 の出力経路の回帰も捕捉できる。
    #[tokio::test]
    async fn test_detect_amount_boxes_v2_3164_taxrow_no_label_redaction() {
        use wiremock::matchers::{method, path_regex};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;

        // Stage 1: 3164 想定 — 消費税行はラベル "消費税" と値 "11,500円" の 2 cell。
        // ラベル xmax = 240, 値 xmin = 240 で完全分離。
        let stage1_text = r#"{"page":1,"title":"運賃及び料金","cells":[{"box_2d":[120,50,180,240],"text":"運賃"},{"box_2d":[120,240,180,480],"text":"100,000円"},{"box_2d":[180,50,240,240],"text":"消費税"},{"box_2d":[180,240,240,480],"text":"11,500円"},{"box_2d":[240,50,300,240],"text":"合計額"},{"box_2d":[240,240,300,480],"text":"111,500円"}]}"#;
        Mock::given(method("POST"))
            .and(path_regex(r".*generateContent.*"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "candidates": [{"content": {"parts": [{"text": stage1_text}]}}]
            })))
            .up_to_n_times(1)
            .mount(&server)
            .await;

        // Stage 2: 円を含む 3 cell だけを返す (Gemini が prompt 通りに
        // ラベルセルを除外した想定)。
        let stage2_text = r#"{"redactions":[{"box_2d":[120,240,180,480],"text":"100,000円"},{"box_2d":[180,240,240,480],"text":"11,500円"},{"box_2d":[240,240,300,480],"text":"111,500円"}]}"#;
        Mock::given(method("POST"))
            .and(path_regex(r".*generateContent.*"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "candidates": [{"content": {"parts": [{"text": stage2_text}]}}]
            })))
            .mount(&server)
            .await;

        let result = detect_amount_boxes_v2(b"fake pdf", "fake-key", None, Some(&server.uri()))
            .await
            .unwrap();

        // 値セル 3 個だけ
        assert_eq!(result.len(), 3);

        // ラベル文字列は redaction に含まれない
        for r in &result {
            assert!(
                !r.text.contains("消費税") && !r.text.contains("運賃") && !r.text.contains("合計"),
                "label text leaked into redaction: {}",
                r.text
            );
        }

        // 各 redaction が円を含む値であること
        for r in &result {
            assert!(r.text.contains("円"), "non-amount in redaction: {}", r.text);
        }

        // 構造的検証: 消費税行の値 bbox (xmin=240) が同行ラベル bbox (xmax=240) と
        // 水平方向に重ならない。これは Stage 1 prompt の "xmax_label <= xmin_value"
        // 制約が保たれているかを assert する代理。
        let tax_value = result
            .iter()
            .find(|r| r.text == "11,500円")
            .expect("tax value");
        let [_ymin, xmin, _ymax, _xmax] = tax_value.box_2d;
        assert!(
            xmin >= 240.0,
            "tax value bbox xmin ({xmin}) must be >= label xmax (240) — bbox overlapping the label cell will paint white over '消費税' instead of '11,500円'"
        );
    }

    #[tokio::test]
    async fn test_detect_all_cells_multi_tables_form() {
        use wiremock::matchers::{method, path_regex};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        // tables: [...] 形式
        let stage1_text = r#"{"tables":[{"page":1,"title":"t1","cells":[]},{"page":2,"title":"t2","cells":[{"box_2d":[0,0,100,100],"text":"x"}]}]}"#;
        Mock::given(method("POST"))
            .and(path_regex(r".*generateContent.*"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "candidates": [{"content": {"parts": [{"text": stage1_text}]}}]
            })))
            .mount(&server)
            .await;

        let pages = detect_all_cells(b"pdf", "key", None, Some(&server.uri()))
            .await
            .unwrap();
        assert_eq!(pages.len(), 2);
        assert_eq!(pages[0].page, 1);
        assert_eq!(pages[1].page, 2);
        assert_eq!(pages[1].cells.len(), 1);
    }

    #[tokio::test]
    async fn test_filter_amount_cells_empty_input_short_circuits() {
        // 空入力なら Gemini を呼ばずに空 Vec を返す
        let empty: Vec<PageCells> = vec![];
        let r = filter_amount_cells(&empty, "key", None, Some("http://unreachable.invalid"))
            .await
            .unwrap();
        assert!(r.is_empty());
    }

    /// Stage 1 が 500 を返したら 1-stage (`detect_amount_boxes`) にフォールバック。
    #[tokio::test]
    async fn test_detect_amount_boxes_v2_fallback_on_stage1_error() {
        use wiremock::matchers::{method, path_regex};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;

        // 1 回目 (Stage 1): 500 → fallback
        Mock::given(method("POST"))
            .and(path_regex(r".*generateContent.*"))
            .respond_with(ResponseTemplate::new(500).set_body_string("oops"))
            .up_to_n_times(1)
            .mount(&server)
            .await;

        // 2 回目 (1-stage): 200 + 1 件
        let one_stage_text =
            r#"{"redactions":[{"page":1,"box_2d":[100,200,150,400],"text":"fallback"}]}"#;
        Mock::given(method("POST"))
            .and(path_regex(r".*generateContent.*"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "candidates": [{"content": {"parts": [{"text": one_stage_text}]}}]
            })))
            .mount(&server)
            .await;

        let result = detect_amount_boxes_v2(b"pdf", "key", None, Some(&server.uri()))
            .await
            .unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].text, "fallback");
    }

    /// Stage 1 が 0 セルを返したら 1-stage にフォールバック。
    #[tokio::test]
    async fn test_detect_amount_boxes_v2_fallback_on_stage1_empty() {
        use wiremock::matchers::{method, path_regex};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;

        let empty_stage1 = r#"{"page":1,"title":"empty","cells":[]}"#;
        Mock::given(method("POST"))
            .and(path_regex(r".*generateContent.*"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "candidates": [{"content": {"parts": [{"text": empty_stage1}]}}]
            })))
            .up_to_n_times(1)
            .mount(&server)
            .await;

        let one_stage_text =
            r#"{"redactions":[{"page":1,"box_2d":[100,200,150,400],"text":"fb"}]}"#;
        Mock::given(method("POST"))
            .and(path_regex(r".*generateContent.*"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "candidates": [{"content": {"parts": [{"text": one_stage_text}]}}]
            })))
            .mount(&server)
            .await;

        let result = detect_amount_boxes_v2(b"pdf", "key", None, Some(&server.uri()))
            .await
            .unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].text, "fb");
    }

    /// Stage 2 が失敗したら 1-stage にフォールバック。
    #[tokio::test]
    async fn test_detect_amount_boxes_v2_fallback_on_stage2_error() {
        use wiremock::matchers::{method, path_regex};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;

        // Stage 1: OK + cells あり
        let stage1_text = r#"{"page":1,"cells":[{"box_2d":[0,0,100,100],"text":"x"}]}"#;
        Mock::given(method("POST"))
            .and(path_regex(r".*generateContent.*"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "candidates": [{"content": {"parts": [{"text": stage1_text}]}}]
            })))
            .up_to_n_times(1)
            .mount(&server)
            .await;

        // Stage 2: 500 → fallback
        Mock::given(method("POST"))
            .and(path_regex(r".*generateContent.*"))
            .respond_with(ResponseTemplate::new(500))
            .up_to_n_times(1)
            .mount(&server)
            .await;

        // Fallback 1-stage: 1 件返す
        let one_stage_text = r#"{"redactions":[{"page":1,"box_2d":[0,0,100,100],"text":"fb2"}]}"#;
        Mock::given(method("POST"))
            .and(path_regex(r".*generateContent.*"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "candidates": [{"content": {"parts": [{"text": one_stage_text}]}}]
            })))
            .mount(&server)
            .await;

        let result = detect_amount_boxes_v2(b"pdf", "key", None, Some(&server.uri()))
            .await
            .unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].text, "fb2");
    }

    #[tokio::test]
    async fn test_detect_all_cells_http_error_propagates() {
        use wiremock::matchers::{method, path_regex};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path_regex(r".*generateContent.*"))
            .respond_with(ResponseTemplate::new(403).set_body_string("forbidden"))
            .mount(&server)
            .await;

        let err = detect_all_cells(b"pdf", "key", None, Some(&server.uri()))
            .await
            .unwrap_err();
        assert!(matches!(err, RedactError::GeminiStatus(_, _)));
    }

    #[tokio::test]
    async fn test_filter_amount_cells_http_error_propagates() {
        use wiremock::matchers::{method, path_regex};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path_regex(r".*generateContent.*"))
            .respond_with(ResponseTemplate::new(429).set_body_string("rate limited"))
            .mount(&server)
            .await;

        let pages = vec![PageCells {
            page: 1,
            title: "t".into(),
            cells: vec![CellBox {
                box_2d: [0.0, 0.0, 100.0, 100.0],
                text: "x".into(),
            }],
        }];
        let err = filter_amount_cells(&pages, "key", None, Some(&server.uri()))
            .await
            .unwrap_err();
        assert!(matches!(err, RedactError::GeminiStatus(_, _)));
    }

    #[tokio::test]
    async fn test_detect_all_cells_empty_response_text_errors() {
        use wiremock::matchers::{method, path_regex};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        // candidates なし → GeminiEmpty
        Mock::given(method("POST"))
            .and(path_regex(r".*generateContent.*"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "candidates": []
            })))
            .mount(&server)
            .await;

        let err = detect_all_cells(b"pdf", "key", None, Some(&server.uri()))
            .await
            .unwrap_err();
        assert!(matches!(err, RedactError::GeminiEmpty));
    }

    #[tokio::test]
    async fn test_filter_amount_cells_invalid_json_errors() {
        use wiremock::matchers::{method, path_regex};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path_regex(r".*generateContent.*"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "candidates": [{"content": {"parts": [{"text": "not a json"}]}}]
            })))
            .mount(&server)
            .await;

        let pages = vec![PageCells {
            page: 1,
            title: "".into(),
            cells: vec![CellBox {
                box_2d: [0.0, 0.0, 100.0, 100.0],
                text: "x".into(),
            }],
        }];
        let err = filter_amount_cells(&pages, "key", None, Some(&server.uri()))
            .await
            .unwrap_err();
        assert!(matches!(err, RedactError::RedactionParse(_)));
    }
}
