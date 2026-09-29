//! OpenStreetMap のタイルに軌跡を重ねて描く [`MapRenderer`] の実装。

use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{Context as _, Result};
use tiny_skia::{
    Color, FillRule, LineCap, LineJoin, Paint, PathBuilder, Pixmap, PixmapPaint, Stroke, Transform,
};
use tracing::warn;

use kgd_application::ports::MapRenderer;
use kgd_domain::{Activity, TilePlacement, TrackSegment, Viewport};

#[cfg(test)]
mod tests;

/// OpenStreetMap の公式タイルのベース URL。
const OSM_TILE_BASE_URL: &str = "https://tile.openstreetmap.org";

/// タイル 1 枚の取得のタイムアウト。
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// タイルが無い部分の背景色。
const BACKGROUND: (u8, u8, u8) = (0xdd, 0xdd, 0xdd);

/// 軌跡の線の太さ (ピクセル)。
const LINE_WIDTH: f32 = 4.0;

/// 静止点と、1 点だけの区間を示す円の半径 (ピクセル)。
const DOT_RADIUS: f32 = 3.0;

/// OpenStreetMap のタイルに軌跡を重ねて描く描画器。
pub struct TileMapRenderer {
    /// タイル取得用の HTTP クライアント (User-Agent 設定済み)
    http: reqwest::Client,
    /// タイルサーバーのベース URL (テストではスタブを指す)
    base_url: String,
    /// タイルのキャッシュ先ディレクトリ
    cache_dir: PathBuf,
}

impl TileMapRenderer {
    /// OpenStreetMap の公式タイルを使う描画器を作る。
    pub fn new(user_agent: &str, cache_dir: impl Into<PathBuf>) -> Result<Self> {
        Self::with_base_url(user_agent, cache_dir, OSM_TILE_BASE_URL)
    }

    /// タイルサーバーのベース URL を指定して描画器を作る。
    pub(crate) fn with_base_url(
        user_agent: &str,
        cache_dir: impl Into<PathBuf>,
        base_url: &str,
    ) -> Result<Self> {
        let http = reqwest::Client::builder()
            .user_agent(user_agent)
            .timeout(REQUEST_TIMEOUT)
            .build()
            .context("Failed to build map tile HTTP client")?;
        Ok(Self {
            http,
            base_url: base_url.trim_end_matches('/').to_string(),
            cache_dir: cache_dir.into(),
        })
    }

    /// タイルをキャッシュから、無ければサーバーから読み込む。
    ///
    /// 取得やデコードに失敗したら `None` を返し、呼び出し側は背景色のまま描画を続ける。
    async fn load_tile(&self, tile: &TilePlacement) -> Option<Pixmap> {
        let path = self
            .cache_dir
            .join(tile.z.to_string())
            .join(tile.x.to_string())
            .join(format!("{}.png", tile.y));

        if let Ok(bytes) = tokio::fs::read(&path).await {
            match Pixmap::decode_png(&bytes) {
                Ok(pixmap) => return Some(pixmap),
                Err(error) => {
                    warn!(?error, path = %path.display(), "Ignoring broken cached map tile")
                }
            }
        }

        let bytes = match self.fetch_tile(tile).await {
            Ok(bytes) => bytes,
            Err(error) => {
                warn!(
                    ?error,
                    z = tile.z,
                    x = tile.x,
                    y = tile.y,
                    "Failed to fetch map tile"
                );
                return None;
            }
        };
        let pixmap = match Pixmap::decode_png(&bytes) {
            Ok(pixmap) => pixmap,
            Err(error) => {
                warn!(
                    ?error,
                    z = tile.z,
                    x = tile.x,
                    y = tile.y,
                    "Failed to decode map tile"
                );
                return None;
            }
        };
        if let Err(error) = save_tile(&path, &bytes).await {
            warn!(?error, path = %path.display(), "Failed to cache map tile");
        }
        Some(pixmap)
    }

    /// タイルをサーバーから取得する。
    async fn fetch_tile(&self, tile: &TilePlacement) -> Result<Vec<u8>> {
        let url = format!("{}/{}/{}/{}.png", self.base_url, tile.z, tile.x, tile.y);
        let response = self
            .http
            .get(&url)
            .send()
            .await
            .with_context(|| format!("Failed to request map tile: {url}"))?
            .error_for_status()
            .with_context(|| format!("Map tile server returned an error: {url}"))?;
        let bytes = response
            .bytes()
            .await
            .context("Failed to read map tile body")?;
        Ok(bytes.to_vec())
    }
}

#[async_trait::async_trait]
impl MapRenderer for TileMapRenderer {
    async fn render(&self, viewport: &Viewport, segments: &[TrackSegment]) -> Result<Vec<u8>> {
        let mut canvas =
            Pixmap::new(viewport.width, viewport.height).context("Invalid map image size")?;
        let (r, g, b) = BACKGROUND;
        canvas.fill(Color::from_rgba8(r, g, b, 255));

        for tile in viewport.tiles() {
            if let Some(pixmap) = self.load_tile(&tile).await {
                canvas.draw_pixmap(
                    tile.offset_x,
                    tile.offset_y,
                    pixmap.as_ref(),
                    &PixmapPaint::default(),
                    Transform::identity(),
                    None,
                );
            }
        }

        for segment in segments {
            draw_segment(&mut canvas, viewport, segment);
        }

        canvas.encode_png().context("Failed to encode map image")
    }
}

/// タイルをキャッシュへ書き出す。
async fn save_tile(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(path, bytes).await?;
    Ok(())
}

/// 移動種別ごとの描画色 (R, G, B)。
fn segment_rgb(activity: Option<Activity>) -> (u8, u8, u8) {
    match activity {
        Some(Activity::Walking) => (0x2e, 0x9e, 0x44),
        Some(Activity::Automotive) => (0x1f, 0x6f, 0xd1),
        Some(Activity::Cycling) => (0xf0, 0x8c, 0x1a),
        Some(Activity::Stationary) => (0xc0, 0x39, 0x2b),
        None => (0x80, 0x80, 0x80),
    }
}

/// 区間を 1 つ描く。
///
/// 静止の区間と 1 点だけの区間は点で、それ以外は折れ線で描く。
fn draw_segment(canvas: &mut Pixmap, viewport: &Viewport, segment: &TrackSegment) {
    let (r, g, b) = segment_rgb(segment.activity);
    let mut paint = Paint::default();
    paint.set_color_rgba8(r, g, b, 255);
    paint.anti_alias = true;

    if segment.activity == Some(Activity::Stationary) || segment.points.len() == 1 {
        for point in &segment.points {
            let (x, y) = viewport.project(point.lat, point.lon);
            if let Some(circle) = PathBuilder::from_circle(x, y, DOT_RADIUS) {
                canvas.fill_path(
                    &circle,
                    &paint,
                    FillRule::Winding,
                    Transform::identity(),
                    None,
                );
            }
        }
        return;
    }

    let mut builder = PathBuilder::new();
    for (index, point) in segment.points.iter().enumerate() {
        let (x, y) = viewport.project(point.lat, point.lon);
        if index == 0 {
            builder.move_to(x, y);
        } else {
            builder.line_to(x, y);
        }
    }
    let Some(path) = builder.finish() else {
        return;
    };
    let stroke = Stroke {
        width: LINE_WIDTH,
        line_cap: LineCap::Round,
        line_join: LineJoin::Round,
        ..Stroke::default()
    };
    canvas.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
}
