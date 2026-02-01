//! HEIC/HEIF 画像を JPEG に変換するライブラリ。
//!
//! Unix 環境では libheif-rs (embedded-libheif) を使用して変換を行う。
//! Windows 環境では変換機能は無効化される。

use anyhow::Result;

/// HEIC データを JPEG に変換する。
///
/// Unix 環境では libheif-rs と image クレートを使用して変換を行う。
/// Windows 環境ではエラーを返す。
#[cfg(unix)]
pub fn convert_heic_to_jpeg(heic_data: &[u8]) -> Result<Vec<u8>> {
    use std::io::Cursor;

    use anyhow::Context as _;
    use image::ImageReader;

    // libheif-rs のデコーダーフックを登録
    libheif_rs::integration::image::register_all_decoding_hooks();

    // HEIC データを読み込んでデコード
    let reader = ImageReader::new(Cursor::new(heic_data))
        .with_guessed_format()
        .context("Failed to create image reader")?;

    let img = reader.decode().context("Failed to decode HEIC image")?;

    // JPEG にエンコード
    let mut jpeg_data = Vec::new();
    img.write_to(&mut Cursor::new(&mut jpeg_data), image::ImageFormat::Jpeg)
        .context("Failed to encode image as JPEG")?;

    tracing::debug!(
        input_size = heic_data.len(),
        output_size = jpeg_data.len(),
        "HEIC to JPEG conversion succeeded"
    );

    Ok(jpeg_data)
}

/// Windows 環境では HEIC 変換はサポートされていない。
#[cfg(not(unix))]
pub fn convert_heic_to_jpeg(_heic_data: &[u8]) -> Result<Vec<u8>> {
    anyhow::bail!("HEIC to JPEG conversion is not supported on non-Unix platforms")
}
