# kgd 開発用 devcontainer イメージ
# heif-sys が libheif をソースからビルドするため、ビルド依存を一式入れる
FROM rust:bookworm

# システム依存ライブラリ
# - bindgen 用: libclang-dev
# - libheif ビルド/リンク用: cmake, ninja-build, libde265/x265/aom/webp/jpeg, zlib
# - reqwest(OpenSSL) 用: libssl-dev, pkg-config
# - DB 操作の利便性: postgresql-client
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
    postgresql-client \
    && rm -rf /var/lib/apt/lists/*

# rustfmt / clippy を確実に入れる
RUN rustup component add rustfmt clippy

# 開発ワークフローで使うツール
# (--locked で Cargo.lock 準拠の互換バージョンを使う)
RUN cargo install --locked just cargo-deny cargo-machete
