# Stage 1: build
FROM rust:1.82-bookworm AS builder
WORKDIR /app

# Cache dependencies by copying manifests first
COPY Cargo.toml ./
RUN mkdir src && echo "fn main() {}" > src/main.rs && \
    cargo build --release 2>/dev/null; rm -rf src

# Build with real source
COPY src ./src
RUN touch src/main.rs && cargo build --release

# Stage 2: minimal runtime
FROM debian:bookworm-slim
RUN apt-get update && \
    apt-get install -y --no-install-recommends ca-certificates libssl3 && \
    rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=builder /app/target/release/polymarket-arbitrage-bot ./polymarket-arbitrage-bot

ENTRYPOINT ["./polymarket-arbitrage-bot", "--config", "/app/config/config.json"]
