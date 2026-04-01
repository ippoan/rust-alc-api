FROM rust:latest AS builder

WORKDIR /app
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*

# Copy workspace manifests for dependency caching
COPY Cargo.toml Cargo.lock ./
COPY crates/alc-core/Cargo.toml crates/alc-core/Cargo.toml
COPY crates/alc-carins/Cargo.toml crates/alc-carins/Cargo.toml
COPY crates/alc-compare/Cargo.toml crates/alc-compare/Cargo.toml
COPY crates/alc-csv-parser/Cargo.toml crates/alc-csv-parser/Cargo.toml
COPY crates/alc-devices/Cargo.toml crates/alc-devices/Cargo.toml
COPY crates/alc-dtako/Cargo.toml crates/alc-dtako/Cargo.toml
COPY crates/alc-misc/Cargo.toml crates/alc-misc/Cargo.toml
COPY crates/alc-pdf/Cargo.toml crates/alc-pdf/Cargo.toml
COPY crates/alc-storage/Cargo.toml crates/alc-storage/Cargo.toml
COPY crates/alc-tenko/Cargo.toml crates/alc-tenko/Cargo.toml

# Create dummy sources for dependency caching
RUN mkdir -p src src/bin \
    && echo "fn main() {}" > src/main.rs \
    && echo "fn main() {}" > src/bin/migrate.rs \
    && for d in crates/*/; do mkdir -p "$d/src" && echo "" > "$d/src/lib.rs"; done \
    && cargo build --release \
    && rm -rf src crates/*/src

# Copy real sources and build
COPY src ./src
COPY crates ./crates
COPY migrations ./migrations
COPY assets ./assets
RUN find src crates -name '*.rs' -exec touch {} + && cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/rust-alc-api /usr/local/bin/
COPY --from=builder /app/target/release/migrate /usr/local/bin/
COPY --from=builder /app/migrations /app/migrations

WORKDIR /app
ENV PORT=8080
EXPOSE 8080

CMD ["rust-alc-api"]
