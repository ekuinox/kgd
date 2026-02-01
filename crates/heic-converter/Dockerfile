FROM debian:bookworm-slim AS magick-builder

ARG IMAGEMAGICK_VERSION=7.1.2-13

RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential git pkg-config autoconf libtool ca-certificates \
    libheif-dev libde265-dev libx265-dev x265 zlib1g-dev \
    libjpeg62-turbo-dev libpng-dev libtiff-dev libwebp-dev \
    libfreetype6-dev liblcms2-dev libxml2-dev libfontconfig1-dev \
    && rm -rf /var/lib/apt/lists/*

RUN git clone -b ${IMAGEMAGICK_VERSION} --depth 1 \
      https://github.com/ImageMagick/ImageMagick.git /tmp/ImageMagick && \
    cd /tmp/ImageMagick && \
    ./configure \
      --with-heic=yes \
      --with-jpeg=yes \
      --with-png=yes \
      --with-tiff=yes \
      --with-webp=yes \
      --without-magick-plus-plus \
      --disable-docs \
      --disable-static && \
    make -j$(nproc) && \
    make install && \
    ldconfig /usr/local/lib && \
    rm -rf /tmp/ImageMagick

# HEIC delegate が有効であることを検証
RUN magick --version && magick --version | grep -q 'heic'

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    libheif1 libheif-plugin-x265 libde265-0 libaom3 libdav1d6 libx265-199 \
    libjpeg62-turbo libpng16-16 libtiff6 libwebp7 libsharpyuv0 \
    libfreetype6 liblcms2-2 libxml2 libfontconfig1 \
    libgomp1 zlib1g \
    # heif-enc is needed to generate test HEIC files
    libheif-examples \
    && rm -rf /var/lib/apt/lists/*

COPY --from=magick-builder /usr/local/bin/magick /usr/local/bin/magick
COPY --from=magick-builder /usr/local/lib/ /usr/local/lib/
COPY --from=magick-builder /usr/local/etc/ImageMagick-7/ /usr/local/etc/ImageMagick-7/
COPY --from=magick-builder /usr/local/share/ImageMagick-7/ /usr/local/share/ImageMagick-7/

RUN ldconfig /usr/local/lib

WORKDIR /work

# Generate test HEIC file using magick (create JPEG) + heif-enc (convert to HEIC)
RUN mkdir -p /work/input /work/output \
    && magick -size 1x1 xc:red /work/input/test.jpg \
    && heif-enc /work/input/test.jpg -o /work/input/test.heic \
    && rm /work/input/test.jpg

# Cross-compiled example binary
COPY target/aarch64-unknown-linux-gnu/release/examples/convert_dir /work/convert_dir

CMD ["/work/convert_dir", "/work/input", "/work/output"]
