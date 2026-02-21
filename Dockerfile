# CraftNet Node Docker Image
# Multi-stage build for minimal image size

# Build stage
FROM rust:1.75-bookworm AS builder

WORKDIR /app

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Copy workspace files
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/
COPY apps/ apps/

# Build release binaries
RUN cargo build --release -p craftnet-node -p craftnet-cli

# Runtime stage
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN useradd -m -u 1000 craftnet

# Copy binaries from builder
COPY --from=builder /app/target/release/craftnet-node /usr/local/bin/
COPY --from=builder /app/target/release/craftnet /usr/local/bin/

# Create data directory
RUN mkdir -p /data && chown craftnet:craftnet /data

USER craftnet
WORKDIR /data

# Default port for P2P
EXPOSE 9000

# Health check
HEALTHCHECK --interval=30s --timeout=10s --start-period=5s --retries=3 \
    CMD pgrep craftnet-node || exit 1

# Default command: run as full node
ENTRYPOINT ["craftnet-node"]
CMD ["--keyfile", "/data/node.key", "-l", "/ip4/0.0.0.0/tcp/9000", "full"]
