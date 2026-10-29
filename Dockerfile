FROM debian:trixie-slim

RUN apt-get update && apt-get install -y ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*

COPY rust-alc-api /usr/local/bin/
COPY migrate /usr/local/bin/
COPY archive /usr/local/bin/

WORKDIR /app
ENV PORT=8080
EXPOSE 8080

CMD ["rust-alc-api"]
