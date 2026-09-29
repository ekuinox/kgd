//! TileMapRenderer の単体テスト。スタブのタイルサーバーへ実際に HTTP で取りにいく。

use std::sync::{Arc, Mutex};

use axum::{
    Router,
    http::{HeaderMap, StatusCode, Uri, header::USER_AGENT},
};
use tiny_skia::{Color, Pixmap};

use kgd_domain::{Activity, TrackPoint, TrackSegment, Viewport};

use super::*;

/// 受け取ったリクエスト (パス, User-Agent) の記録。
type Requests = Arc<Mutex<Vec<(String, Option<String>)>>>;

/// どのパスにも同じ応答を返すスタブのタイルサーバーを起動し、ベース URL と記録を返す。
async fn start_tile_server(status: StatusCode, body: Vec<u8>) -> (String, Requests) {
    let requests: Requests = Arc::new(Mutex::new(Vec::new()));
    let recorded = requests.clone();
    let app = Router::new().fallback(move |uri: Uri, headers: HeaderMap| {
        let recorded = recorded.clone();
        let body = body.clone();
        async move {
            let agent = headers
                .get(USER_AGENT)
                .and_then(|value| value.to_str().ok())
                .map(str::to_string);
            recorded
                .lock()
                .unwrap()
                .push((uri.path().to_string(), agent));
            (status, body)
        }
    });

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base_url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (base_url, requests)
}

/// 単色で塗った 256x256 のタイル PNG を作る。
fn solid_tile(r: u8, g: u8, b: u8) -> Vec<u8> {
    let mut pixmap = Pixmap::new(256, 256).unwrap();
    pixmap.fill(Color::from_rgba8(r, g, b, 255));
    pixmap.encode_png().unwrap()
}

/// 描画結果の指定ピクセルの (R, G, B) を返す。
fn rgb_at(png: &[u8], x: u32, y: u32) -> (u8, u8, u8) {
    let pixel = Pixmap::decode_png(png).unwrap().pixel(x, y).unwrap();
    (pixel.red(), pixel.green(), pixel.blue())
}

/// ズーム 1 のタイル (0, 0) だけを覆う表示範囲。
fn single_tile_viewport() -> Viewport {
    Viewport {
        zoom: 1,
        left: 0.0,
        top: 0.0,
        width: 256,
        height: 256,
    }
}

/// タイルを OSM と同じパスで、指定した User-Agent を付けて取りにいき、画像に貼ることを確認する。
///
/// OSM のタイル利用規約は識別可能な User-Agent を求めるため。
#[tokio::test]
async fn render_requests_tiles_with_user_agent() {
    let (base_url, requests) = start_tile_server(StatusCode::OK, solid_tile(0, 0, 255)).await;
    let cache = tempfile::tempdir().unwrap();
    let renderer = TileMapRenderer::with_base_url("kgd/test", cache.path(), &base_url).unwrap();

    let png = renderer.render(&single_tile_viewport(), &[]).await.unwrap();

    assert_eq!(
        requests.lock().unwrap().clone(),
        vec![("/1/0/0.png".to_string(), Some("kgd/test".to_string()))]
    );
    assert_eq!(rgb_at(&png, 10, 10), (0, 0, 255));
}

/// 2 回目の描画ではキャッシュしたタイルを使い、同じタイルを取り直さないことを確認する。
#[tokio::test]
async fn render_reuses_cached_tiles() {
    let (base_url, requests) = start_tile_server(StatusCode::OK, solid_tile(0, 0, 255)).await;
    let cache = tempfile::tempdir().unwrap();
    let renderer = TileMapRenderer::with_base_url("kgd/test", cache.path(), &base_url).unwrap();

    renderer.render(&single_tile_viewport(), &[]).await.unwrap();
    renderer.render(&single_tile_viewport(), &[]).await.unwrap();

    assert_eq!(requests.lock().unwrap().len(), 1);
    assert!(cache.path().join("1").join("0").join("0.png").exists());
}

/// タイルの取得に失敗しても描画を続け、その部分を灰色で残すことを確認する。
///
/// 失敗を描画全体の失敗にすると、定時ジョブが毎分 OSM へ取りにいき直すことになるため。
#[tokio::test]
async fn render_fills_failed_tiles_with_gray_and_continues() {
    let (base_url, _) = start_tile_server(StatusCode::NOT_FOUND, Vec::new()).await;
    let cache = tempfile::tempdir().unwrap();
    let renderer = TileMapRenderer::with_base_url("kgd/test", cache.path(), &base_url).unwrap();

    let png = renderer.render(&single_tile_viewport(), &[]).await.unwrap();

    assert_eq!(rgb_at(&png, 10, 10), BACKGROUND);
}

/// PNG として読めない応答は灰色で残し、キャッシュにも保存しないことを確認する。
#[tokio::test]
async fn render_fills_undecodable_tiles_with_gray() {
    let (base_url, _) = start_tile_server(StatusCode::OK, b"not a png".to_vec()).await;
    let cache = tempfile::tempdir().unwrap();
    let renderer = TileMapRenderer::with_base_url("kgd/test", cache.path(), &base_url).unwrap();

    let png = renderer.render(&single_tile_viewport(), &[]).await.unwrap();

    assert_eq!(rgb_at(&png, 10, 10), BACKGROUND);
    assert!(!cache.path().join("1").join("0").join("0.png").exists());
}

/// 区間が移動種別の色の線で描かれることを確認する。
#[tokio::test]
async fn render_draws_segments_in_activity_color() {
    let (base_url, _) = start_tile_server(StatusCode::NOT_FOUND, Vec::new()).await;
    let cache = tempfile::tempdir().unwrap();
    let renderer = TileMapRenderer::with_base_url("kgd/test", cache.path(), &base_url).unwrap();
    // 赤道が画像の縦の中央 (y = 128) に来る表示範囲
    let viewport = Viewport {
        zoom: 1,
        left: 0.0,
        top: 128.0,
        width: 256,
        height: 256,
    };
    let at = chrono::Utc::now();
    let point = |lon: f64| TrackPoint {
        at,
        lat: 0.0,
        lon,
        accuracy_m: None,
        activity: Some(Activity::Walking),
    };
    // 画像の x = 20 と x = 236 にあたる経度
    let segment = TrackSegment {
        activity: Some(Activity::Walking),
        points: vec![point(-165.9375), point(-14.0625)],
    };

    let png = renderer.render(&viewport, &[segment]).await.unwrap();

    assert_eq!(rgb_at(&png, 128, 128), segment_rgb(Some(Activity::Walking)));
}
