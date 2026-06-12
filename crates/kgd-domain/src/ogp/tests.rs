use super::*;

/// og:title と og:description が両方揃った HTML を解析し、
/// それぞれの content がタイトル・説明として取得できることを確認する。
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

/// meta タグで content 属性が property より先に書かれていても、
/// 属性の順序に依存せずタイトル・説明を取得できることを確認する。
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

/// og:title が無い場合に <title> タグの内容へフォールバックし、
/// 説明は取得できず None になることを確認する。
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

/// og:description が無い場合に meta name="description" の内容へ
/// フォールバックして説明を取得できることを確認する。
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

/// og:description と meta name="description" が両方ある場合、
/// og:description が優先されることを確認する。
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

/// og:title が空文字、og:description が空白のみの場合はそれらを無視し、
/// タイトルは <title> タグへフォールバック、説明は None になることを確認する。
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

/// content の前後に空白がある場合、トリムされた値が
/// タイトル・説明として取得されることを確認する。
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

/// OGP も <title> も meta description も無い HTML では、
/// タイトル・説明ともに None になることを確認する。
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

/// content 内の HTML エンティティ（&amp; や &lt; など）が
/// 対応する文字へデコードされることを確認する。
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

/// extract_title_tag が <title> の内容をトリムして返し、
/// 空タイトルや title 要素が無い場合は None を返すことを確認する。
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
