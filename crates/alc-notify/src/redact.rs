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
use image::ImageEncoder;
use lopdf::{Document, Object, ObjectId, Stream};

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
- bbox は数字 + 単位 (円) を確実に **完全に内包する** ように、左右に余裕を持たせる
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

        // XObject Image stream を取得し、JPEG bytes (`content`) と寸法 (`Width`,
        // `Height`) を読む。
        let (img_bytes, img_w, img_h, dict_keys) = {
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
            // dict の他キーは温存して再構築するため key 一覧を控える
            let keys: Vec<Vec<u8>> = stream.dict.iter().map(|(k, _)| k.to_vec()).collect();
            (stream.content.clone(), w, h, keys)
        };

        // 3) JPEG decode → 白矩形描画 → JPEG re-encode
        let mut img = image::load_from_memory(&img_bytes)?.to_rgb8();
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

        let mut new_jpeg = Vec::with_capacity(img_bytes.len());
        // quality 90: 元の FAX スキャンが既に低品質なので 90 で十分、ファイル膨張も
        // 抑えられる。
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut new_jpeg, 90).write_image(
            img.as_raw(),
            real_w,
            real_h,
            image::ExtendedColorType::Rgb8,
        )?;

        // 4) Stream の content だけ差し替え。dict (Filter=DCTDecode、Width/Height、
        //    ColorSpace 等) は元のまま保持する。
        // 寸法が変わらないので Width/Height も書き換え不要。
        let stream = doc.get_object_mut(image_obj_id)?.as_stream_mut()?;
        stream.set_content(new_jpeg);
        // dict_keys は将来の検証用 (本実装では未使用)。dead code 警告を抑止。
        let _ = dict_keys;
    }

    let mut out = Vec::new();
    doc.save_to(&mut out)?;
    Ok(out)
}

/// 指定ページの Resources/XObject に登録されている画像 XObject の **最初の 1 つ**
/// の ObjectId を返す。FAX スキャン PDF はページに大きな JPEG 1 枚という構造が
/// 大半なので、これで十分。
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
        let is_image = stream
            .dict
            .get(b"Subtype")
            .ok()
            .and_then(|o| o.as_name().ok())
            .map(|n| n == b"Image")
            .unwrap_or(false);
        if is_image {
            return Ok(Some(id));
        }
    }
    Ok(None)
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
}
