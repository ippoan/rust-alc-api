FROM debian:trixie-slim

ARG BINARY_NAME=rust-alc-api

RUN apt-get update && apt-get install -y ca-certificates postgresql-client curl \
    && rm -rf /var/lib/apt/lists/*

# PDFium (Chromium prebuilt) — used by alc-notify::redact for PDF rasterize.
# bblanchon/pdfium-binaries: linux-x64.tgz contains lib/libpdfium.so (~12 MB).
# Backend バイナリは alc-notify を含み libpdfium.so を dlopen する。
ARG PDFIUM_VERSION=chromium/7825
RUN curl -fsSL "https://github.com/bblanchon/pdfium-binaries/releases/download/${PDFIUM_VERSION}/pdfium-linux-x64.tgz" \
    | tar -xz -C /tmp \
    && cp /tmp/lib/libpdfium.so /usr/lib/libpdfium.so \
    && ldconfig \
    && rm -rf /tmp/lib /tmp/include /tmp/LICENSE /tmp/PDFiumConfig.cmake /tmp/VERSION 2>/dev/null || true

COPY ${BINARY_NAME} /usr/local/bin/
COPY migrate /usr/local/bin/
COPY migrations /app/migrations
COPY staging/entrypoint.sh /app/entrypoint.sh
RUN chmod +x /app/entrypoint.sh

ENV APP_BINARY=${BINARY_NAME}

WORKDIR /app
ENV PORT=8080
EXPOSE 8080

CMD ["/app/entrypoint.sh"]
