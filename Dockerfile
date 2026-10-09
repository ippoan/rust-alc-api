FROM debian:trixie-slim

RUN apt-get update && apt-get install -y ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*

# PDFium (Chromium prebuilt) — used by alc-notify::redact for PDF rasterize.
# bblanchon/pdfium-binaries: linux-x64.tgz contains lib/libpdfium.so (~12 MB).
ARG PDFIUM_VERSION=chromium/7825
RUN curl -fsSL "https://github.com/bblanchon/pdfium-binaries/releases/download/${PDFIUM_VERSION}/pdfium-linux-x64.tgz" \
    | tar -xz -C /tmp \
    && cp /tmp/lib/libpdfium.so /usr/lib/libpdfium.so \
    && ldconfig \
    && rm -rf /tmp/lib /tmp/include /tmp/LICENSE /tmp/PDFiumConfig.cmake /tmp/VERSION 2>/dev/null || true

COPY rust-alc-api /usr/local/bin/
COPY migrate /usr/local/bin/
COPY archive /usr/local/bin/
COPY migrations /app/migrations

WORKDIR /app
ENV PORT=8080
EXPOSE 8080

CMD ["rust-alc-api"]
