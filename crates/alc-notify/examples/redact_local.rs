//! Full local pipeline: PDF → rasterize → Gemini bbox → mask → JPEG.
//!
//! ```bash
//! GEMINI_API_KEY=$(grep ^GEMINI_API_KEY= ~/rust/rust-alc-api/.env | cut -d= -f2) \
//!   cargo run -p alc-notify --example redact_local
//! # 出力: ~/js/nuxt-notify/docs/redacted/3164_001_redacted_local.jpg
//! ```
use alc_notify::redact::{apply_redactions, detect_amount_boxes};

#[allow(unused_imports)]
use alc_notify::redact::detect_amount_boxes_v2;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter("alc_notify=info")
        .init();

    let api_key = std::env::var("GEMINI_API_KEY").expect("GEMINI_API_KEY env var");

    let home = std::env::var("HOME").unwrap();
    let input = format!("{home}/js/nuxt-notify/docs/reference/3164_001.pdf");
    let output = format!("{home}/js/nuxt-notify/docs/redacted/3164_001_redacted_local.jpg");

    let pdf_bytes = std::fs::read(&input).unwrap_or_else(|e| panic!("read {input}: {e}"));
    eprintln!("input : {input} ({} bytes)", pdf_bytes.len());

    let redactions = detect_amount_boxes(&pdf_bytes, &api_key, None, None)
        .await
        .expect("detect_amount_boxes");
    eprintln!("Gemini returned {} redaction(s):", redactions.len());
    for r in &redactions {
        eprintln!("  - {:?} bbox={:?}", r.text, r.box_2d);
    }

    let jpeg = apply_redactions(&pdf_bytes, &redactions).expect("apply_redactions");
    eprintln!("output: {output} ({} bytes)", jpeg.len());

    std::fs::write(&output, &jpeg).unwrap_or_else(|e| panic!("write {output}: {e}"));
}
