//! Rasterize a PDF to JPEG (page 1 only) using pdfium-render — for sending to Gemini.
use pdfium_render::prelude::*;

fn main() {
    let home = std::env::var("HOME").unwrap();
    let input = format!("{home}/js/nuxt-notify/docs/reference/3164_001.pdf");
    let output = "/tmp/3164_rasterized.jpg".to_string();

    let pdfium = Pdfium::new(Pdfium::bind_to_system_library().unwrap());
    let pdf_bytes = std::fs::read(&input).unwrap();
    let doc = pdfium.load_pdf_from_byte_slice(&pdf_bytes, None).unwrap();
    let page = doc.pages().get(0).unwrap();
    let render_config = PdfRenderConfig::new().scale_page_by_factor(200.0 / 72.0);
    let bitmap = page.render_with_config(&render_config).unwrap();
    let img = bitmap.as_image().into_rgb8();
    let (w, h) = img.dimensions();
    eprintln!("rendered {w}x{h}");
    let mut jpeg = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 90)
        .encode(img.as_raw(), w, h, image::ExtendedColorType::Rgb8)
        .unwrap();
    std::fs::write(&output, &jpeg).unwrap();
    eprintln!("wrote {output} ({} bytes)", jpeg.len());
}
