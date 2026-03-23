# ── Build stage ───────────────────────────────────────────────────────────────
FROM rust:1.85-bookworm AS builder

WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates crates
COPY migrations migrations

RUN cargo build --release --bin sqz

# ── Runtime stage ─────────────────────────────────────────────────────────────
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /src/target/release/sqz /usr/local/bin/sqz
COPY rules/ /app/rules/
COPY concutter.example.toml /app/concutter.toml

RUN mkdir -p /data

WORKDIR /app
EXPOSE 8080

HEALTHCHECK --interval=10s --timeout=3s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:8080/health || exit 1

CMD ["sqz", "--config", "/app/concutter.toml", "--host", "0.0.0.0", "--db", "/data/concutter.db"]
