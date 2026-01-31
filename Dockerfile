FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    libcap2-bin \
    imagemagick \
    libheif-examples \
    && rm -rf /var/lib/apt/lists/*

RUN useradd -r -s /bin/false kgd

WORKDIR /app

# Copy cross-compiled binary
COPY target/aarch64-unknown-linux-gnu/release/kgd /app/kgd

RUN setcap cap_net_raw+ep /app/kgd

USER kgd

ENTRYPOINT ["/app/kgd"]
