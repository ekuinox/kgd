FROM debian:bookworm-slim

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

# Copy cross-compiled binary
COPY target/aarch64-unknown-linux-gnu/release/kgd /app/kgd

RUN setcap cap_net_raw+ep /app/kgd

USER kgd

ENTRYPOINT ["/app/kgd"]
