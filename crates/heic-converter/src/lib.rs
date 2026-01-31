//! HEIC/HEIF 画像を JPEG に変換するライブラリ。
//!
//! 外部コマンド (`heif-convert`, `magick`, `convert`) を利用して変換を行う。

use std::path::Path;
use std::process::{Command, Output};

use anyhow::{Context as _, Result};

/// HEIC データを JPEG に変換する。
///
/// 変換ツールを優先順位に従って試行する:
/// 1. `heif-convert` (libheif-examples) - HEIC 専用の変換ツール
/// 2. `magick` (ImageMagick v7) - 汎用画像変換
/// 3. `convert` (ImageMagick v6) - レガシー ImageMagick
pub fn convert_heic_to_jpeg(heic_data: &[u8]) -> Result<Vec<u8>> {
    use std::io::Write;

    // 一時ディレクトリを作成して一時ファイルの衝突を回避
    let tmp_dir = tempfile::tempdir().context("Failed to create temp directory")?;
    let input_path = tmp_dir.path().join("input.heic");
    let output_path = tmp_dir.path().join("output.jpg");

    std::fs::File::create(&input_path)
        .and_then(|mut f| f.write_all(heic_data))
        .context("Failed to write HEIC data to temp file")?;

    // heif-convert -> magick -> convert の順に試行
    let (tool_name, output) = try_heif_convert(&input_path, &output_path)
        .map(|o| ("heif-convert", o))
        .or_else(|_| try_imagemagick_v7(&input_path, &output_path).map(|o| ("magick", o)))
        .or_else(|_| try_imagemagick_v6(&input_path, &output_path).map(|o| ("convert", o)))
        .context("No HEIC conversion tool available. Install libheif-examples or ImageMagick.")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("HEIC to JPEG conversion failed ({}): {}", tool_name, stderr);
    }

    tracing::debug!(tool = tool_name, "HEIC to JPEG conversion succeeded");

    let jpeg_data =
        std::fs::read(&output_path).context("Failed to read converted JPEG from temp file")?;

    // tmp_dir の drop で一時ファイルは自動削除される
    Ok(jpeg_data)
}

/// `heif-convert` (libheif-examples) による変換を試行する。
fn try_heif_convert(input_path: &Path, output_path: &Path) -> std::io::Result<Output> {
    Command::new("heif-convert")
        .arg(input_path)
        .arg(output_path)
        .output()
}

/// ImageMagick v7 (`magick`) による変換を試行する。
fn try_imagemagick_v7(input_path: &Path, output_path: &Path) -> std::io::Result<Output> {
    Command::new("magick")
        .arg(input_path)
        .arg(output_path)
        .output()
}

/// ImageMagick v6 (`convert`) による変換を試行する。
fn try_imagemagick_v6(input_path: &Path, output_path: &Path) -> std::io::Result<Output> {
    Command::new("convert")
        .arg(input_path)
        .arg(output_path)
        .output()
}
