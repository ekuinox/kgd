# ========================================
# Stage 1: Chef base (install cargo-chef)
# ========================================
FROM rust:bookworm AS chef

RUN cargo install cargo-chef

WORKDIR /app

# ========================================
# Stage 2: Planner (generate recipe.json)
# ========================================
FROM chef AS planner

COPY . .

RUN cargo chef prepare --recipe-path recipe.json

# ========================================
# Stage 3: Builder (build dependencies and app)
# ========================================
FROM chef AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    git \
    cmake \
    ninja-build \
    libclang-dev \
    libde265-dev \
    libx265-dev \
    libaom-dev \
    libwebp-dev \
    zlib1g-dev \
    libjpeg-dev \
    && rm -rf /var/lib/apt/lists/*

# Copy recipe and build dependencies only (cached layer)
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json

# Copy source code and build application
COPY . .
RUN cargo build --release --bin kgd

# ========================================
# Stage 4: Runtime base
# ========================================
FROM debian:bookworm-slim AS runtime-base

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    libcap2-bin \
    # libheif の実行時依存ライブラリ
    libde265-0 \
    libx265-199 \
    libaom3 \
    libwebp7 \
    zlib1g \
    libjpeg62-turbo \
    && rm -rf /var/lib/apt/lists/*

RUN useradd -r -s /bin/false kgd

WORKDIR /app

# ========================================
# Stage 5: Local development target (--target local)
# ========================================
FROM runtime-base AS local

COPY --from=builder /app/target/release/kgd /app/kgd

RUN setcap cap_net_raw+ep /app/kgd

USER kgd

ENTRYPOINT ["/app/kgd"]

# ========================================
# Stage 6: CI target (--target ci)
# ========================================
FROM runtime-base AS ci

COPY --from=builder /app/target/release/kgd /app/kgd

RUN setcap cap_net_raw+ep /app/kgd

USER kgd

ENTRYPOINT ["/app/kgd"]
