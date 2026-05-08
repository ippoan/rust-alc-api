//! 公開 viewer (ログイン不要) — nuxt-notify の `/v/{token}` ページから呼ばれる。
//!
//! 既読化はしない (それは `/api/notify/read/{token}` の責務)。
//! トークンが有効である限り何度でも閲覧できる。
//!
//! - GET /api/notify/v/{token}      → メタデータ JSON (件名 / 送信者 / ファイル名 / 受信日時 / 期限)
//! - GET /api/notify/v/{token}/file → ファイル本体ストリーム (`Content-Disposition: inline`)
//!
//! file エンドポイントは R2 へのリダイレクトではなく **同一オリジンで bytes を返す**。
//! 理由: PDF.js (フロントエンド canvas 描画) が R2 を直接 fetch すると CORS で失敗する。
//! API ストリームなら既存の `CorsLayer::allow_origin(Any)` で fetch 可能で、
//! LINE/LINE WORKS 内蔵 webview のような PDF をネイティブ表示できない環境でも canvas 描画できる。

use axum::{
    extract::{Path, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json, Router,
};
use uuid::Uuid;

use alc_core::repository::notify_deliveries::DeliveryViewInfo;
use alc_core::AppState;

pub fn public_router() -> Router<AppState> {
    Router::new()
        .route("/notify/v/{token}", axum::routing::get(view_metadata))
        .route("/notify/v/{token}/file", axum::routing::get(view_file))
        .route("/notify/v/{token}/image", axum::routing::get(view_image))
}

#[derive(serde::Serialize, Debug, PartialEq)]
pub struct ViewMetadata {
    pub file_name: Option<String>,
    pub file_size_bytes: Option<i64>,
    pub source_subject: Option<String>,
    pub source_sender: Option<String>,
    pub source_received_at: Option<chrono::DateTime<chrono::Utc>>,
    pub expire_at: chrono::DateTime<chrono::Utc>,
}

pub(crate) fn build_metadata(info: &DeliveryViewInfo) -> ViewMetadata {
    ViewMetadata {
        file_name: info.file_name.clone(),
        file_size_bytes: info.file_size_bytes,
        source_subject: info.source_subject.clone(),
        source_sender: info.source_sender.clone(),
        source_received_at: info.source_received_at,
        expire_at: info.expire_at,
    }
}

/// 期限切れなら 410 Gone、有効なら Ok(())
pub(crate) fn check_not_expired(
    expire_at: chrono::DateTime<chrono::Utc>,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<(), StatusCode> {
    if expire_at <= now {
        return Err(StatusCode::GONE);
    }
    Ok(())
}

/// ファイル名から content-type を推測する。
/// PDF が圧倒的多数なので不明拡張子は `application/pdf` に倒す。
pub(crate) fn guess_content_type(file_name: Option<&str>) -> &'static str {
    let name = file_name.unwrap_or("").to_ascii_lowercase();
    if name.ends_with(".pdf") {
        "application/pdf"
    } else if name.ends_with(".png") {
        "image/png"
    } else if name.ends_with(".jpg") || name.ends_with(".jpeg") {
        "image/jpeg"
    } else if name.ends_with(".gif") {
        "image/gif"
    } else if name.ends_with(".webp") {
        "image/webp"
    } else if name.ends_with(".svg") {
        "image/svg+xml"
    } else if name.ends_with(".txt") {
        "text/plain; charset=utf-8"
    } else {
        "application/pdf"
    }
}

/// `Content-Disposition: inline; filename="..."; filename*=UTF-8''...` を組み立てる。
/// RFC 5987 形式で UTF-8 ファイル名を安全にエンコードする。
pub(crate) fn build_inline_disposition(file_name: Option<&str>) -> String {
    let display = file_name.unwrap_or("attachment");
    let encoded = urlencoding::encode(display);
    format!(
        "inline; filename=\"{}\"; filename*=UTF-8''{}",
        display.replace('"', "_"),
        encoded
    )
}

async fn view_metadata(
    State(state): State<AppState>,
    Path(token): Path<Uuid>,
) -> Result<Json<ViewMetadata>, StatusCode> {
    let info = state
        .notify_deliveries
        .get_for_view(token)
        .await
        .map_err(|e| {
            tracing::error!("get_for_view: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    check_not_expired(info.expire_at, chrono::Utc::now())?;
    Ok(Json(build_metadata(&info)))
}

async fn view_file(
    State(state): State<AppState>,
    Path(token): Path<Uuid>,
) -> Result<Response, StatusCode> {
    let info = state
        .notify_deliveries
        .get_for_view(token)
        .await
        .map_err(|e| {
            tracing::error!("get_for_view: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    check_not_expired(info.expire_at, chrono::Utc::now())?;

    let storage = state.notify_storage.as_ref().ok_or_else(|| {
        tracing::error!("notify_storage not configured");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let bytes = storage.download(&info.r2_key).await.map_err(|e| {
        tracing::error!("notify_storage.download: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let mut headers = HeaderMap::new();
    // Content-Type は **実際の R2 key** で決める。redacted は `.jpg` (PR #327 以降)
    // で、原本ファイル名 (info.file_name) は `.pdf` のままなので、原本名で判定
    // すると client が PDF.js でデコード試行して "Invalid PDF structure" になる。
    let content_type = guess_content_type(Some(&info.r2_key));
    if let Ok(v) = content_type.parse() {
        headers.insert(header::CONTENT_TYPE, v);
    }
    // download 時のファイル名は原本ファイル名 (UX の都合で .pdf を維持)。
    let cd = build_inline_disposition(info.file_name.as_deref());
    if let Ok(v) = cd.parse() {
        headers.insert(header::CONTENT_DISPOSITION, v);
    }

    Ok((StatusCode::OK, headers, bytes).into_response())
}

/// `/api/notify/v/{token}/image` — `image/jpeg` を返す。
///
/// `lookup_delivery_for_view` の `r2_key` は `COALESCE(redacted_r2_key, r2_key)`。
/// redacted_r2_key が set されていれば既に `.jpg` (黒塗り済 JPEG) なのでそのまま返す。
/// 原本だけの場合は `.pdf` なので pdfium で 1 ページ目を rasterize して JPEG 化。
async fn view_image(
    State(state): State<AppState>,
    Path(token): Path<Uuid>,
) -> Result<Response, StatusCode> {
    let info = state
        .notify_deliveries
        .get_for_view(token)
        .await
        .map_err(|e| {
            tracing::error!("view_image: get_for_view: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    check_not_expired(info.expire_at, chrono::Utc::now())?;

    let storage = state.notify_storage.as_ref().ok_or_else(|| {
        tracing::error!("view_image: notify_storage not configured");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let bytes = storage.download(&info.r2_key).await.map_err(|e| {
        tracing::error!("view_image: notify_storage.download: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // r2_key の拡張子で分岐:
    //  - .jpg / .jpeg → 既に redacted JPEG (apply_redactions の出力) → そのまま返す
    //  - その他 (.pdf 等) → pdfium で page 1 を rasterize → JPEG
    let jpeg_bytes = if info.r2_key.ends_with(".jpg") || info.r2_key.ends_with(".jpeg") {
        bytes
    } else {
        crate::redact::rasterize_first_page_jpeg(&bytes).map_err(|e| match &e {
            crate::redact::RedactError::PageNoImage(_) => {
                tracing::info!("view_image: pdf has no renderable page, returning 415");
                StatusCode::UNSUPPORTED_MEDIA_TYPE
            }
            _ => {
                tracing::error!("view_image: rasterize_first_page_jpeg: {e}");
                StatusCode::INTERNAL_SERVER_ERROR
            }
        })?
    };

    let mut headers = HeaderMap::new();
    if let Ok(v) = "image/jpeg".parse() {
        headers.insert(header::CONTENT_TYPE, v);
    }
    if let Ok(v) = "inline".parse() {
        headers.insert(header::CONTENT_DISPOSITION, v);
    }
    Ok((StatusCode::OK, headers, jpeg_bytes).into_response())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_info(expire_in_hours: i64) -> DeliveryViewInfo {
        DeliveryViewInfo {
            document_id: Uuid::new_v4(),
            tenant_id: Uuid::new_v4(),
            r2_key: "tenant/email/msg/file.pdf".into(),
            file_name: Some("file.pdf".into()),
            file_size_bytes: Some(2048),
            source_subject: Some("件名".into()),
            source_sender: Some("from@example.com".into()),
            source_received_at: Some(chrono::Utc::now()),
            expire_at: chrono::Utc::now() + chrono::Duration::hours(expire_in_hours),
        }
    }

    #[test]
    fn build_metadata_copies_all_fields() {
        let info = sample_info(24);
        let m = build_metadata(&info);
        assert_eq!(m.file_name, info.file_name);
        assert_eq!(m.file_size_bytes, info.file_size_bytes);
        assert_eq!(m.source_subject, info.source_subject);
        assert_eq!(m.source_sender, info.source_sender);
        assert_eq!(m.source_received_at, info.source_received_at);
        assert_eq!(m.expire_at, info.expire_at);
        let json = serde_json::to_string(&m).unwrap();
        assert!(!json.contains("r2_key"));
        assert!(!json.contains("document_id"));
        assert!(!json.contains("tenant_id"));
    }

    #[test]
    fn check_not_expired_ok_when_future() {
        let now = chrono::Utc::now();
        let expire = now + chrono::Duration::hours(1);
        assert!(check_not_expired(expire, now).is_ok());
    }

    #[test]
    fn check_not_expired_returns_gone_at_boundary() {
        let now = chrono::Utc::now();
        let err = check_not_expired(now, now).unwrap_err();
        assert_eq!(err, StatusCode::GONE);
    }

    #[test]
    fn check_not_expired_returns_gone_when_past() {
        let now = chrono::Utc::now();
        let expire = now - chrono::Duration::seconds(1);
        let err = check_not_expired(expire, now).unwrap_err();
        assert_eq!(err, StatusCode::GONE);
    }

    #[test]
    fn guess_content_type_pdf() {
        assert_eq!(guess_content_type(Some("a.pdf")), "application/pdf");
        assert_eq!(guess_content_type(Some("A.PDF")), "application/pdf");
    }

    #[test]
    fn guess_content_type_images() {
        assert_eq!(guess_content_type(Some("a.png")), "image/png");
        assert_eq!(guess_content_type(Some("a.jpg")), "image/jpeg");
        assert_eq!(guess_content_type(Some("a.jpeg")), "image/jpeg");
        assert_eq!(guess_content_type(Some("a.gif")), "image/gif");
        assert_eq!(guess_content_type(Some("a.webp")), "image/webp");
        assert_eq!(guess_content_type(Some("a.svg")), "image/svg+xml");
    }

    #[test]
    fn guess_content_type_text() {
        assert_eq!(
            guess_content_type(Some("note.txt")),
            "text/plain; charset=utf-8"
        );
    }

    #[test]
    fn guess_content_type_unknown_falls_back_to_pdf() {
        assert_eq!(guess_content_type(Some("a.xlsx")), "application/pdf");
        assert_eq!(guess_content_type(Some("noext")), "application/pdf");
        assert_eq!(guess_content_type(None), "application/pdf");
    }

    #[test]
    fn build_inline_disposition_basic() {
        let cd = build_inline_disposition(Some("hello.pdf"));
        assert!(cd.starts_with("inline; "));
        assert!(cd.contains("filename=\"hello.pdf\""));
        assert!(cd.contains("filename*=UTF-8''hello.pdf"));
    }

    #[test]
    fn build_inline_disposition_utf8() {
        let cd = build_inline_disposition(Some("点呼.pdf"));
        assert!(cd.starts_with("inline; "));
        // RFC 5987 形式で URL エンコードされる
        assert!(cd.contains("filename*=UTF-8''"));
        assert!(cd.contains("%E7%82%B9%E5%91%BC.pdf"));
    }

    #[test]
    fn build_inline_disposition_quote_escape() {
        let cd = build_inline_disposition(Some("a\"b.pdf"));
        // inline 内のダブルクォートは _ に置換される
        assert!(cd.contains("filename=\"a_b.pdf\""));
    }

    #[test]
    fn build_inline_disposition_default_name() {
        let cd = build_inline_disposition(None);
        assert!(cd.contains("filename=\"attachment\""));
    }
}
