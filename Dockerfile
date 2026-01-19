# syntax=docker/dockerfile:1

# ===== Stage 1: Chef =====
FROM rust:1.85-bookworm AS chef
RUN cargo install cargo-chef
WORKDIR /app

# ===== Stage 2: Planner =====
FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# ===== Stage 3: Builder =====
FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json

# Build dependencies (cached if recipe.json unchanged)
RUN cargo chef cook --release --recipe-path recipe.json

# Build application
COPY . .
RUN cargo build --release --bin kgd-bot

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
COPY --from=builder /app/target/release/kgd-bot /app/kgd-bot

# Set capability for raw socket (WoL)
RUN setcap cap_net_raw+ep /app/kgd-bot

# Switch to non-root user
USER kgd

ENTRYPOINT ["/app/kgd-bot"]
