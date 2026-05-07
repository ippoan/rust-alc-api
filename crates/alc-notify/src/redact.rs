//! PDF 金額黒塗り (redaction) の中核ロジック。
//!
//! 1. `detect_amount_boxes` — Gemini API に PDF を inlineData として直接送り、
//!    金額表記の bounding box を JSON で返してもらう。
//! 2. `apply_redactions` — 受け取った bbox を `lopdf` で各ページの content stream
//!    末尾に「白矩形描画」命令として追記し、新しい PDF バイト列を返す。
//!
//! 既存描画の **上に** 白矩形を重ねる方式なので、元の PDF 構造 (ページ数 / 埋め込み
//! 画像 / フォーム) はそのまま保持される。FAX 由来のスキャン画像 PDF (= 1 ページ
//! 1 埋め込み画像) でも問題なく動作する。
//!
//! 設計ドキュメント: `~/.claude/projects/-home-yhonda-rust-rust-alc-api/memory/notify_pdf_redact_design.md`

use base64::Engine;
use lopdf::{dictionary, Document, Object, ObjectId, Stream};

const GEMINI_DEFAULT_ENDPOINT: &str = "https://generativelanguage.googleapis.com/v1beta";
// gemini-2.0-flash は 2026 年に新規ユーザー向け提供終了 (404)。
// gemini-3.1-flash-lite-preview を採用 — 軽量 / 高速 / PDF inlineData + JSON 出力対応。
// 2026-05-07 staging で実 API 200 OK 確認済み。stable suffix が付いたら更新。
const GEMINI_DEFAULT_MODEL: &str = "gemini-3.1-flash-lite-preview";

/// プロンプト本文。Gemini に「金額の bounding box だけ」を JSON で返させる。
const REDACT_PROMPT: &str = r#"この PDF 内の「金額表記」の位置をすべて検出し、JSON で返してください。

対象となる金額の例: 運賃 / 代金 / 消費税 / 合計 / 支払額。
形式: 「N円」「￥N」「JPY N」「N円(税別)」「N円(税込)」など、N にカンマや全角数字を含むものすべて。

出力形式 (これ以外の文字を一切出力しない):
{
  "redactions": [
    {
      "page": 1,
      "box_2d": [ymin, xmin, ymax, xmax],
      "text": "160,000円"
    }
  ]
}

ルール:
- box_2d は 0-1000 で正規化された [ymin, xmin, ymax, xmax] (Gemini 標準 bbox 形式)
- page は 1-origin
- 金額のみ。氏名・電話番号・FAX 番号・郵便番号・車両ナンバー・住所などは含めない
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

    let list: RedactionList = serde_json::from_str(text).map_err(RedactError::RedactionParse)?;
    Ok(list.redactions)
}

/// PDF バイト列に「白矩形オーバーレイ」を適用して、新しい PDF バイト列を返す。
/// pure 関数 (HTTP / DB に触らない)。
pub fn apply_redactions(
    pdf_bytes: &[u8],
    redactions: &[RedactionBox],
) -> Result<Vec<u8>, RedactError> {
    let mut doc = Document::load_mem(pdf_bytes)?;

    // 1-origin page → ObjectId への変換テーブル
    let pages: Vec<ObjectId> = doc.get_pages().into_values().collect();

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

        let page_id = pages[r.page - 1];
        let (page_w, page_h) = page_size(&doc, page_id)?;

        // box_2d は 0-1000 / 左上原点 → PDF 座標 (左下原点 pt) に変換
        let x = (xmin / 1000.0) * page_w;
        let y = page_h - (ymax / 1000.0) * page_h;
        let bw = ((xmax - xmin) / 1000.0) * page_w;
        let bh = ((ymax - ymin) / 1000.0) * page_h;

        let cmd = format!("q 1 1 1 rg {x:.2} {y:.2} {bw:.2} {bh:.2} re f Q\n");
        let stream = Stream::new(dictionary! {}, cmd.into_bytes());
        let stream_id = doc.add_object(stream);

        append_content_stream(&mut doc, page_id, stream_id)?;
    }

    let mut out = Vec::new();
    doc.save_to(&mut out)?;
    Ok(out)
}

/// Page の MediaBox から (width, height) を pt 単位で取得。
fn page_size(doc: &Document, page_id: ObjectId) -> Result<(f32, f32), RedactError> {
    // 自分の MediaBox がなければ親 (Pages tree) を辿る
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
    // フォールバック: A4 縦
    Ok((595.0, 842.0))
}

fn obj_to_f32(o: &Object) -> Result<f32, lopdf::Error> {
    match o {
        Object::Integer(i) => Ok(*i as f32),
        Object::Real(r) => Ok(*r),
        _ => Err(lopdf::Error::ObjectNotFound),
    }
}

/// Page の Contents に新しい Stream を末尾追加する。
/// Contents は Reference 単一 / 配列 / なし のどれかなので、それぞれ対応。
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

#[cfg(test)]
mod tests {
    use super::*;

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
        let pdf = minimal_pdf();
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
        let pdf = minimal_pdf();
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
        let pdf = minimal_pdf();
        let out = apply_redactions(&pdf, &[]).unwrap();
        // 出力 PDF は valid (再 load できる) こと
        let _doc = Document::load_mem(&out).unwrap();
    }

    #[test]
    fn test_apply_redactions_overlay_succeeds() {
        let pdf = minimal_pdf();
        let r = vec![RedactionBox {
            page: 1,
            box_2d: [100.0, 100.0, 200.0, 300.0],
            text: "100円".into(),
        }];
        let out = apply_redactions(&pdf, &r).unwrap();
        let doc = Document::load_mem(&out).unwrap();
        // ページ数は 1 のまま
        assert_eq!(doc.get_pages().len(), 1);
    }

    /// テスト用: A4 1 ページの最小 PDF
    fn minimal_pdf() -> Vec<u8> {
        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let page_id = doc.new_object_id();

        // Catalog
        let catalog_id = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        doc.trailer.set("Root", catalog_id);

        // Pages tree
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Count" => 1,
                "Kids" => vec![page_id.into()],
                "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
            }),
        );

        // Page
        doc.objects.insert(
            page_id,
            Object::Dictionary(dictionary! {
                "Type" => "Page",
                "Parent" => pages_id,
            }),
        );

        let mut out = Vec::new();
        doc.save_to(&mut out).unwrap();
        out
    }
}
