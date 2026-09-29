//! Web メルカトル投影と、地図画像の表示範囲・ズームの決定。

use std::f64::consts::PI;

use super::{summary::haversine_m, track::TrackPoint};

/// タイル 1 枚の一辺のピクセル数。
pub const TILE_SIZE: u32 = 256;

/// Web メルカトルで表せる緯度の上限。
const MAX_LATITUDE: f64 = 85.051_128_779_806_59;

/// 選べるズームの下限。
const MIN_ZOOM: u8 = 3;

/// 選べるズームの上限。
const MAX_ZOOM: u8 = 17;

/// bbox が潰れているときに使うズーム。
const STILL_ZOOM: u8 = 15;

/// bbox が潰れているとみなす長辺の長さ (メートル)。
const STILL_SPAN_M: f64 = 200.0;

/// 画像の縁に残す余白 (ピクセル)。
const PADDING_PX: f64 = 32.0;

/// 地図画像に写す範囲。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Viewport {
    /// ズームレベル
    pub zoom: u8,
    /// 画像の左端の世界ピクセル座標
    pub left: f64,
    /// 画像の上端の世界ピクセル座標
    pub top: f64,
    /// 画像の幅 (ピクセル)
    pub width: u32,
    /// 画像の高さ (ピクセル)
    pub height: u32,
}

/// 画像に貼るタイル 1 枚とその位置。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TilePlacement {
    /// ズームレベル
    pub z: u8,
    /// タイルの x (経度方向に折り返し済み)
    pub x: u32,
    /// タイルの y
    pub y: u32,
    /// 画像内でタイルの左上を置く x
    pub offset_x: i32,
    /// 画像内でタイルの左上を置く y
    pub offset_y: i32,
}

/// 緯度経度を、指定ズームでの世界ピクセル座標へ変換する。
pub fn world_pixel(lat: f64, lon: f64, zoom: u8) -> (f64, f64) {
    let scale = f64::from(TILE_SIZE) * 2f64.powi(i32::from(zoom));
    let x = (lon + 180.0) / 360.0 * scale;
    let lat = lat.clamp(-MAX_LATITUDE, MAX_LATITUDE).to_radians();
    let y = (1.0 - (lat.tan() + 1.0 / lat.cos()).ln() / PI) / 2.0 * scale;
    (x, y)
}

impl Viewport {
    /// 緯度経度を画像内のピクセル座標へ変換する。
    pub fn project(&self, lat: f64, lon: f64) -> (f32, f32) {
        let (x, y) = world_pixel(lat, lon, self.zoom);
        ((x - self.left) as f32, (y - self.top) as f32)
    }

    /// 表示範囲を覆うタイルを、上の行から左から順に列挙する。
    ///
    /// 経度方向は世界の端で折り返し、緯度方向で世界の外にはみ出した行は含めない。
    pub fn tiles(&self) -> Vec<TilePlacement> {
        let tile = f64::from(TILE_SIZE);
        let count = 1i64 << self.zoom;
        let first_x = (self.left / tile).floor() as i64;
        let last_x = ((self.left + f64::from(self.width) - 1.0) / tile).floor() as i64;
        let first_y = (self.top / tile).floor() as i64;
        let last_y = ((self.top + f64::from(self.height) - 1.0) / tile).floor() as i64;

        let mut placements = Vec::new();
        for ty in first_y.max(0)..=last_y.min(count - 1) {
            for tx in first_x..=last_x {
                placements.push(TilePlacement {
                    z: self.zoom,
                    x: tx.rem_euclid(count) as u32,
                    y: ty as u32,
                    offset_x: (tx as f64 * tile - self.left).round() as i32,
                    offset_y: (ty as f64 * tile - self.top).round() as i32,
                });
            }
        }
        placements
    }
}

/// 点群がすべて収まる表示範囲を決める。
///
/// 余白を除いた範囲に収まる最大のズームを 3 から 17 の間で選び、bbox の中心を画像の中心に置く。
/// bbox の長辺が 200 m 未満 (1 点のみ、終日ほぼ静止など) のときはズーム 15 に固定する。
pub fn fit_viewport(points: &[TrackPoint], width: u32, height: u32) -> Option<Viewport> {
    let first = points.first()?;
    let (mut south, mut north, mut west, mut east) = (first.lat, first.lat, first.lon, first.lon);
    for point in points {
        south = south.min(point.lat);
        north = north.max(point.lat);
        west = west.min(point.lon);
        east = east.max(point.lon);
    }

    let span_m =
        haversine_m((south, west), (north, west)).max(haversine_m((south, west), (south, east)));
    let usable_width = f64::from(width) - PADDING_PX * 2.0;
    let usable_height = f64::from(height) - PADDING_PX * 2.0;

    let zoom = if span_m < STILL_SPAN_M {
        STILL_ZOOM
    } else {
        (MIN_ZOOM..=MAX_ZOOM)
            .rev()
            .find(|&zoom| {
                let (x0, y0) = world_pixel(north, west, zoom);
                let (x1, y1) = world_pixel(south, east, zoom);
                x1 - x0 <= usable_width && y1 - y0 <= usable_height
            })
            .unwrap_or(MIN_ZOOM)
    };

    let (x0, y0) = world_pixel(north, west, zoom);
    let (x1, y1) = world_pixel(south, east, zoom);
    let center_x = (x0 + x1) / 2.0;
    let center_y = (y0 + y1) / 2.0;

    Some(Viewport {
        zoom,
        left: center_x - f64::from(width) / 2.0,
        top: center_y - f64::from(height) / 2.0,
        width,
        height,
    })
}

#[cfg(test)]
mod tests {
    use crate::location::track::tests::point;

    use super::*;

    /// ズーム 0 で緯度経度 (0, 0) が世界の中心ピクセルになることを確認する。
    #[test]
    fn world_pixel_maps_origin_to_center_at_zoom_zero() {
        let (x, y) = world_pixel(0.0, 0.0, 0);

        assert!((x - 128.0).abs() < 1e-9);
        assert!((y - 128.0).abs() < 1e-9);
    }

    /// Web メルカトルの北端と西端が世界の左上隅になることを確認する。
    #[test]
    fn world_pixel_maps_north_west_limit_to_top_left() {
        let (x, y) = world_pixel(MAX_LATITUDE, -180.0, 0);

        assert!(x.abs() < 1e-6);
        assert!(y.abs() < 1e-6);
    }

    /// 点が無いとき表示範囲を作らないことを確認する。
    #[test]
    fn fit_viewport_returns_none_for_no_points() {
        assert_eq!(fit_viewport(&[], 1024, 1024), None);
    }

    /// 1 点だけのとき、ズーム 15 でその点を画像の中心に置くことを確認する。
    ///
    /// bbox が潰れると収まる最大ズームが決まらないため。
    #[test]
    fn fit_viewport_uses_fixed_zoom_for_a_single_point() {
        let viewport = fit_viewport(&[point(0, 35.68, 139.76, None)], 1024, 1024).unwrap();

        assert_eq!(viewport.zoom, 15);
        let (x, y) = viewport.project(35.68, 139.76);
        assert!((x - 512.0).abs() < 0.5);
        assert!((y - 512.0).abs() < 0.5);
    }

    /// 終日ほぼ静止していた (bbox が 200 m 未満の) とき、ズーム 15 に固定されることを確認する。
    #[test]
    fn fit_viewport_uses_fixed_zoom_when_span_is_small() {
        let points = vec![
            point(0, 35.6800, 139.7600, None),
            point(1, 35.6805, 139.7605, None),
        ];

        let viewport = fit_viewport(&points, 1024, 1024).unwrap();

        assert_eq!(viewport.zoom, 15);
    }

    /// 東京駅と新宿駅 (東西に約 6 km) が余白を除いた範囲に収まる最大のズーム 14 が選ばれることを確認する。
    #[test]
    fn fit_viewport_picks_largest_zoom_that_fits() {
        let points = vec![
            point(0, 35.681236, 139.767125, None),
            point(30, 35.690921, 139.700258, None),
        ];

        let viewport = fit_viewport(&points, 1024, 1024).unwrap();

        assert_eq!(viewport.zoom, 14);
        for p in &points {
            let (x, y) = viewport.project(p.lat, p.lon);
            assert!((PADDING_PX as f32..=(1024.0 - PADDING_PX as f32)).contains(&x));
            assert!((PADDING_PX as f32..=(1024.0 - PADDING_PX as f32)).contains(&y));
        }
    }

    /// 表示範囲を覆うタイルが、画像内の貼り付け位置とともに列挙されることを確認する。
    #[test]
    fn tiles_cover_the_viewport_with_offsets() {
        let viewport = Viewport {
            zoom: 1,
            left: 0.0,
            top: 0.0,
            width: 512,
            height: 512,
        };

        let tiles = viewport.tiles();

        assert_eq!(
            tiles,
            vec![
                TilePlacement {
                    z: 1,
                    x: 0,
                    y: 0,
                    offset_x: 0,
                    offset_y: 0
                },
                TilePlacement {
                    z: 1,
                    x: 1,
                    y: 0,
                    offset_x: 256,
                    offset_y: 0
                },
                TilePlacement {
                    z: 1,
                    x: 0,
                    y: 1,
                    offset_x: 0,
                    offset_y: 256
                },
                TilePlacement {
                    z: 1,
                    x: 1,
                    y: 1,
                    offset_x: 256,
                    offset_y: 256
                },
            ]
        );
    }

    /// 経度方向に世界の端をまたぐとき、タイルの x が折り返されることを確認する。
    #[test]
    fn tiles_wrap_horizontally_across_the_antimeridian() {
        let viewport = Viewport {
            zoom: 1,
            left: -128.0,
            top: 0.0,
            width: 256,
            height: 256,
        };

        let tiles = viewport.tiles();

        assert_eq!(
            tiles,
            vec![
                TilePlacement {
                    z: 1,
                    x: 1,
                    y: 0,
                    offset_x: -128,
                    offset_y: 0
                },
                TilePlacement {
                    z: 1,
                    x: 0,
                    y: 0,
                    offset_x: 128,
                    offset_y: 0
                },
            ]
        );
    }

    /// 世界の上端より上にはみ出した部分のタイルを要求しないことを確認する。
    #[test]
    fn tiles_skip_rows_outside_the_world() {
        let viewport = Viewport {
            zoom: 0,
            left: 0.0,
            top: -256.0,
            width: 256,
            height: 512,
        };

        let tiles = viewport.tiles();

        assert_eq!(
            tiles,
            vec![TilePlacement {
                z: 0,
                x: 0,
                y: 0,
                offset_x: 0,
                offset_y: 256
            }]
        );
    }
}
