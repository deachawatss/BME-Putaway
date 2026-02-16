# ============================================================
# Putaway Backend - Rust Dockerfile
# Multi-stage build for optimized production image
# ============================================================

# Stage 1: Builder
FROM rust:1.88-slim-bookworm AS builder

WORKDIR /app

# Install dependencies for SQL Server connectivity
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Copy Cargo files first for better caching
COPY Cargo.toml Cargo.lock ./

# Create dummy main.rs to cache dependencies
RUN mkdir -p src && \
    echo "fn main() {}" > src/main.rs && \
    cargo build --release && \
    rm -rf src

# Copy actual source code
COPY src ./src

# Build release binary (touch main.rs to force rebuild)
RUN touch src/main.rs && \
    cargo build --release

# Stage 2: Runtime
FROM debian:bookworm-slim AS runtime

WORKDIR /app

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    curl \
    && rm -rf /var/lib/apt/lists/* \
    && apt-get clean

# Create non-root user for security
RUN groupadd -r appuser && useradd -r -g appuser appuser

# Copy binary from builder
COPY --from=builder /app/target/release/putaway-backend /app/putaway-backend

# Create directory for frontend assets
RUN mkdir -p /app/frontend/dist/frontend/browser && \
    chown -R appuser:appuser /app

# Switch to non-root user
USER appuser

# Expose port (matches SERVER_PORT in .env)
EXPOSE 4400

# Health check
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:4400/api/health || exit 1

# Run the binary
CMD ["./putaway-backend"]
