//! parse_segments のテスト。

use super::*;

#[test]
fn test_parse_segments_no_urls() {
    let result = parse_segments("hello world");
    assert_eq!(result, vec![TextSegment::Plain("hello world".to_string())]);
}

#[test]
fn test_parse_segments_single_url() {
    let result = parse_segments("check https://example.com please");
    assert_eq!(
        result,
        vec![
            TextSegment::Plain("check ".to_string()),
            TextSegment::Url("https://example.com".to_string()),
            TextSegment::Plain(" please".to_string()),
        ]
    );
}

#[test]
fn test_parse_segments_multiple_urls() {
    let result = parse_segments("https://a.com and https://b.com");
    assert_eq!(
        result,
        vec![
            TextSegment::Url("https://a.com".to_string()),
            TextSegment::Plain(" and ".to_string()),
            TextSegment::Url("https://b.com".to_string()),
        ]
    );
}

#[test]
fn test_parse_segments_url_only() {
    let result = parse_segments("https://example.com");
    assert_eq!(
        result,
        vec![TextSegment::Url("https://example.com".to_string())]
    );
}

#[test]
fn test_parse_segments_http_url() {
    let result = parse_segments("link: http://example.com");
    assert_eq!(
        result,
        vec![
            TextSegment::Plain("link: ".to_string()),
            TextSegment::Url("http://example.com".to_string()),
        ]
    );
}

#[test]
fn test_parse_segments_empty() {
    let result = parse_segments("");
    assert!(result.is_empty());
}
