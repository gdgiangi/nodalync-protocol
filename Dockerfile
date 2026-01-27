# Build stage
FROM rust:1.85-bookworm AS builder

WORKDIR /app

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Copy only Cargo files first for dependency caching
COPY Cargo.toml Cargo.lock ./
COPY crates/nodalync-crypto/Cargo.toml crates/nodalync-crypto/Cargo.toml
COPY crates/nodalync-types/Cargo.toml crates/nodalync-types/Cargo.toml
COPY crates/nodalync-wire/Cargo.toml crates/nodalync-wire/Cargo.toml
COPY crates/nodalync-store/Cargo.toml crates/nodalync-store/Cargo.toml
COPY crates/nodalync-valid/Cargo.toml crates/nodalync-valid/Cargo.toml
COPY crates/nodalync-econ/Cargo.toml crates/nodalync-econ/Cargo.toml
COPY crates/nodalync-net/Cargo.toml crates/nodalync-net/Cargo.toml
COPY crates/nodalync-ops/Cargo.toml crates/nodalync-ops/Cargo.toml
COPY crates/nodalync-settle/Cargo.toml crates/nodalync-settle/Cargo.toml
COPY crates/nodalync-mcp/Cargo.toml crates/nodalync-mcp/Cargo.toml
COPY crates/nodalync-cli/Cargo.toml crates/nodalync-cli/Cargo.toml

# Create dummy source files to build dependencies
RUN mkdir -p crates/nodalync-crypto/src && echo "pub fn dummy() {}" > crates/nodalync-crypto/src/lib.rs && \
    mkdir -p crates/nodalync-types/src && echo "pub fn dummy() {}" > crates/nodalync-types/src/lib.rs && \
    mkdir -p crates/nodalync-wire/src && echo "pub fn dummy() {}" > crates/nodalync-wire/src/lib.rs && \
    mkdir -p crates/nodalync-store/src && echo "pub fn dummy() {}" > crates/nodalync-store/src/lib.rs && \
    mkdir -p crates/nodalync-valid/src && echo "pub fn dummy() {}" > crates/nodalync-valid/src/lib.rs && \
    mkdir -p crates/nodalync-econ/src && echo "pub fn dummy() {}" > crates/nodalync-econ/src/lib.rs && \
    mkdir -p crates/nodalync-net/src && echo "pub fn dummy() {}" > crates/nodalync-net/src/lib.rs && \
    mkdir -p crates/nodalync-ops/src && echo "pub fn dummy() {}" > crates/nodalync-ops/src/lib.rs && \
    mkdir -p crates/nodalync-settle/src && echo "pub fn dummy() {}" > crates/nodalync-settle/src/lib.rs && \
    mkdir -p crates/nodalync-mcp/src && echo "pub fn dummy() {}" > crates/nodalync-mcp/src/lib.rs && \
    mkdir -p crates/nodalync-cli/src && echo "fn main() {}" > crates/nodalync-cli/src/main.rs

# Build dependencies only (this layer is cached)
RUN cargo build --release -p nodalync-cli 2>/dev/null || true

# Now copy actual source code
COPY crates ./crates

# Build release binary (only recompiles our code, not dependencies)
RUN cargo build --release -p nodalync-cli

# Runtime stage
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN useradd -m -u 1000 nodalync

# Copy binary from builder
COPY --from=builder /app/target/release/nodalync /usr/local/bin/nodalync

# Set ownership
RUN chown nodalync:nodalync /usr/local/bin/nodalync

# Switch to non-root user
USER nodalync

# Create data directory
RUN mkdir -p /home/nodalync/.nodalync

# Set environment
ENV NODALYNC_DATA_DIR=/home/nodalync/.nodalync
ENV RUST_LOG=nodalync=info

WORKDIR /home/nodalync

ENTRYPOINT ["nodalync"]
CMD ["--help"]
