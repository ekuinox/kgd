//! 埋め込んだ画面 (`web/dist`) の配信。

use axum::{
    extract::Path,
    http::{StatusCode, header},
    response::{Html, IntoResponse, Redirect, Response},
};
use rust_embed::{Embed, EmbeddedFile};

/// ビルドした画面。`web/dist` が無くてもビルドは通り、そのときは空になる。
///
/// `web/dist` の中身が変わったら `build.rs` がこのクレートを作り直させる。
/// Docker のイメージでは、ビルドの段で index.html があることを確かめてから埋め込む。
#[derive(Embed)]
#[folder = "../../web/dist"]
#[allow_missing = true]
struct WebAssets;

/// 画面が未ビルドのときに返すページ。
const NOT_BUILT_PAGE: &str = r#"<!doctype html>
<html lang="ja">
<head><meta charset="utf-8"><title>位置ログ</title></head>
<body>
<p>画面が未ビルドです。リポジトリで <code>just web-build</code> を実行し、kgd をビルドし直してから起動し直してください。</p>
</body>
</html>
"#;

/// `/viewer` を `/viewer/` へ転送する。
pub(super) async fn redirect_to_index() -> Redirect {
    Redirect::permanent("/viewer/")
}

/// `/viewer/` に index.html を返す。
pub(super) async fn handle_index() -> Response {
    index_response(WebAssets::get("index.html"))
}

/// `/viewer/` 以下のファイルを返す。
///
/// 見つからないパスには index.html を返す。ただし `assets/` 以下は 404 にする
/// (JS や CSS の代わりに HTML を返さないため)。`api/` 以下はこのハンドラに届かない。
pub(super) async fn handle_asset(Path(path): Path<String>) -> Response {
    match WebAssets::get(&path) {
        Some(file) => file_response(&path, file),
        None if path.starts_with("assets/") => StatusCode::NOT_FOUND.into_response(),
        None => index_response(WebAssets::get("index.html")),
    }
}

/// index.html を返す。無ければ未ビルドの案内を 503 で返す。
fn index_response(index: Option<EmbeddedFile>) -> Response {
    match index {
        Some(file) => file_response("index.html", file),
        None => (StatusCode::SERVICE_UNAVAILABLE, Html(NOT_BUILT_PAGE)).into_response(),
    }
}

/// 埋め込んだファイルを、種類とキャッシュの指定を付けて返す。
fn file_response(path: &str, file: EmbeddedFile) -> Response {
    (
        [
            (header::CONTENT_TYPE, file.metadata.mimetype().to_string()),
            (header::CACHE_CONTROL, cache_control(path).to_string()),
        ],
        file.data.into_owned(),
    )
        .into_response()
}

/// パスに応じたキャッシュの指定を返す。
///
/// Vite は `assets/` 以下のファイル名に内容のハッシュを付けるため、長くキャッシュさせてよい。
fn cache_control(path: &str) -> &'static str {
    if path.starts_with("assets/") {
        "public, max-age=31536000, immutable"
    } else {
        "no-cache"
    }
}

#[cfg(test)]
mod tests {
    use axum::body::to_bytes;

    use super::*;

    /// ファイル名にハッシュを含む `assets/` 以下だけを長くキャッシュさせることを確認する。
    ///
    /// index.html をキャッシュさせると、画面を更新しても古い JS を読み続けるため。
    #[test]
    fn cache_control_is_immutable_only_for_hashed_assets() {
        assert_eq!(
            cache_control("assets/index-3f9a1c.js"),
            "public, max-age=31536000, immutable"
        );
        assert_eq!(cache_control("index.html"), "no-cache");
        assert_eq!(cache_control("favicon.svg"), "no-cache");
    }

    /// index.html が無ければ、ビルドの手順を添えた 503 の HTML を返すことを確認する。
    #[tokio::test]
    async fn index_response_explains_how_to_build_when_missing() {
        let response = index_response(None);

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body = String::from_utf8(body.to_vec()).unwrap();
        assert!(body.contains("just web-build"));
    }
}
