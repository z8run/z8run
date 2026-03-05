# ============================================================
# z8run – Multi-stage Docker build
# Stage 1: Build Rust backend
# Stage 2: Build React frontend
# Stage 3: Minimal runtime image
# ============================================================

# ── Stage 1: Rust build ─────────────────────────────────────
FROM rust:1.83-bookworm AS backend-builder

WORKDIR /build

# Cache dependencies first
COPY Cargo.toml Cargo.lock ./
COPY crates/z8run-core/Cargo.toml       crates/z8run-core/Cargo.toml
COPY crates/z8run-api/Cargo.toml        crates/z8run-api/Cargo.toml
COPY crates/z8run-storage/Cargo.toml    crates/z8run-storage/Cargo.toml
COPY crates/z8run-protocol/Cargo.toml   crates/z8run-protocol/Cargo.toml
COPY crates/z8run-runtime/Cargo.toml    crates/z8run-runtime/Cargo.toml
COPY bins/z8run-cli/Cargo.toml          bins/z8run-cli/Cargo.toml
COPY bins/z8run-server/Cargo.toml       bins/z8run-server/Cargo.toml

# Create dummy src files so cargo can resolve deps
RUN mkdir -p crates/z8run-core/src      && echo "" > crates/z8run-core/src/lib.rs && \
    mkdir -p crates/z8run-api/src       && echo "" > crates/z8run-api/src/lib.rs && \
    mkdir -p crates/z8run-storage/src   && echo "" > crates/z8run-storage/src/lib.rs && \
    mkdir -p crates/z8run-protocol/src  && echo "" > crates/z8run-protocol/src/lib.rs && \
    mkdir -p crates/z8run-runtime/src   && echo "" > crates/z8run-runtime/src/lib.rs && \
    mkdir -p bins/z8run-cli/src         && echo "fn main(){}" > bins/z8run-cli/src/main.rs && \
    mkdir -p bins/z8run-server/src      && echo "fn main(){}" > bins/z8run-server/src/main.rs

RUN cargo build --release --bin z8run 2>/dev/null || true

# Now copy real source and build
COPY crates/ crates/
COPY bins/   bins/

# Touch main files to invalidate the dummy cache
RUN touch bins/z8run-cli/src/main.rs && \
    touch bins/z8run-server/src/main.rs

RUN cargo build --release --bin z8run

# ── Stage 2: Frontend build ─────────────────────────────────
FROM node:22-bookworm-slim AS frontend-builder

WORKDIR /build/frontend

COPY frontend/package.json frontend/package-lock.json* ./
RUN npm ci

COPY frontend/ .
RUN npm run build

# ── Stage 3: Runtime ────────────────────────────────────────
FROM debian:bookworm-slim AS runtime

RUN apt-get update && \
    apt-get install -y --no-install-recommends \
        ca-certificates \
        libssl3 \
        curl \
    && rm -rf /var/lib/apt/lists/*

# Non-root user
RUN groupadd -r z8run && useradd -r -g z8run -m z8run

WORKDIR /app

# Copy binary
COPY --from=backend-builder /build/target/release/z8run /app/z8run

# Copy frontend build
COPY --from=frontend-builder /build/frontend/dist /app/frontend/dist

# Data directory
RUN mkdir -p /app/data /app/data/plugins && chown -R z8run:z8run /app

USER z8run

# Defaults
ENV Z8_PORT=7700 \
    Z8_BIND=0.0.0.0 \
    Z8_DATA_DIR=/app/data \
    Z8_LOG_LEVEL=info

EXPOSE 7700

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD curl -f http://localhost:7700/api/v1/flows || exit 1

ENTRYPOINT ["/app/z8run"]
CMD ["serve"]
