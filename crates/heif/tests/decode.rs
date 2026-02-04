use heif::read_heif_to_dynamic_image;

const SAMPLE_HEIC: &[u8] = include_bytes!("sample1.heic");

#[test]
fn test_read_heif_to_dynamic_image() {
    let image = read_heif_to_dynamic_image(SAMPLE_HEIC).expect("Failed to decode HEIC");

    assert!(image.width() > 0);
    assert!(image.height() > 0);
}
