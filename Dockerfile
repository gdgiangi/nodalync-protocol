# Build stage
FROM rust:1.85-bookworm AS builder

WORKDIR /app

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    protobuf-compiler \
    && rm -rf /var/lib/apt/lists/*

# Copy only Cargo files first for dependency caching
COPY Cargo.toml Cargo.lock ./
COPY crates/protocol/nodalync-crypto/Cargo.toml crates/protocol/nodalync-crypto/Cargo.toml
COPY crates/protocol/nodalync-types/Cargo.toml crates/protocol/nodalync-types/Cargo.toml
COPY crates/protocol/nodalync-wire/Cargo.toml crates/protocol/nodalync-wire/Cargo.toml
COPY crates/protocol/nodalync-store/Cargo.toml crates/protocol/nodalync-store/Cargo.toml
COPY crates/protocol/nodalync-valid/Cargo.toml crates/protocol/nodalync-valid/Cargo.toml
COPY crates/protocol/nodalync-econ/Cargo.toml crates/protocol/nodalync-econ/Cargo.toml
COPY crates/protocol/nodalync-net/Cargo.toml crates/protocol/nodalync-net/Cargo.toml
COPY crates/protocol/nodalync-ops/Cargo.toml crates/protocol/nodalync-ops/Cargo.toml
COPY crates/protocol/nodalync-settle/Cargo.toml crates/protocol/nodalync-settle/Cargo.toml
COPY crates/apps/nodalync-mcp/Cargo.toml crates/apps/nodalync-mcp/Cargo.toml
COPY crates/apps/nodalync-cli/Cargo.toml crates/apps/nodalync-cli/Cargo.toml
COPY crates/nodalync/Cargo.toml crates/nodalync/Cargo.toml

# Create dummy source files to build dependencies
RUN mkdir -p crates/protocol/nodalync-crypto/src && echo "pub fn dummy() {}" > crates/protocol/nodalync-crypto/src/lib.rs && \
    mkdir -p crates/protocol/nodalync-types/src && echo "pub fn dummy() {}" > crates/protocol/nodalync-types/src/lib.rs && \
    mkdir -p crates/protocol/nodalync-wire/src && echo "pub fn dummy() {}" > crates/protocol/nodalync-wire/src/lib.rs && \
    mkdir -p crates/protocol/nodalync-store/src && echo "pub fn dummy() {}" > crates/protocol/nodalync-store/src/lib.rs && \
    mkdir -p crates/protocol/nodalync-valid/src && echo "pub fn dummy() {}" > crates/protocol/nodalync-valid/src/lib.rs && \
    mkdir -p crates/protocol/nodalync-econ/src && echo "pub fn dummy() {}" > crates/protocol/nodalync-econ/src/lib.rs && \
    mkdir -p crates/protocol/nodalync-net/src && echo "pub fn dummy() {}" > crates/protocol/nodalync-net/src/lib.rs && \
    mkdir -p crates/protocol/nodalync-ops/src && echo "pub fn dummy() {}" > crates/protocol/nodalync-ops/src/lib.rs && \
    mkdir -p crates/protocol/nodalync-settle/src && echo "pub fn dummy() {}" > crates/protocol/nodalync-settle/src/lib.rs && \
    mkdir -p crates/apps/nodalync-mcp/src && echo "pub fn dummy() {}" > crates/apps/nodalync-mcp/src/lib.rs && \
    mkdir -p crates/apps/nodalync-cli/src && echo "fn main() {}" > crates/apps/nodalync-cli/src/main.rs && \
    mkdir -p crates/nodalync/src && echo "pub fn dummy() {}" > crates/nodalync/src/lib.rs

# Build dependencies only (this layer is cached)
RUN cargo build --release --features hedera-sdk -p nodalync-cli 2>/dev/null || true

# Now copy actual source code
COPY crates ./crates

# Touch all source files to ensure cargo detects changes from dummy builds.
# This is necessary because Docker layer caching can preserve timestamps that
# make cargo think the dummy lib.rs files are still valid.
RUN find crates -name "*.rs" -exec touch {} +

# Build release binary with Hedera SDK support
RUN cargo build --release --features hedera-sdk -p nodalync-cli

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
