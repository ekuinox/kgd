//! HEIC から JPEG への変換テスト
//!
//! iPhone で撮影された実際の HEIC 画像を使用して変換をテストする。

#[cfg(unix)]
#[test]
fn test_convert_real_heic_to_jpeg() {
    use std::io::Write;

    let heic_data = include_bytes!("fixtures/sample.heic");

    let result = heic_converter::convert_heic_to_jpeg(heic_data);

    assert!(
        result.is_ok(),
        "Failed to convert HEIC to JPEG: {:?}",
        result.err()
    );

    let jpeg_data = result.unwrap();

    // JPEG データが生成されていることを確認
    assert!(!jpeg_data.is_empty(), "JPEG data should not be empty");

    // JPEG マジックバイト (FF D8 FF) を確認
    assert_eq!(
        &jpeg_data[0..3],
        &[0xFF, 0xD8, 0xFF],
        "Output should start with JPEG magic bytes"
    );

    // デバッグ用: 変換後の JPEG を一時ファイルに保存
    if std::env::var("SAVE_TEST_OUTPUT").is_ok() {
        let output_path = std::env::temp_dir().join("test_output.jpg");
        if let Ok(mut file) = std::fs::File::create(&output_path) {
            let _ = file.write_all(&jpeg_data);
            println!("Saved test output to: {:?}", output_path);
        }
    }
}

#[cfg(not(unix))]
#[test]
fn test_convert_heic_unsupported_on_windows() {
    let heic_data = include_bytes!("fixtures/sample.heic");

    let result = heic_converter::convert_heic_to_jpeg(heic_data);

    assert!(
        result.is_err(),
        "HEIC conversion should fail on non-Unix platforms"
    );
}
