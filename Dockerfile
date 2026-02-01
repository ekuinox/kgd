# ローカル開発用ビルドステージ
# docker build --target local で使用する
FROM rust:bookworm AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    git \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY . .

RUN cargo build --release --bin kgd

# 共通ランタイムベース
FROM debian:bookworm-slim AS runtime-base

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    libcap2-bin \
    # HEIC→JPEG 変換に heif-convert (libheif-examples) を使用する。
    # ImageMagick 7 をソースビルドすれば magick コマンドでも HEIC を扱えるが、
    # CI で QEMU ARM64 エミュレーション上のコンパイルに 40 分以上かかるため断念した。
    # Docker ビルドキャッシュや事前ビルド済みバイナリの配布で解決できる可能性はある。
    libheif-examples \
    && rm -rf /var/lib/apt/lists/*

# heif-convert が利用可能であることを検証
RUN heif-convert --version

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
