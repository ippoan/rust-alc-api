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
#[cfg(test)]
use lopdf::Stream;
use lopdf::{Document, Object, ObjectId};
use std::io::Read;
use std::sync::{Mutex, OnceLock};

const GEMINI_DEFAULT_ENDPOINT: &str = "https://generativelanguage.googleapis.com/v1beta";
// gemini-2.0-flash は 2026 年に新規ユーザー向け提供終了 (404)。
// gemini-3.1-flash-lite-preview を採用 — 軽量 / 高速 / PDF inlineData + JSON 出力対応。
// 2026-05-07 staging で実 API 200 OK 確認済み。stable suffix が付いたら更新。
const GEMINI_DEFAULT_MODEL: &str = "gemini-3.1-flash-lite-preview";

/// プロンプト本文 — **「円」を含むテキストの bbox** を直接返してもらう方式。
///
/// 旧版は「金額が入った表のセル全体の bbox」を要求していたが、Gemini はセル境界
/// 判定を時々間違えて隣のラベル行/担当者行に bbox を置いてしまう問題があった
/// (3164_001.pdf の 11,500円 が 担当者情報 行に飛ぶ等)。
///
/// 新方式: **画像から「N円」を含む文字列を OCR 的に検出してその文字バウンディング
/// ボックスをそのまま返してもらう**。セル罫線解釈に依存しないので誤行に飛ぶことが
/// 構造的に発生しない。bbox に少し padding を足すのは呼び出し側 (apply_redactions)
/// で対応する。
const REDACT_PROMPT: &str = r#"この画像は業務帳票 (1 ページ) の rasterize 画像です。
**画像中に印字または手書きされた「N円」表記の金額** を全て検出し、その文字列の
バウンディングボックスをそのまま返してください。表のセル全体ではなく、文字列の
タイトな矩形 (上下左右に最小限のマージン付き) で十分です。

対象パターン (これらに合致するテキストは全て検出):
- 「123,456円」「12,345円(税別)」「12,345円(税込)」
- 「￥123,456」「JPY 12,345」
- 数字とカンマと「円」「￥」「JPY」がセットで現れた箇所すべて

検出しないもの:
- 「3,200 kg」「9PL」「10t」「10:00」など「円/￥/JPY」が無い数字
- 氏名・電話番号・FAX 番号・郵便番号・車両ナンバー・住所
- 日付 (例: "2026/06/30")

出力形式 (Structured Output 強制、ストリーミング外文字は出さない):
{
  "redactions": [
    {
      "page": 1,
      "box_2d": [ymin, xmin, ymax, xmax],
      "text": "115,000円",
      "cell_label": "運賃 (任意)"
    }
  ]
}

ルール:
- box_2d は 0-1000 で正規化された [ymin, xmin, ymax, xmax] (左上原点、Gemini 標準形式)
- page は常に 1
- **bbox は必ず実際の文字列の上にあること** (空白セルを返さない)
- text に「円/￥/JPY」のいずれかが含まれること
- 該当が無ければ {"redactions": []} を返す"#;

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
    #[error("pdfium: {0}")]
    Pdfium(String),
}

/// `pdfium-render::Pdfium` は内部に `Box<dyn PdfiumLibraryBindings>` を持つ。
/// dyn trait は自動で `Send + Sync` を持たないため、そのままでは `static` に
/// 置けない。実体は FFI でロードした共有ライブラリ (libpdfium.so) なのでプロセス
/// 内のどのスレッドからアクセスしても問題ない。Mutex 越しに排他化することで
/// PDFium 本体のスレッドアンセーフ性も保護する。
struct PdfiumGuard(pdfium_render::prelude::Pdfium);
// SAFETY: PDFium ライブラリ自体は dlopen された .so で、ハンドル + 関数ポインタは
// プロセス全体で有効。Mutex で排他しているので並行呼び出しは発生しない。
unsafe impl Send for PdfiumGuard {}
unsafe impl Sync for PdfiumGuard {}

/// 共有 Pdfium インスタンス。最初の `apply_redactions` 呼び出し時に
/// `libpdfium.so` を `dlopen` する。Docker image では `/usr/lib/libpdfium.so`
/// に bblanchon/pdfium-binaries の linux-x64 prebuilt を配置している。
fn pdfium_locked() -> Result<&'static Mutex<PdfiumGuard>, RedactError> {
    use pdfium_render::prelude::Pdfium;
    static INSTANCE: OnceLock<Result<Mutex<PdfiumGuard>, String>> = OnceLock::new();
    INSTANCE
        .get_or_init(|| {
            Pdfium::bind_to_system_library()
                .map(|b| Mutex::new(PdfiumGuard(Pdfium::new(b))))
                .map_err(|e| format!("bind_to_system_library: {e}"))
        })
        .as_ref()
        .map_err(|e| RedactError::Pdfium(e.clone()))
}

/// PDF 1 ページ目を pdfium で 200 DPI ラスタライズして JPEG bytes を返す。
///
/// 用途: Gemini に「PDF」ではなく「rasterize 済 JPEG」を送るため。元の PDF が
/// 多層構造 (Obj4 JPEG + Obj5 CCITT ImageMask など) の場合、Gemini に PDF を
/// inlineData で渡すと内部のレンダリング基準が不明で bbox が現実のレイアウト
/// とズレるケースがある (3164_001.pdf で実測 ~5pp ズレ)。同じ pdfium で
/// rasterize した JPEG を bbox 検出にも mask 適用にも使えば、座標系が完全に
/// 揃うため必ず一致する。
pub fn rasterize_first_page_jpeg(pdf_bytes: &[u8]) -> Result<Vec<u8>, RedactError> {
    use pdfium_render::prelude::PdfRenderConfig;
    let pdfium_mu = pdfium_locked()?;
    let pdfium_guard = pdfium_mu
        .lock()
        .map_err(|e| RedactError::Pdfium(format!("mutex poisoned: {e}")))?;
    let document = pdfium_guard
        .0
        .load_pdf_from_byte_slice(pdf_bytes, None)
        .map_err(|e| RedactError::Pdfium(format!("load_pdf: {e}")))?;
    let pages = document.pages();
    if pages.is_empty() {
        return Err(RedactError::PageNoImage(1));
    }
    let page = pages
        .get(0)
        .map_err(|e| RedactError::Pdfium(format!("get page 0: {e}")))?;
    let render_config = PdfRenderConfig::new().scale_page_by_factor(300.0 / 72.0);
    let bitmap = page
        .render_with_config(&render_config)
        .map_err(|e| RedactError::Pdfium(format!("render p1: {e}")))?;
    let rgb = bitmap.as_image().into_rgb8();
    let (w, h) = rgb.dimensions();
    let mut jpeg = Vec::with_capacity((w as usize * h as usize) / 8);
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 90).write_image(
        rgb.as_raw(),
        w,
        h,
        image::ExtendedColorType::Rgb8,
    )?;
    Ok(jpeg)
}

/// Gemini API を叩いて、PDF 内の金額表記の bbox を取得する。
///
/// 内部的には PDF を pdfium で 200 DPI rasterize して JPEG にしてから送る。
/// この同じ JPEG (の同じピクセル座標系) を `apply_redactions` も使うので、
/// Gemini が返した bbox は必ずマスク描画位置と整合する。
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
    // PDF ではなく **rasterize 済 JPEG** を送る (3164 系 multi-layer PDF で bbox が
    // ズレる問題の根治: rasterize → 同じ JPEG で bbox 取得 → 同じ JPEG にマスク)。
    let jpeg = rasterize_first_page_jpeg(pdf_bytes)?;
    let img_b64 = base64::engine::general_purpose::STANDARD.encode(&jpeg);

    let body = serde_json::json!({
        "contents": [{
            "role": "user",
            "parts": [
                { "inlineData": { "mimeType": "image/jpeg", "data": img_b64 } },
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
- 業務帳票では「ラベル文字列だけが入ったセル」(例: 運賃 / 消費税 / 合計 / 合計金額 /
  合算 / 総額 / 請求額 / 支払額 / 支払運賃 / 高速代 / 搬出料 / 燃料サーチャージ /
  駐車代 / 待機料 / 通関料 / 付帯作業料 / 入金予定日) とその隣の「金額が入った値セル」
  (例: 100,000円 / 11,500円 / 127,000円) は **必ず別の box_2d** として返すこと。
  1 つの box_2d にラベル文字 + 金額を同時に含めてはならない。
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
- 罫線をたどって表全体を網羅 (取りこぼし禁止)
- **「N円」パターンは絶対に取りこぼさない**: 表の右下や 1 行だけの小さい領域でも、
  「数字 + 円」を含むセルは必ず単独の box_2d として返す
  (例: 「合計金額: 127,000円」が 1 行で書かれていても、「合計金額」と「127,000円」を
  別セルとして 2 つ列挙する)"#;

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
    // PDF ではなく rasterize 済 JPEG を送る (詳細は detect_amount_boxes コメント)。
    let jpeg = rasterize_first_page_jpeg(pdf_bytes)?;
    let img_b64 = base64::engine::general_purpose::STANDARD.encode(&jpeg);

    // Stage 1 は全セル列挙なので token 上限を多めに (帳票によっては 50+ セル)
    let body = serde_json::json!({
        "contents": [{
            "role": "user",
            "parts": [
                { "inlineData": { "mimeType": "image/jpeg", "data": img_b64 } },
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
/// 入力 PDF の 1 ページ目を pdfium で rasterize → 黒矩形を画素単位で焼き込む
/// → JPEG (quality 85) を返す。**出力は PDF ではなく JPEG bytes**。
///
/// ## 設計判断
///
/// 1. **PDF wrapping を廃止**: 配信先 (LINE / LINE WORKS / nuxt-notify viewer)
///    は最終的に画像 1 枚を見せるだけなので、PDF 再構築の overhead と複雑性を
///    削った。多ページ PDF はとりあえず page 1 のみ対応。
///
/// 2. **rasterize で全レイヤー合成**: Canon iR-ADV C5535 III 系 (3164_001.pdf)
///    のような Obj4 JPEG + Obj5 CCITT ImageMask + Obj6 ロゴ多層 PDF も、pdfium が
///    全レイヤーを合成した最終ピクセルにマスクを焼き込むので確実に隠せる。
///
/// 3. **bbox 整合**: bbox は `detect_amount_boxes` 系が同じ pdfium で rasterize した
///    JPEG を Gemini に送って取得した値。同じ JPEG (= 同じピクセル座標系) に
///    マスクを描くので必ず正しい位置に揃う。
pub fn apply_redactions(
    pdf_bytes: &[u8],
    redactions: &[RedactionBox],
) -> Result<Vec<u8>, RedactError> {
    use pdfium_render::prelude::PdfRenderConfig;

    // 1) bbox 検証
    for r in redactions {
        if r.page == 0 {
            return Err(RedactError::PageNotFound(r.page));
        }
        if r.page != 1 {
            // 多ページ PDF は将来対応。当面 page 1 以外は弾く。
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
    }

    // 2) Pdfium で PDF を開いて 1 ページ目を 200 DPI rasterize
    let pdfium_mu = pdfium_locked()?;
    let pdfium_guard = pdfium_mu
        .lock()
        .map_err(|e| RedactError::Pdfium(format!("mutex poisoned: {e}")))?;
    let document = pdfium_guard
        .0
        .load_pdf_from_byte_slice(pdf_bytes, None)
        .map_err(|e| RedactError::Pdfium(format!("load_pdf: {e}")))?;
    let pages = document.pages();
    if pages.is_empty() {
        return Err(RedactError::PageNoImage(1));
    }
    let page = pages
        .get(0)
        .map_err(|e| RedactError::Pdfium(format!("get page 0: {e}")))?;
    let mb_w_pt = page.width().value;
    let mb_h_pt = page.height().value;

    let render_config = PdfRenderConfig::new().scale_page_by_factor(300.0 / 72.0);
    let bitmap = page
        .render_with_config(&render_config)
        .map_err(|e| RedactError::Pdfium(format!("render p1: {e}")))?;
    let mut rgb = bitmap.as_image().into_rgb8();
    let (img_w, img_h) = rgb.dimensions();

    tracing::info!(
        "redact[rasterize] page={mb_w_pt:.1}x{mb_h_pt:.1}pt → img={img_w}x{img_h}px, {} redaction(s)",
        redactions.len()
    );

    // 3) 黒矩形でマスク
    for (i, r) in redactions.iter().enumerate() {
        let [ymin, xmin, ymax, xmax] = r.box_2d;
        let px = ((xmin / 1000.0) * img_w as f32).round().max(0.0) as u32;
        let py = ((ymin / 1000.0) * img_h as f32).round().max(0.0) as u32;
        let pw = ((xmax - xmin) / 1000.0 * img_w as f32).round().max(0.0) as u32;
        let ph = ((ymax - ymin) / 1000.0 * img_h as f32).round().max(0.0) as u32;
        let x0 = px.min(img_w);
        let y0 = py.min(img_h);
        let x_end = px.saturating_add(pw).min(img_w);
        let y_end = py.saturating_add(ph).min(img_h);
        tracing::info!(
            "redact[rasterize] r[{i}] text={:?} bbox=[{ymin:.1},{xmin:.1},{ymax:.1},{xmax:.1}] → px=({x0},{y0})→({x_end},{y_end})",
            r.text
        );
        for y in y0..y_end {
            for x in x0..x_end {
                rgb.put_pixel(x, y, image::Rgb([0, 0, 0]));
            }
        }
    }

    // 4) JPEG エンコード (quality 85)
    let mut jpeg = Vec::with_capacity((img_w as usize * img_h as usize) / 8);
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 85).write_image(
        rgb.as_raw(),
        img_w,
        img_h,
        image::ExtendedColorType::Rgb8,
    )?;
    Ok(jpeg)
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

/// PDF の 1 ページ目に埋め込まれた最大サイズの DCTDecode (JPEG) 画像を取り出す。
///
/// FAX 由来 PDF が前提 (1 page = 1 image XObject、DCTDecode フィルタ)。FAX 系は
/// JPEG の上に zlib 圧縮を被せていることが多いので、`/FlateDecode` 配列が含まれる
/// ケースは Flate を解凍してから生 JPEG bytes を返す。
///
/// LINE / LINE WORKS の image メッセージで配信するため、redact 済 PDF から
/// 「画面で見えている画像そのもの」を取り出す用途。`apply_redactions` の前半部分と
/// 同じロジックで、書き換えはしない。
///
/// pure 関数 (HTTP / DB に触らない)。テキスト PDF 等で JPEG が見つからない場合は
/// `PageNoImage(1)` を返す。
pub fn extract_first_page_jpeg(pdf_bytes: &[u8]) -> Result<Vec<u8>, RedactError> {
    let doc = Document::load_mem(pdf_bytes)?;
    let pages: Vec<ObjectId> = doc.get_pages().into_values().collect();
    if pages.is_empty() {
        return Err(RedactError::PageNotFound(1));
    }
    let page_id = pages[0];
    let image_obj_id =
        find_first_image_xobject(&doc, page_id)?.ok_or(RedactError::PageNoImage(1))?;
    let stream = doc.get_object(image_obj_id)?.as_stream()?;
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
    Ok(jpeg)
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
    fn test_apply_redactions_empty_returns_clean_jpeg() {
        // redactions=空でも apply_redactions は rasterize → JPEG を返す
        // (出力 PDF wrapping を廃止したので空時のショートカット返しは無し)。
        let pdf = pdf_with_jpeg_image(50, 50);
        let out = apply_redactions(&pdf, &[]).unwrap();
        // 出力は JPEG (SOI = 0xFFD8FF...)
        assert!(out.len() > 100, "output too short: {}", out.len());
        assert_eq!(&out[0..3], &[0xFF, 0xD8, 0xFF], "expected JPEG SOI marker");
    }

    #[test]
    fn test_apply_redactions_paints_black_on_jpeg() {
        // 元画像は全面赤。bbox = 全面。redact 後の JPEG は全面 黒 になっているはず。
        let pdf = pdf_with_jpeg_image(40, 40);
        let r = vec![RedactionBox {
            page: 1,
            box_2d: [0.0, 0.0, 1000.0, 1000.0],
            text: "x".into(),
        }];
        let out = apply_redactions(&pdf, &r).unwrap();
        // 出力 JPEG を decode して中央ピクセルが黒 (0,0,0) であることを確認
        let img = image::load_from_memory(&out).unwrap().to_rgb8();
        let (w, h) = img.dimensions();
        let center = img.get_pixel(w / 2, h / 2);
        assert!(
            center.0[0] < 30 && center.0[1] < 30 && center.0[2] < 30,
            "center pixel should be black, got {:?}",
            center.0
        );
    }

    // Note: test_apply_redactions_no_image_returns_error was removed when redact.rs
    // switched to pdfium rasterize. The old code path required an embedded JPEG XObject
    // and surfaced PageNoImage when none existed. With rasterize, even text-only PDFs
    // (no image XObject) are rendered to a JPEG by pdfium and pass through normally.

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

        // Contents — pdfium がレンダリング時に Im0 を MediaBox 全体に描画するように、
        // image-space (1×1) → page-space (595×842) の CTM を載せた最小 stream。
        // (旧テストはこの Contents を持たず pdfium レンダリング結果が空白になった。)
        let content = b"q\n595 0 0 842 0 0 cm\n/Im0 Do\nQ\n".to_vec();
        let content_id = doc.add_object(Stream::new(dictionary! {}, content));

        // Page
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "Resources" => resources_id,
            "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
            "Contents" => content_id,
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

        let result = detect_amount_boxes_v2(
            &pdf_with_jpeg_image(40, 40),
            "fake-key",
            None,
            Some(&server.uri()),
        )
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

        let result = detect_amount_boxes_v2(
            &pdf_with_jpeg_image(40, 40),
            "fake-key",
            None,
            Some(&server.uri()),
        )
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

        let pages = detect_all_cells(
            &pdf_with_jpeg_image(40, 40),
            "key",
            None,
            Some(&server.uri()),
        )
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

        let result = detect_amount_boxes_v2(
            &pdf_with_jpeg_image(40, 40),
            "key",
            None,
            Some(&server.uri()),
        )
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

        let result = detect_amount_boxes_v2(
            &pdf_with_jpeg_image(40, 40),
            "key",
            None,
            Some(&server.uri()),
        )
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

        let result = detect_amount_boxes_v2(
            &pdf_with_jpeg_image(40, 40),
            "key",
            None,
            Some(&server.uri()),
        )
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

        let err = detect_all_cells(
            &pdf_with_jpeg_image(40, 40),
            "key",
            None,
            Some(&server.uri()),
        )
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

        let err = detect_all_cells(
            &pdf_with_jpeg_image(40, 40),
            "key",
            None,
            Some(&server.uri()),
        )
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
