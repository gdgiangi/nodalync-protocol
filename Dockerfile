# Build stage
FROM rust:1.85-bookworm AS builder

WORKDIR /app

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Copy workspace files
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates

# Build release binary
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
