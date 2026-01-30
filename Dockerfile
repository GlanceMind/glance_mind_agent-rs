# syntax=docker/dockerfile:1.4

# ============================================================================
# Multi-stage Dockerfile for Rust Agent with optimized caching
# 
# Cache Strategy:
# 1. cargo-chef creates a "recipe" of dependencies
# 2. Dependencies are compiled in a separate layer (cached unless Cargo.* changes)
# 3. Application code is compiled in final layer
# ============================================================================

# ===== Chef Stage - Install cargo-chef =====
FROM rust:1.83-bookworm AS chef
RUN cargo install cargo-chef --locked
WORKDIR /app

# ===== Planner Stage - Analyze dependencies =====
FROM chef AS planner
# Copy only files needed for dependency analysis
COPY Cargo.toml Cargo.lock ./
# Create dummy source files for cargo to analyze
RUN mkdir -p src/bin && \
    echo "fn main() {}" > src/bin/main.rs && \
    echo "// dummy" > src/lib.rs
# Generate recipe.json
RUN cargo chef prepare --recipe-path recipe.json

# ===== Dependencies Stage - Build only dependencies =====
FROM chef AS deps
# Copy recipe from planner
COPY --from=planner /app/recipe.json recipe.json
# Build dependencies only - this layer is cached unless Cargo.toml/lock changes
RUN cargo chef cook --release --recipe-path recipe.json

# ===== Builder Stage - Build application =====
FROM deps AS builder
# Copy actual source code
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY tests ./tests
# Build the application (dependencies are already compiled)
RUN cargo build --release --bin gm-agent && \
    cp target/release/gm-agent /app/gm-agent && \
    strip /app/gm-agent

# ===== Runtime Stage - Minimal production image =====
FROM debian:bookworm-slim AS runtime

# Install runtime dependencies
RUN apt-get update && \
    apt-get install -y --no-install-recommends \
        libpq5 \
        ca-certificates \
        openssl \
        curl \
    && rm -rf /var/lib/apt/lists/* \
    && apt-get clean

# Create non-root user for security
RUN useradd -m -u 1000 appuser

# Copy binary from builder
COPY --from=builder /app/gm-agent /usr/local/bin/gm-agent

# Set ownership and permissions
RUN chown appuser:appuser /usr/local/bin/gm-agent

# Switch to non-root user
USER appuser

# Environment variables (can be overridden)
ENV RUST_LOG=info

# Health check - just verify binary runs
HEALTHCHECK --interval=30s --timeout=3s --start-period=10s --retries=3 \
    CMD gm-agent --help > /dev/null 2>&1 || exit 1

# Run application
CMD ["gm-agent"]
