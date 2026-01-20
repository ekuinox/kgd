# syntax=docker/dockerfile:1

# ===== Stage 1: Chef =====
FROM lukemathwalker/cargo-chef:latest-rust-1.92-bookworm AS chef
WORKDIR /app

# ===== Stage 2: Planner =====
FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# ===== Stage 3: Builder =====
FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json

# Build dependencies (cached if recipe.json unchanged)
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/app/target \
    cargo chef cook --release --recipe-path recipe.json

# Build application
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/app/target \
    cargo build --release --bin kgd && \
    cp /app/target/release/kgd /app/kgd-binary

# ===== Stage 4: Runtime =====
FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    libcap2-bin \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN useradd -r -s /bin/false kgd

WORKDIR /app

# Copy binary
COPY --from=builder /app/kgd-binary /app/kgd

# Set capability for raw socket (WoL)
RUN setcap cap_net_raw+ep /app/kgd

# Switch to non-root user
USER kgd

ENTRYPOINT ["/app/kgd"]
