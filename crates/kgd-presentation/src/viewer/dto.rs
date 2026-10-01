//! ビューアの API の入出力の型。
//!
//! 画面側の valibot のスキーマはこの型から生成する。型を変えたら `just gen-api` で
//! `web/src/api/schema.json` と `web/src/api/schema.gen.ts` を作り直す。

use chrono::{DateTime, NaiveDate, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// `GET /viewer/api/calendar` の応答。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub(super) struct CalendarResponse {
    /// 暦日を区切るタイムゾーン (IANA 名)。画面はこの暦で「今日」を決める
    pub timezone: String,
}

/// `GET /viewer/api/history` のクエリ。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, JsonSchema)]
pub(super) struct HistoryQuery {
    /// 開始日 (含む)
    pub from: NaiveDate,
    /// 終了日 (含む)
    pub to: NaiveDate,
}

/// `GET /viewer/api/history` の応答。
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub(super) struct HistoryResponse {
    /// 対象の期間
    pub range: HistoryRange,
    /// 期間全体の集計 (日ごとの集計の和)
    pub total: HistorySummary,
    /// 日ごとの集計。期間のすべての日を日付順に並べる
    pub days: Vec<DailySummary>,
    /// 地図に描く軌跡 (GeoJSON の FeatureCollection)
    pub track: Track,
    /// 軌跡の間引きの情報
    pub track_meta: TrackMeta,
}

/// 対象の期間。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub(super) struct HistoryRange {
    /// 開始日 (含む)
    pub from: NaiveDate,
    /// 終了日 (含む)
    pub to: NaiveDate,
    /// 暦日を区切ったタイムゾーン (IANA 名)
    pub timezone: String,
}

/// 位置ログの集計。
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub(super) struct HistorySummary {
    /// 移動距離の合計 (メートル)
    pub distance_m: f64,
    /// 移動種別ごとの距離 (メートル)
    pub distance_by_activity: DistanceByActivity,
    /// 移動していた時間 (秒)
    pub moving_s: i64,
    /// 静止していた時間 (秒)
    pub stationary_s: i64,
    /// 集計に使った点の数
    pub point_count: usize,
    /// 精度不足で除外した点の数
    pub excluded_count: usize,
    /// 最初の記録時刻。点が無ければ null
    pub first_at: Option<DateTime<Utc>>,
    /// 最後の記録時刻。点が無ければ null
    pub last_at: Option<DateTime<Utc>>,
}

/// 移動種別ごとの距離 (メートル)。静止は距離を持たないため含めない。
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub(super) struct DistanceByActivity {
    /// 徒歩
    pub walking: f64,
    /// 自転車
    pub cycling: f64,
    /// 車などの乗り物
    pub automotive: f64,
    /// 移動種別が不明
    pub unknown: f64,
}

/// 1 日ぶんの集計。
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub(super) struct DailySummary {
    /// 暦日
    pub date: NaiveDate,
    /// その日の集計
    pub summary: HistorySummary,
}

/// 地図に描く軌跡 (GeoJSON の FeatureCollection)。
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub(super) struct Track {
    /// GeoJSON の種別
    #[serde(rename = "type")]
    pub kind: FeatureCollectionType,
    /// 移動種別が続く区間ごとの線
    pub features: Vec<TrackFeature>,
}

/// GeoJSON の FeatureCollection の種別。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
pub(super) enum FeatureCollectionType {
    /// FeatureCollection
    FeatureCollection,
}

/// 移動種別が続く 1 区間の線 (GeoJSON の Feature)。
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub(super) struct TrackFeature {
    /// GeoJSON の種別
    #[serde(rename = "type")]
    pub kind: FeatureType,
    /// 区間の属性
    pub properties: TrackProperties,
    /// 区間の線
    pub geometry: LineString,
}

/// GeoJSON の Feature の種別。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
pub(super) enum FeatureType {
    /// Feature
    Feature,
}

/// 区間の属性。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
pub(super) struct TrackProperties {
    /// 区間の移動種別
    pub activity: ActivityKind,
}

/// 移動種別。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(super) enum ActivityKind {
    /// 徒歩
    Walking,
    /// 自転車
    Cycling,
    /// 車などの乗り物
    Automotive,
    /// 静止
    Stationary,
    /// 不明
    Unknown,
}

/// GeoJSON の LineString。
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub(super) struct LineString {
    /// GeoJSON の種別
    #[serde(rename = "type")]
    pub kind: LineStringType,
    /// `[経度, 緯度]` の列
    pub coordinates: Vec<[f64; 2]>,
}

/// GeoJSON の LineString の種別。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
pub(super) enum LineStringType {
    /// LineString
    LineString,
}

/// 軌跡の間引きの情報。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
pub(super) struct TrackMeta {
    /// 間引く前の点数 (区間の境目の点は両方の区間で数える)
    pub original_points: usize,
    /// 間引いた後の点数 (数え方は `original_points` と同じ)
    pub returned_points: usize,
    /// 間引いたかどうか
    pub simplified: bool,
}

/// エラーの応答。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub(super) struct ErrorResponse {
    /// エラーの理由
    pub error: String,
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};

    use super::*;

    /// 画面側へ渡す API の型をまとめた、スキーマ生成専用の型。
    ///
    /// JSON Schema の `$defs` に各型を並べ、ルートからそれらを参照させるために使う。
    #[allow(dead_code)] // スキーマを作るためだけの型で、値は作らない
    #[derive(JsonSchema)]
    struct ViewerApi {
        /// `GET /viewer/api/calendar` の応答
        calendar_response: CalendarResponse,
        /// `GET /viewer/api/history` のクエリ
        history_query: HistoryQuery,
        /// `GET /viewer/api/history` の応答
        history_response: HistoryResponse,
        /// エラーの応答
        error_response: ErrorResponse,
    }

    /// コミット済みのスキーマのパス。
    fn schema_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../web/src/api/schema.json")
    }

    /// API の型から作った JSON Schema が、コミット済みの `web/src/api/schema.json` と一致することを確認する。
    ///
    /// 型を変えたのに画面側のスキーマを作り直し忘れると、画面が応答の検証に失敗するため。
    /// 環境変数 `UPDATE_API_SCHEMA` を付けて実行したときだけ、ファイルを書き換える。
    #[test]
    fn api_schema_matches_committed_file() {
        let schema = schemars::schema_for!(ViewerApi);
        let generated = serde_json::to_string_pretty(&schema).unwrap() + "\n";

        if std::env::var_os("UPDATE_API_SCHEMA").is_some() {
            fs::create_dir_all(schema_path().parent().unwrap()).unwrap();
            fs::write(schema_path(), &generated).unwrap();
            return;
        }

        let committed = fs::read_to_string(schema_path()).unwrap_or_default();
        assert!(
            committed == generated,
            "web/src/api/schema.json is stale. Run `just gen-api` to regenerate it."
        );
    }
}
