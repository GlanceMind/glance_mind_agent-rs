# =============================================================================
# GlanceMind Agent-rs Dockerfile (with USTC mirror)
# =============================================================================

FROM rust:latest AS builder

WORKDIR /app

# Set proxy for apt (use host.docker.internal for macOS)
ENV HTTP_PROXY=http://host.docker.internal:7890
ENV HTTPS_PROXY=http://host.docker.internal:7890
ENV http_proxy=http://host.docker.internal:7890
ENV https_proxy=http://host.docker.internal:7890

# Configure cargo to use USTC mirror
RUN mkdir -p /usr/local/cargo && \
    echo '[source.crates-io]' > /usr/local/cargo/config.toml && \
    echo 'replace-with = "ustc"' >> /usr/local/cargo/config.toml && \
    echo '' >> /usr/local/cargo/config.toml && \
    echo '[source.ustc]' >> /usr/local/cargo/config.toml && \
    echo 'registry = "sparse+https://mirrors.ustc.edu.cn/crates.io-index/"' >> /usr/local/cargo/config.toml && \
    echo '' >> /usr/local/cargo/config.toml && \
    echo '[net]' >> /usr/local/cargo/config.toml && \
    echo 'git-fetch-with-cli = true' >> /usr/local/cargo/config.toml

# Install build dependencies
RUN apt-get update && apt-get install -y libpq-dev && rm -rf /var/lib/apt/lists/*

# Copy source code
COPY Cargo.toml Cargo.lock ./
COPY src ./src

# Build the application
RUN cargo build --release --bin gm-agent

# =============================================================================
# Runtime Stage (no proxy needed)
# =============================================================================
FROM debian:bookworm-slim

WORKDIR /app

# Install runtime dependencies
RUN apt-get update && \
    apt-get install -y \
    libpq5 \
    ca-certificates \
    openssl \
    && rm -rf /var/lib/apt/lists/*

# Copy the compiled binary from builder
COPY --from=builder /app/target/release/gm-agent /usr/local/bin/gm-agent

# Set executable permission
RUN chmod +x /usr/local/bin/gm-agent

# Default environment variables
ENV RUST_LOG=info,glance_mind_agent_rs=debug
ENV DATABASE_URL=""
ENV REDIS_URL=""
ENV AGENT_QUEUE_NAME=crawler:task_queue

# Run the agent
CMD ["gm-agent", "--workers", "1"]
