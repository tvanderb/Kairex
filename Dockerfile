# Stage 1: Build the Rust binary
FROM rust:latest AS builder

WORKDIR /build

# Cache dependency builds: copy manifests first, build deps, then copy source
COPY Cargo.toml Cargo.toml
COPY kairex/Cargo.toml kairex/Cargo.toml
COPY kairex-bin/Cargo.toml kairex-bin/Cargo.toml

# Create stub source files so cargo can resolve the workspace
RUN mkdir -p kairex/src && echo "" > kairex/src/lib.rs \
    && mkdir -p kairex-bin/src && echo "fn main() {}" > kairex-bin/src/main.rs

RUN cargo build --release 2>/dev/null || true

# Now copy real source and rebuild
COPY kairex/src kairex/src
COPY kairex-bin/src kairex-bin/src

RUN cargo build --release

# Stage 2: Runtime image
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    python3 \
    python3-pip \
    python3-venv \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Install Python dependencies
COPY scripts/requirements.txt /app/scripts/requirements.txt
RUN python3 -m venv /app/.venv \
    && /app/.venv/bin/pip install --no-cache-dir -r /app/scripts/requirements.txt

ENV PATH="/app/.venv/bin:$PATH"

# Copy the Rust binary
COPY --from=builder /build/target/release/kairex /app/kairex

# Volume mount points (populated at runtime via docker-compose)
# scripts/, config/, prompts/ are mounted read-only
# data/ is mounted read-write for SQLite

EXPOSE 9090

ENTRYPOINT ["/app/kairex"]
