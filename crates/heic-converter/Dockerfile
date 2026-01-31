FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    libheif-examples \
    imagemagick \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /work

# テスト用 HEIC ファイルを生成
# 1. ImageMagick で 1x1 のダミー JPEG を作成
# 2. heif-enc で JPEG → HEIC に変換
RUN mkdir -p /work/input /work/output \
    && convert -size 1x1 xc:red /work/input/test.jpg \
    && heif-enc /work/input/test.jpg -o /work/input/test.heic \
    && rm /work/input/test.jpg

# Cross-compiled example binary
COPY target/aarch64-unknown-linux-gnu/release/examples/convert_dir /work/convert_dir

CMD ["/work/convert_dir", "/work/input", "/work/output"]
