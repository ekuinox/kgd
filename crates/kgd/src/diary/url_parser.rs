//! メッセージテキスト内の URL を解析し、Notion ブロック構築用のセグメントに分割する。

use regex::Regex;

/// テキストセグメントの種類。
#[derive(Debug, Clone, PartialEq, Eq)]
enum TextSegment {
    /// 通常のテキスト
    Plain(String),
    /// URL
    Url(String),
}

/// URL の表示方法。
#[derive(Debug, Clone, PartialEq, Eq)]
enum UrlRendering {
    /// Notion ブックマークブロックとして表示
    Bookmark,
    /// rich_text のインラインリンクとして表示
    InlineLink,
}

/// パターン文字列のスライスからコンパイル済み正規表現のベクタを作成する。
///
/// 無効なパターンは警告をログに出力してスキップする。
pub fn compile_patterns(patterns: &[String]) -> Vec<Regex> {
    patterns
        .iter()
        .filter_map(|p| {
            Regex::new(p)
                .map_err(|e| {
                    tracing::warn!(pattern = %p, error = %e, "Invalid bookmark URL pattern, skipping");
                })
                .ok()
        })
        .collect()
}

/// テキストからセグメントを解析し、Notion paragraph 用の rich_text JSON 配列と
/// ブックマークとして別ブロック化する URL のリストを返す。
///
/// # Returns
/// `(rich_text_array, bookmark_urls)` のタプル。
/// - `rich_text_array`: paragraph ブロックの rich_text に使用する JSON 配列
/// - `bookmark_urls`: ブックマークブロックとして追加する URL のリスト
pub fn build_rich_text_and_bookmarks(
    text: &str,
    bookmark_patterns: &[Regex],
) -> (Vec<serde_json::Value>, Vec<String>) {
    let segments = parse_segments(text);
    let mut rich_text_items: Vec<serde_json::Value> = Vec::new();
    let mut bookmark_urls: Vec<String> = Vec::new();

    for segment in segments {
        match segment {
            TextSegment::Plain(s) => {
                if !s.is_empty() {
                    rich_text_items.push(serde_json::json!({
                        "type": "text",
                        "text": {
                            "content": s
                        }
                    }));
                }
            }
            TextSegment::Url(url) => {
                match classify_url(&url, bookmark_patterns) {
                    UrlRendering::Bookmark => {
                        bookmark_urls.push(url.clone());
                    }
                    UrlRendering::InlineLink => {}
                }
                // URL は常にインラインリンクとしてテキストに含める
                rich_text_items.push(serde_json::json!({
                    "type": "text",
                    "text": {
                        "content": url,
                        "link": {
                            "url": url
                        }
                    }
                }));
            }
        }
    }

    (rich_text_items, bookmark_urls)
}

/// テキストを URL とプレーンテキストのセグメントに分割する。
fn parse_segments(text: &str) -> Vec<TextSegment> {
    let url_re = Regex::new(r"https?://[^\s<>\[\]()]+").unwrap();

    let mut segments = Vec::new();
    let mut last_end = 0;

    for m in url_re.find_iter(text) {
        if m.start() > last_end {
            segments.push(TextSegment::Plain(text[last_end..m.start()].to_string()));
        }
        segments.push(TextSegment::Url(m.as_str().to_string()));
        last_end = m.end();
    }

    if last_end < text.len() {
        segments.push(TextSegment::Plain(text[last_end..].to_string()));
    }

    segments
}

/// URL がブックマークパターンに一致するかを判定する。
fn classify_url(url: &str, bookmark_patterns: &[Regex]) -> UrlRendering {
    for pattern in bookmark_patterns {
        if pattern.is_match(url) {
            return UrlRendering::Bookmark;
        }
    }
    UrlRendering::InlineLink
}

#[cfg(test)]
mod tests {
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

    #[test]
    fn test_classify_url_no_patterns() {
        assert_eq!(
            classify_url("https://example.com", &[]),
            UrlRendering::InlineLink
        );
    }

    #[test]
    fn test_classify_url_matching_pattern() {
        let patterns = vec![Regex::new(r"https://github\.com/.*").unwrap()];
        assert_eq!(
            classify_url("https://github.com/ekuinox/kgd", &patterns),
            UrlRendering::Bookmark
        );
    }

    #[test]
    fn test_classify_url_non_matching_pattern() {
        let patterns = vec![Regex::new(r"https://github\.com/.*").unwrap()];
        assert_eq!(
            classify_url("https://example.com", &patterns),
            UrlRendering::InlineLink
        );
    }

    #[test]
    fn test_build_rich_text_no_urls() {
        let (rich_text, bookmarks) = build_rich_text_and_bookmarks("plain text", &[]);
        assert_eq!(rich_text.len(), 1);
        assert_eq!(rich_text[0]["text"]["content"], "plain text");
        assert!(rich_text[0]["text"]["link"].is_null());
        assert!(bookmarks.is_empty());
    }

    #[test]
    fn test_build_rich_text_with_inline_url() {
        let (rich_text, bookmarks) =
            build_rich_text_and_bookmarks("see https://example.com here", &[]);
        assert_eq!(rich_text.len(), 3);
        assert_eq!(rich_text[0]["text"]["content"], "see ");
        assert_eq!(rich_text[1]["text"]["content"], "https://example.com");
        assert_eq!(rich_text[1]["text"]["link"]["url"], "https://example.com");
        assert_eq!(rich_text[2]["text"]["content"], " here");
        assert!(bookmarks.is_empty());
    }

    #[test]
    fn test_build_rich_text_with_bookmark_url() {
        let patterns = vec![Regex::new(r"https://github\.com/.*").unwrap()];
        let (rich_text, bookmarks) =
            build_rich_text_and_bookmarks("check https://github.com/ekuinox/kgd", &patterns);
        assert_eq!(rich_text.len(), 2);
        assert_eq!(rich_text[0]["text"]["content"], "check ");
        assert_eq!(
            rich_text[1]["text"]["content"],
            "https://github.com/ekuinox/kgd"
        );
        assert_eq!(
            rich_text[1]["text"]["link"]["url"],
            "https://github.com/ekuinox/kgd"
        );
        assert_eq!(bookmarks, vec!["https://github.com/ekuinox/kgd"]);
    }

    #[test]
    fn test_build_rich_text_mixed_urls() {
        let patterns = vec![Regex::new(r"https://github\.com/.*").unwrap()];
        let (rich_text, bookmarks) = build_rich_text_and_bookmarks(
            "see https://example.com and https://github.com/ekuinox/kgd",
            &patterns,
        );
        assert_eq!(rich_text.len(), 4);
        assert_eq!(rich_text[0]["text"]["content"], "see ");
        assert_eq!(rich_text[1]["text"]["content"], "https://example.com");
        assert_eq!(rich_text[2]["text"]["content"], " and ");
        assert_eq!(
            rich_text[3]["text"]["content"],
            "https://github.com/ekuinox/kgd"
        );
        assert_eq!(bookmarks.len(), 1);
        assert_eq!(bookmarks[0], "https://github.com/ekuinox/kgd");
    }

    #[test]
    fn test_compile_patterns_valid() {
        let patterns = vec!["https://github\\.com/.*".to_string()];
        let compiled = compile_patterns(&patterns);
        assert_eq!(compiled.len(), 1);
    }

    #[test]
    fn test_compile_patterns_invalid_skipped() {
        let patterns = vec![
            "https://github\\.com/.*".to_string(),
            "[invalid".to_string(),
        ];
        let compiled = compile_patterns(&patterns);
        assert_eq!(compiled.len(), 1);
    }

    #[test]
    fn test_compile_patterns_empty() {
        let compiled = compile_patterns(&[]);
        assert!(compiled.is_empty());
    }
}
