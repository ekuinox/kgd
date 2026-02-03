# heif-sys

libheif の Rust バインディング (FFI)

## ビルド要件

### ビルドツール

- CMake
- C++ コンパイラ (GCC, Clang など)
- libclang (bindgen 用)
- Ninja (オプション、あれば使用される)

### 依存ライブラリ

以下のライブラリの開発パッケージが必要です:

| ライブラリ | 用途 |
|-----------|------|
| libde265 | HEVC デコーダ |
| x265 | HEVC エンコーダ |
| libaom | AV1 コーデック (AVIF サポート) |
| libsharpyuv | YUV 変換 (libwebp の一部) |
| zlib | 圧縮 |
| libjpeg | JPEG エンコード/デコード |

### Ubuntu / Debian

```bash
sudo apt-get install \
    cmake \
    libclang-dev \
    libde265-dev \
    libx265-dev \
    libaom-dev \
    libwebp-dev \
    zlib1g-dev \
    libjpeg-dev
```

### Fedora

```bash
sudo dnf install \
    cmake \
    clang-devel \
    libde265-devel \
    x265-devel \
    libaom-devel \
    libwebp-devel \
    zlib-devel \
    libjpeg-turbo-devel
```

### macOS (Homebrew)

```bash
brew install \
    cmake \
    llvm \
    libde265 \
    x265 \
    aom \
    webp \
    jpeg
```
