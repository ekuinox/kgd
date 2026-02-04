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

# ローカル開発用ランタイムベース
# builder ステージ (rust:bookworm/Debian 12) と libjpeg のバージョンを合わせるため Debian を使用
FROM debian:bookworm-slim AS runtime-base-local

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

# CI/本番用ランタイムベース
# cross-rs のビルド環境 (Ubuntu 22.04) と libjpeg のバージョンを合わせるため Ubuntu を使用
FROM ubuntu:22.04 AS runtime-base-ci

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    libcap2-bin \
    # libheif の実行時依存ライブラリ
    libde265-0 \
    libx265-199 \
    libaom3 \
    libwebp7 \
    zlib1g \
    libjpeg-turbo8 \
    && rm -rf /var/lib/apt/lists/*

RUN useradd -r -s /bin/false kgd

WORKDIR /app

# ローカル開発用 (--target local)
# builder ステージでビルドしたバイナリを使用する
FROM runtime-base-local AS local

COPY --from=builder /app/target/release/kgd /app/kgd

RUN setcap cap_net_raw+ep /app/kgd

USER kgd

ENTRYPOINT ["/app/kgd"]

# CI/本番用 (デフォルトターゲット)
# cross-rs でビルド済みのバイナリをホストからコピーする
FROM runtime-base-ci

ARG CROSS_TARGET=aarch64-unknown-linux-gnu
COPY target/${CROSS_TARGET}/release/kgd /app/kgd

RUN setcap cap_net_raw+ep /app/kgd

USER kgd

ENTRYPOINT ["/app/kgd"]
