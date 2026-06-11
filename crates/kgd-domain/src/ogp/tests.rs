use super::*;

#[test]
fn test_parse_ogp_metadata_full() {
    let html = r#"
            <!DOCTYPE html>
            <html>
            <head>
                <meta property="og:title" content="Test Title">
                <meta property="og:description" content="Test Description">
            </head>
            </html>
        "#;

    let metadata = parse_ogp_metadata(html);
    assert_eq!(metadata.title, Some("Test Title".to_string()));
    assert_eq!(metadata.description, Some("Test Description".to_string()));
}

#[test]
fn test_parse_ogp_metadata_content_first() {
    let html = r#"
            <!DOCTYPE html>
            <html>
            <head>
                <meta content="Title Content First" property="og:title">
                <meta content="Description Content First" property="og:description">
            </head>
            </html>
        "#;

    let metadata = parse_ogp_metadata(html);
    assert_eq!(metadata.title, Some("Title Content First".to_string()));
    assert_eq!(
        metadata.description,
        Some("Description Content First".to_string())
    );
}

#[test]
fn test_parse_ogp_metadata_fallback_to_title_tag() {
    let html = r#"
            <!DOCTYPE html>
            <html>
            <head>
                <title>Fallback Title</title>
            </head>
            </html>
        "#;

    let metadata = parse_ogp_metadata(html);
    assert_eq!(metadata.title, Some("Fallback Title".to_string()));
    assert_eq!(metadata.description, None);
}

#[test]
fn test_parse_ogp_metadata_fallback_to_meta_description() {
    let html = r#"
            <!DOCTYPE html>
            <html>
            <head>
                <meta property="og:title" content="OGP Title">
                <meta name="description" content="Meta Description">
            </head>
            </html>
        "#;

    let metadata = parse_ogp_metadata(html);
    assert_eq!(metadata.title, Some("OGP Title".to_string()));
    assert_eq!(metadata.description, Some("Meta Description".to_string()));
}

#[test]
fn test_parse_ogp_metadata_og_description_takes_priority() {
    let html = r#"
            <!DOCTYPE html>
            <html>
            <head>
                <meta property="og:description" content="OGP Description">
                <meta name="description" content="Meta Description">
            </head>
            </html>
        "#;

    let metadata = parse_ogp_metadata(html);
    assert_eq!(metadata.description, Some("OGP Description".to_string()));
}

#[test]
fn test_parse_ogp_metadata_empty_values_ignored() {
    let html = r#"
            <!DOCTYPE html>
            <html>
            <head>
                <meta property="og:title" content="">
                <meta property="og:description" content="   ">
                <title>Fallback Title</title>
            </head>
            </html>
        "#;

    let metadata = parse_ogp_metadata(html);
    assert_eq!(metadata.title, Some("Fallback Title".to_string()));
    assert_eq!(metadata.description, None);
}

#[test]
fn test_parse_ogp_metadata_whitespace_trimmed() {
    let html = r#"
            <!DOCTYPE html>
            <html>
            <head>
                <meta property="og:title" content="  Trimmed Title  ">
                <meta property="og:description" content="  Trimmed Description  ">
            </head>
            </html>
        "#;

    let metadata = parse_ogp_metadata(html);
    assert_eq!(metadata.title, Some("Trimmed Title".to_string()));
    assert_eq!(
        metadata.description,
        Some("Trimmed Description".to_string())
    );
}

#[test]
fn test_parse_ogp_metadata_no_metadata() {
    let html = r#"
            <!DOCTYPE html>
            <html>
            <head></head>
            <body>Hello</body>
            </html>
        "#;

    let metadata = parse_ogp_metadata(html);
    assert_eq!(metadata.title, None);
    assert_eq!(metadata.description, None);
}

#[test]
fn test_parse_ogp_metadata_html_entities_decoded() {
    let html = r#"
            <!DOCTYPE html>
            <html>
            <head>
                <meta property="og:title" content="Title &amp; More">
                <meta property="og:description" content="&lt;Test&gt; &quot;Description&quot;">
            </head>
            </html>
        "#;

    let metadata = parse_ogp_metadata(html);
    assert_eq!(metadata.title, Some("Title & More".to_string()));
    assert_eq!(
        metadata.description,
        Some("<Test> \"Description\"".to_string())
    );
}

#[test]
fn test_extract_title_tag() {
    assert_eq!(
        extract_title_tag("<title>Test</title>"),
        Some("Test".to_string())
    );
    assert_eq!(
        extract_title_tag("<title>  Trimmed  </title>"),
        Some("Trimmed".to_string())
    );
    assert_eq!(extract_title_tag("<title></title>"), None);
    assert_eq!(extract_title_tag("<p>No title</p>"), None);
}
