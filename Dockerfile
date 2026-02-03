# ローカル開発用ビルドステージ
# docker build --target local で使用する
FROM rust:bookworm AS builder

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

WORKDIR /app

COPY . .

RUN cargo build --release --bin kgd

# 共通ランタイムベース
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

# ローカル開発用 (--target local)
# builder ステージでビルドしたバイナリを使用する
FROM runtime-base AS local

COPY --from=builder /app/target/release/kgd /app/kgd

RUN setcap cap_net_raw+ep /app/kgd

USER kgd

ENTRYPOINT ["/app/kgd"]

# CI/本番用 (デフォルトターゲット)
# cross-rs でビルド済みのバイナリをホストからコピーする
FROM runtime-base

ARG CROSS_TARGET=aarch64-unknown-linux-gnu
COPY target/${CROSS_TARGET}/release/kgd /app/kgd

RUN setcap cap_net_raw+ep /app/kgd

USER kgd

ENTRYPOINT ["/app/kgd"]
