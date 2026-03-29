# syntax=docker/dockerfile:1.4

FROM rust:1.88-bookworm AS chef
RUN cargo install cargo-chef --locked
WORKDIR /app

RUN mkdir -p /usr/local/cargo && cat > /usr/local/cargo/config.toml << 'CARGO_EOF'
[source.crates-io]
replace-with = "rsproxy-sparse"

[source.rsproxy-sparse]
registry = "sparse+https://rsproxy.cn/index/"

[registries.rsproxy]
index = "https://rsproxy.cn/crates.io-index"

[net]
git-fetch-with-cli = true
CARGO_EOF

FROM chef AS planner
COPY Cargo.toml Cargo.lock ./
RUN mkdir -p src/bin && \
    echo "fn main() {}" > src/bin/main.rs && \
    echo "// dummy" > src/lib.rs
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS deps
COPY --from=planner /app/recipe.json recipe.json
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/app/target \
    cargo chef cook --release --recipe-path recipe.json

FROM deps AS builder
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY tests ./tests
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/app/target \
    cargo build --release --bin gm-agent && \
    cp /app/target/release/gm-agent /app/gm-agent && \
    strip /app/gm-agent

FROM debian:bookworm-slim AS runtime
RUN apt-get update && \
    apt-get install -y --no-install-recommends \
        libpq5 ca-certificates openssl curl \
    && rm -rf /var/lib/apt/lists/* && apt-get clean
RUN useradd -m -u 1000 appuser
COPY --from=builder /app/gm-agent /usr/local/bin/gm-agent
RUN chown appuser:appuser /usr/local/bin/gm-agent
USER appuser
ENV RUST_LOG=info
HEALTHCHECK --interval=30s --timeout=3s --start-period=10s --retries=3 \
    CMD gm-agent --help > /dev/null 2>&1 || exit 1
CMD ["gm-agent"]
