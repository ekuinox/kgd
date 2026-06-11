//! HEIC/HEIF 画像変換の ImageConverter ポート実装。

use anyhow::Result;

use kgd_application::ports::ImageConverter;

/// libheif による [`ImageConverter`] 実装。
///
/// Unix 以外のプラットフォームでは変換をサポートせず、常にエラーを返す。
#[derive(Debug, Clone, Copy, Default)]
pub struct HeifConverter;

impl ImageConverter for HeifConverter {
    #[cfg(unix)]
    fn heic_to_jpeg(&self, data: &[u8]) -> Result<Vec<u8>> {
        Ok(heif::convert_heic_to_jpeg(data)?)
    }

    #[cfg(not(unix))]
    fn heic_to_jpeg(&self, _data: &[u8]) -> Result<Vec<u8>> {
        anyhow::bail!("HEIC to JPEG conversion is not supported on this platform")
    }
}
