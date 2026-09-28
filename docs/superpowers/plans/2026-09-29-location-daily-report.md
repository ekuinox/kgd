# 位置ログの日次レポートと日報への投稿 実装計画

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 日報日ごとの位置ログを地図画像と集計値にまとめ、その日の日報スレッドと Notion ページへ自動投稿する。任意の日のレポートを `/location report` で本人だけに見える返信として確認できるようにする。

**Architecture:** レポートを作る `BuildLocationReportUseCase` と、bot の投稿を日報へ載せる汎用の `PublishDiaryPostUseCase` を分ける。判断を伴う計算 (外れ値の除去、区間分割、集計、投影、ズーム決定、文言) は kgd-domain の純粋関数に置き、描画は `MapRenderer` ポートの先 (tiny-skia) に閉じ込める。冪等性は `diary_posts` テーブルに段ごとの完了時刻を記録して担保する。

**Tech Stack:** Rust 2024 / tiny-skia 0.12 / reqwest 0.12 / sqlx 0.8 (PostgreSQL) / serenity 0.12 / chrono 0.4 / mockall 0.13 / axum 0.8 (テストのスタブサーバー)

**Spec:** `docs/superpowers/specs/2026-09-29-location-daily-report-design.md`

## Global Constraints

- 依存方向は kgd-domain ← kgd-application ← kgd-infrastructure / kgd-presentation ← kgd (binary)。kgd-domain と kgd-application に IO ライブラリ (serenity / sqlx / reqwest / tokio の IO / axum / tiny-skia) を入れてはならない
- `cargo test -p kgd-application` が sqlx / serenity / libheif をビルドせずに通る性質を保つ
- 新規依存は workspace の `[workspace.dependencies]` に定義し、各 crate では `tiny-skia.workspace = true` の形で参照する。値が 1 つのときはインラインテーブルを使わない
- 追加してよいライセンスは MIT / Apache-2.0 / Apache-2.0 WITH LLVM-exception / BSD-2-Clause / BSD-3-Clause / ISC / Zlib / Unicode-3.0 / CDLA-Permissive-2.0 のみ (`deny.toml`)
- 判断・変換のロジックは kgd-domain の純粋関数とし、同一ファイル内の `#[cfg(test)] mod tests` でテストする。ユースケースはポートのモック (mockall) でテストする
- 非同期テストは `#[tokio::test]`
- テスト関数名は `<対象>_<動詞>_<条件>` のスネークケース英文。doc コメントは日本語で「何を確認するか」を 1 文、必要なら空行を挟んで「なぜそうあるべきか」
- 構造体・列挙子・フィールドには doc コメントを付ける
- `use` は std / 外部クレート / `crate` / `super` のブロックに分けて書く
- 日報日の 1 日は `DiaryCalendar` (タイムゾーンと `day_start_hour`) で決める。範囲は開始を含み終了を含まない半開区間
- 集計で時間と距離を積算しない間隔のしきい値は 15 分。静止区間は距離に積算しない
- ズームは 3 から 17。bbox の長辺が 200 m 未満ならズーム 15 で中心に置く。画像の余白は 32 px
- 定時投稿のキーは `location-report:YYYY-MM-DD`
- 地図タイルの URL は `https://tile.openstreetmap.org/{z}/{x}/{y}.png`、User-Agent は `kgd/<version> (+https://github.com/ekuinox/kgd)`
- 本文の末尾に `© OpenStreetMap contributors` を入れる
- ビルドとテストは nix を外した PATH で行う: `export PATH="$HOME/.cargo/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin"`。`just` は `~/.nix-profile/bin/just` をフルパスで呼ぶ。worktree で初めてビルドする前に `git submodule update --init crates/heif-sys/libheif` を実行する
- コミット前に `~/.nix-profile/bin/just validate` (fmt / check / clippy -D warnings) を通す。最後のタスクで `~/.nix-profile/bin/just ci` を通す
- コミットメッセージは日本語。末尾に `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>` を付ける
- コミットの署名の扱い (署名鍵 `~/.ssh/id_ed25519.pub` がこの環境に無い) は、実行を始める前に利用者と決めたとおりにする

## Review Focus

- タイルサーバーが 404 を返す、応答しない、PNG でないものを返す: レポートは灰色の背景で最後まで描画される (Task 10 のテスト `render_fills_failed_tiles_with_gray_and_continues`、`render_fills_undecodable_tiles_with_gray`)
- 日報日の点がすべて精度不足で除外される: 画像を作らず「記録なし」として扱う (Task 11 のテスト `build_returns_no_image_when_all_points_are_inaccurate`)
- `/location report date:` に不正な書式や未来の日付が渡される: パニックせず、本人に理由を返す (Task 15 のテスト `resolve_report_date_rejects_*`)
- スレッドへの投稿には成功したが、クローズへ戻すのに失敗する: 投稿済みとして記録し、次の tick で二重投稿しない (Task 12 のテスト `publish_records_thread_post_even_when_reclose_fails`)
- 日報日の境界ちょうどの点: 開始時刻の点は含み、終了時刻の点は含まない。ユースケースは `day_range` の値をそのまま渡し、SQL は `tst >= $1 AND tst < $2` とする (Task 11 のテスト `build_queries_the_half_open_diary_day_range`)

---

## ファイル構成

| ファイル | 責務 |
|---|---|
| `crates/kgd-domain/src/diary.rs` (変更) | `DiaryCalendar::start_of` の公開、`day_range`、`previous_date` |
| `crates/kgd-domain/src/location/mod.rs` (新規) | 位置ログ関連モジュールの束ね |
| `crates/kgd-domain/src/location/track.rs` (新規) | `Activity`、`TrackPoint`、外れ値の除去、区間分割 |
| `crates/kgd-domain/src/location/summary.rs` (新規) | ハバサイン距離、`LocationSummary` と集計 |
| `crates/kgd-domain/src/location/projection.rs` (新規) | Web メルカトル投影、`Viewport`、ズーム決定、タイルの配置 |
| `crates/kgd-domain/src/location/report_text.rs` (新規) | レポートの文言 (本文と embed で共有) |
| `crates/kgd-domain/src/diary_post.rs` (新規) | `DiaryPost`、`DiaryPostImage`、`DiaryPostRecord` |
| `crates/kgd-domain/src/blocks.rs` (変更) | 段落ブロック JSON |
| `crates/kgd-domain/src/url_rules/mod.rs` (変更) | rich_text 分割関数を crate 内へ公開 |
| `crates/kgd-application/src/ports/location.rs` (変更) | `locations_between` |
| `crates/kgd-application/src/ports/diary_post.rs` (新規) | `DiaryPostRepository` |
| `crates/kgd-application/src/ports/map.rs` (新規) | `MapRenderer` |
| `crates/kgd-application/src/ports/discord.rs` (変更) | `send_text_with_images` |
| `crates/kgd-application/src/build_location_report.rs` (新規) | `BuildLocationReportUseCase`、`LocationReport` |
| `crates/kgd-application/src/publish_diary_post.rs` (新規) | `PublishDiaryPostUseCase`、`PublishOutcome` |
| `crates/kgd-application/src/daily_location_report.rs` (新規) | `DailyLocationReportJob` |
| `crates/kgd-infrastructure/migrations/20260929_000001_index_owntracks_messages_tst.sql` (新規) | 時刻範囲の検索用インデックス |
| `crates/kgd-infrastructure/migrations/20260929_000002_create_diary_posts.sql` (新規) | `diary_posts` テーブル |
| `crates/kgd-infrastructure/src/location_store/mod.rs` (変更) | `locations_between` の実装 |
| `crates/kgd-infrastructure/src/diary_post_store.rs` (新規) | `DiaryPostStore` |
| `crates/kgd-infrastructure/src/discord_gateway/mod.rs` (変更) | `send_text_with_images` の実装 |
| `crates/kgd-infrastructure/src/map_renderer/mod.rs` (新規) | `TileMapRenderer` |
| `crates/kgd-infrastructure/src/map_renderer/tests.rs` (新規) | スタブのタイルサーバーを使ったテスト |
| `crates/kgd-presentation/src/presenter/location.rs` (新規) | レポートの embed、日付入力の解釈 |
| `crates/kgd-presentation/src/discord/location_commands.rs` (新規) | `/location report` の処理 |
| `crates/kgd/src/config/mod.rs` / `defaults.rs` (変更) | `[location]` の設定項目 |
| `crates/kgd/src/bootstrap.rs` (変更) | 配線 |
| `config.example.toml` / `compose.yml` / `Dockerfile` (変更) | 設定例、タイルキャッシュのボリューム |
| `docs/architecture.md` / `docs/adr/` (変更・新規) | ドキュメント |

---

### Task 1: 日報日の範囲と前日

**Files:**
- Modify: `crates/kgd-domain/src/diary.rs`
- Test: `crates/kgd-domain/src/diary.rs` (既存の `mod tests`)

**Interfaces:**
- Consumes: なし
- Produces:
  - `impl DiaryCalendar { pub fn start_of(&self, date: NaiveDate) -> DateTime<Utc> }` (既存の private 関数を公開する。日報エントリの `date` 列と同じ表現)
  - `impl DiaryCalendar { pub fn day_range(&self, date: NaiveDate) -> (DateTime<Utc>, DateTime<Utc>) }`
  - `impl DiaryCalendar { pub fn previous_date(&self, now: DateTime<Utc>) -> NaiveDate }`

- [ ] **Step 1: 失敗するテストを書く**

`crates/kgd-domain/src/diary.rs` の `mod tests` の末尾に追加する (`jst` ヘルパーは既存)。

```rust
    /// 日報日の範囲が、その日の day_start_hour から翌日の day_start_hour までになることを確認する。
    ///
    /// 位置ログのレポートは日報と同じ区切りで集計するため。
    #[test]
    fn day_range_spans_from_day_start_hour_to_next_day_start_hour() {
        let calendar = DiaryCalendar::new(chrono_tz::Asia::Tokyo, 8);
        let date = NaiveDate::from_ymd_opt(2026, 9, 28).unwrap();

        let (start, end) = calendar.day_range(date);

        assert_eq!(start, jst(2026, 9, 28, 8, 0));
        assert_eq!(end, jst(2026, 9, 29, 8, 0));
    }

    /// day_start_hour が 0 のとき、日報日の範囲が暦日と一致することを確認する。
    #[test]
    fn day_range_matches_calendar_day_when_day_starts_at_midnight() {
        let calendar = DiaryCalendar::new(chrono_tz::Asia::Tokyo, 0);
        let date = NaiveDate::from_ymd_opt(2026, 9, 28).unwrap();

        let (start, end) = calendar.day_range(date);

        assert_eq!(start, jst(2026, 9, 28, 0, 0));
        assert_eq!(end, jst(2026, 9, 29, 0, 0));
    }

    /// day_start_hour より前は 2 日前、以降は 1 日前が「前の日報日」になることを確認する。
    ///
    /// 定時レポートは日報日が切り替わってから前日分を投稿するため。
    #[test]
    fn previous_date_switches_at_day_start_hour() {
        let calendar = DiaryCalendar::new(chrono_tz::Asia::Tokyo, 8);

        assert_eq!(
            calendar.previous_date(jst(2026, 9, 29, 7, 59)),
            NaiveDate::from_ymd_opt(2026, 9, 27).unwrap()
        );
        assert_eq!(
            calendar.previous_date(jst(2026, 9, 29, 8, 0)),
            NaiveDate::from_ymd_opt(2026, 9, 28).unwrap()
        );
    }

    /// start_of が日報エントリの date 列と同じ表現 (暦日 0 時) を返すことを確認する。
    #[test]
    fn start_of_returns_local_midnight_of_the_date() {
        let calendar = DiaryCalendar::new(chrono_tz::Asia::Tokyo, 8);
        let date = NaiveDate::from_ymd_opt(2026, 9, 28).unwrap();

        assert_eq!(calendar.start_of(date), jst(2026, 9, 28, 0, 0));
    }
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cargo test -p kgd-domain diary::tests`
Expected: FAIL (`no method named day_range` / `previous_date`、`start_of` is private)

- [ ] **Step 3: 実装する**

`start_of` を公開し、2 つのメソッドを `impl DiaryCalendar` に追加する。

```rust
    /// 指定された時刻から見た、1 つ前の日報日を返す。
    ///
    /// 日報日が切り替わった直後に前日分を処理するジョブが使う。
    pub fn previous_date(&self, now: DateTime<Utc>) -> NaiveDate {
        self.local_date(now)
            .pred_opt()
            .expect("diary date underflowed the representable range")
    }

    /// 日報日の開始時刻と終了時刻 (終了は含まない) を UTC で返す。
    ///
    /// 開始は `date` の `day_start_hour`、終了は翌日の `day_start_hour` とする。
    pub fn day_range(&self, date: NaiveDate) -> (DateTime<Utc>, DateTime<Utc>) {
        let next = date
            .succ_opt()
            .expect("diary date overflowed the representable range");
        (self.at_day_start(date), self.at_day_start(next))
    }

    /// 日報日の暦日 0 時をタイムゾーン基準で表した UTC 時刻を返す。
    ///
    /// 日報エントリの `date` 列はこの表現で保存されている。
    pub fn start_of(&self, date: NaiveDate) -> DateTime<Utc> {
        date.and_time(NaiveTime::MIN)
            .and_local_timezone(self.timezone)
            .unwrap()
            .to_utc()
    }

    /// 指定した暦日の `day_start_hour` をタイムゾーン基準で表した UTC 時刻を返す。
    fn at_day_start(&self, date: NaiveDate) -> DateTime<Utc> {
        date.and_hms_opt(self.day_start_hour, 0, 0)
            .expect("day_start_hour is validated to be in 0-23")
            .and_local_timezone(self.timezone)
            .unwrap()
            .to_utc()
    }
```

既存の `fn start_of` は削除して上の公開版に置き換える。

- [ ] **Step 4: テストが通ることを確認する**

Run: `cargo test -p kgd-domain diary::tests`
Expected: PASS

- [ ] **Step 5: コミットする**

```bash
~/.nix-profile/bin/just validate
git add crates/kgd-domain/src/diary.rs
git commit -m "feat(domain): 日報日の範囲と前日を返すメソッドを追加する" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 2: 軌跡の点、外れ値の除去、区間分割

**Files:**
- Create: `crates/kgd-domain/src/location/mod.rs`
- Create: `crates/kgd-domain/src/location/track.rs`
- Modify: `crates/kgd-domain/src/lib.rs`
- Test: `crates/kgd-domain/src/location/track.rs` (同一ファイル内 `mod tests`)

**Interfaces:**
- Consumes: なし
- Produces:
  - `pub enum Activity { Walking, Cycling, Automotive, Stationary }` (`Debug, Clone, Copy, PartialEq, Eq, Hash`)
  - `impl Activity { pub fn from_motion(motion: &str) -> Option<Activity> }`
  - `pub struct TrackPoint { pub at: DateTime<Utc>, pub lat: f64, pub lon: f64, pub accuracy_m: Option<i32>, pub activity: Option<Activity> }` (`Debug, Clone, PartialEq`)
  - `pub fn filter_accurate(points: Vec<TrackPoint>, max_accuracy_m: i32) -> (Vec<TrackPoint>, usize)` (残した点と除外した件数)
  - `pub struct TrackSegment { pub activity: Option<Activity>, pub points: Vec<TrackPoint> }` (`Debug, Clone, PartialEq`)
  - `pub fn split_segments(points: &[TrackPoint]) -> Vec<TrackSegment>`
  - いずれも `kgd_domain::` 直下から参照できるよう `lib.rs` で再公開する

- [ ] **Step 1: モジュールを用意して失敗するテストを書く**

`crates/kgd-domain/src/location/mod.rs`:

```rust
//! 位置ログ (OwnTracks) の集計と描画準備のための純粋ロジック。

mod track;

pub use track::{Activity, TrackPoint, TrackSegment, filter_accurate, split_segments};
```

`crates/kgd-domain/src/lib.rs` に `mod location;` を追加し、再公開する。

```rust
pub use location::{Activity, TrackPoint, TrackSegment, filter_accurate, split_segments};
```

`crates/kgd-domain/src/location/track.rs` にテストを書く。

```rust
//! 軌跡の点と、外れ値の除去・移動種別による区間分割。

#[cfg(test)]
pub(crate) mod tests {
    use chrono::{TimeZone as _, Utc};

    use super::*;

    /// テスト用の点を作る。`minute` は 2026-09-28 00:00 UTC からの分。
    pub(crate) fn point(minute: i64, lat: f64, lon: f64, activity: Option<Activity>) -> TrackPoint {
        TrackPoint {
            at: Utc.with_ymd_and_hms(2026, 9, 28, 0, 0, 0).unwrap()
                + chrono::TimeDelta::minutes(minute),
            lat,
            lon,
            accuracy_m: Some(10),
            activity,
        }
    }

    /// OwnTracks の motionactivities の値が移動種別へ対応づけられることを確認する。
    ///
    /// running は徒歩として扱い、unknown や未知の値は欠損として扱う。
    #[test]
    fn from_motion_maps_known_values_and_treats_others_as_missing() {
        assert_eq!(Activity::from_motion("walking"), Some(Activity::Walking));
        assert_eq!(Activity::from_motion("running"), Some(Activity::Walking));
        assert_eq!(Activity::from_motion("cycling"), Some(Activity::Cycling));
        assert_eq!(Activity::from_motion("automotive"), Some(Activity::Automotive));
        assert_eq!(Activity::from_motion("stationary"), Some(Activity::Stationary));
        assert_eq!(Activity::from_motion("unknown"), None);
        assert_eq!(Activity::from_motion("flying"), None);
    }

    /// 精度がしきい値を超える点だけが除外され、その件数が返ることを確認する。
    ///
    /// 実測データに acc = 1414 の点があり、残すと軌跡が大きく飛ぶため。
    #[test]
    fn filter_accurate_drops_points_above_threshold() {
        let mut far = point(1, 35.0, 139.0, None);
        far.accuracy_m = Some(1414);
        let mut edge = point(2, 35.0, 139.0, None);
        edge.accuracy_m = Some(200);
        let mut unknown = point(3, 35.0, 139.0, None);
        unknown.accuracy_m = None;

        let (kept, excluded) = filter_accurate(vec![far, edge.clone(), unknown.clone()], 200);

        assert_eq!(kept, vec![edge, unknown]);
        assert_eq!(excluded, 1);
    }

    /// 空の点列からは区間が生まれないことを確認する。
    #[test]
    fn split_segments_returns_nothing_for_empty_input() {
        assert!(split_segments(&[]).is_empty());
    }

    /// 移動種別が変わるたびに区間が分かれ、新しい区間は直前の区間の最後の点から始まることを確認する。
    ///
    /// 区間の境目で線が途切れないようにするため。
    #[test]
    fn split_segments_starts_each_segment_from_previous_last_point() {
        let walk1 = point(0, 35.0, 139.0, Some(Activity::Walking));
        let walk2 = point(1, 35.001, 139.0, Some(Activity::Walking));
        let car1 = point(2, 35.002, 139.0, Some(Activity::Automotive));
        let car2 = point(3, 35.003, 139.0, Some(Activity::Automotive));

        let segments = split_segments(&[walk1.clone(), walk2.clone(), car1.clone(), car2.clone()]);

        assert_eq!(
            segments,
            vec![
                TrackSegment {
                    activity: Some(Activity::Walking),
                    points: vec![walk1, walk2.clone()],
                },
                TrackSegment {
                    activity: Some(Activity::Automotive),
                    points: vec![walk2, car1, car2],
                },
            ]
        );
    }

    /// 欠損 (None) も 1 つの種別として区間になり、補完されないことを確認する。
    #[test]
    fn split_segments_keeps_missing_activity_as_its_own_segment() {
        let walk = point(0, 35.0, 139.0, Some(Activity::Walking));
        let missing1 = point(1, 35.001, 139.0, None);
        let missing2 = point(2, 35.002, 139.0, None);

        let segments = split_segments(&[walk, missing1, missing2]);

        let activities: Vec<Option<Activity>> = segments.iter().map(|s| s.activity).collect();
        assert_eq!(activities, vec![Some(Activity::Walking), None]);
        assert_eq!(segments[1].points.len(), 3);
    }

    /// すべて欠損の点列が 1 つの区間になることを確認する。
    #[test]
    fn split_segments_groups_all_missing_points_into_one_segment() {
        let points: Vec<TrackPoint> = (0..3).map(|m| point(m, 35.0, 139.0, None)).collect();

        let segments = split_segments(&points);

        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].activity, None);
        assert_eq!(segments[0].points.len(), 3);
    }

}
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cargo test -p kgd-domain location::track`
Expected: FAIL (`cannot find type Activity` など)

- [ ] **Step 3: 実装する**

`track.rs` の `#[cfg(test)]` より上に書く。

```rust
use chrono::{DateTime, Utc};

/// 移動種別 (OwnTracks の motionactivities の先頭要素)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Activity {
    /// 徒歩 (running を含む)
    Walking,
    /// 自転車
    Cycling,
    /// 車などの乗り物
    Automotive,
    /// 静止
    Stationary,
}

impl Activity {
    /// motionactivities の文字列から移動種別を得る。
    ///
    /// `unknown` と未知の値は欠損 (`None`) として扱う。
    pub fn from_motion(motion: &str) -> Option<Activity> {
        match motion {
            "walking" | "running" => Some(Activity::Walking),
            "cycling" => Some(Activity::Cycling),
            "automotive" => Some(Activity::Automotive),
            "stationary" => Some(Activity::Stationary),
            _ => None,
        }
    }
}

/// 軌跡を構成する 1 点。
#[derive(Debug, Clone, PartialEq)]
pub struct TrackPoint {
    /// 端末が位置を取得した時刻
    pub at: DateTime<Utc>,
    /// 緯度
    pub lat: f64,
    /// 経度
    pub lon: f64,
    /// 水平精度 (メートル)。不明なら None
    pub accuracy_m: Option<i32>,
    /// 移動種別。欠損なら None
    pub activity: Option<Activity>,
}

/// 同じ移動種別が続く軌跡の区間。
#[derive(Debug, Clone, PartialEq)]
pub struct TrackSegment {
    /// 区間の移動種別。欠損なら None
    pub activity: Option<Activity>,
    /// 区間の点 (2 つ目以降の区間は直前の区間の最後の点から始まる)
    pub points: Vec<TrackPoint>,
}

/// 精度がしきい値を超える点を除き、残した点と除外した件数を返す。
///
/// 精度が不明な点は残す。
pub fn filter_accurate(points: Vec<TrackPoint>, max_accuracy_m: i32) -> (Vec<TrackPoint>, usize) {
    let total = points.len();
    let kept: Vec<TrackPoint> = points
        .into_iter()
        .filter(|point| point.accuracy_m.is_none_or(|acc| acc <= max_accuracy_m))
        .collect();
    let excluded = total - kept.len();
    (kept, excluded)
}

/// 点列を移動種別が続く区間に分ける。
///
/// 欠損は補完せず、`None` を 1 つの種別として扱う。
/// 区間の境目で線が途切れないよう、2 つ目以降の区間は直前の区間の最後の点から始める。
pub fn split_segments(points: &[TrackPoint]) -> Vec<TrackSegment> {
    let mut segments: Vec<TrackSegment> = Vec::new();
    for point in points {
        match segments.last_mut() {
            Some(current) if current.activity == point.activity => {
                current.points.push(point.clone());
            }
            Some(current) => {
                let bridge = current
                    .points
                    .last()
                    .cloned()
                    .expect("segments always hold at least one point");
                segments.push(TrackSegment {
                    activity: point.activity,
                    points: vec![bridge, point.clone()],
                });
            }
            None => segments.push(TrackSegment {
                activity: point.activity,
                points: vec![point.clone()],
            }),
        }
    }
    segments
}
```

- [ ] **Step 4: テストが通ることを確認する**

Run: `cargo test -p kgd-domain location::track`
Expected: PASS

- [ ] **Step 5: コミットする**

```bash
~/.nix-profile/bin/just validate
git add crates/kgd-domain/src/location crates/kgd-domain/src/lib.rs
git commit -m "feat(domain): 軌跡の点と外れ値の除去、移動種別による区間分割を追加する" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 3: 距離と時間の集計

**Files:**
- Create: `crates/kgd-domain/src/location/summary.rs`
- Modify: `crates/kgd-domain/src/location/mod.rs`、`crates/kgd-domain/src/lib.rs`
- Test: `crates/kgd-domain/src/location/summary.rs`

**Interfaces:**
- Consumes: Task 2 の `Activity`、`TrackPoint`、テストヘルパー `crate::location::track::tests::point(minute, lat, lon, activity)`
- Produces:
  - `pub fn haversine_m(a: (f64, f64), b: (f64, f64)) -> f64` (引数は `(lat, lon)`、戻り値はメートル)
  - `pub struct LocationSummary { pub point_count: usize, pub excluded_count: usize, pub distance_m: f64, pub distance_by_activity: Vec<(Option<Activity>, f64)>, pub moving: TimeDelta, pub stationary: TimeDelta, pub first_at: Option<DateTime<Utc>>, pub last_at: Option<DateTime<Utc>> }` (`Debug, Clone, PartialEq`)
  - `pub fn summarize(points: &[TrackPoint], excluded_count: usize) -> LocationSummary`
  - `distance_by_activity` は距離が 0 より大きい種別だけを、徒歩、自転車、車、欠損の順で並べる

- [ ] **Step 1: 失敗するテストを書く**

`crates/kgd-domain/src/location/mod.rs` に `mod summary;` と `pub use summary::{LocationSummary, haversine_m, summarize};` を追加し、`lib.rs` の再公開にも `LocationSummary, haversine_m, summarize` を足す。

`summary.rs`:

```rust
//! 軌跡の距離と時間の集計。

#[cfg(test)]
mod tests {
    use chrono::TimeDelta;

    use crate::location::track::tests::point;

    use super::*;

    /// 経度 0 の子午線上で緯度 1 度ぶん離れた 2 点の距離が、地球半径から求めた値になることを確認する。
    #[test]
    fn haversine_m_matches_one_degree_of_latitude() {
        let distance = haversine_m((0.0, 0.0), (1.0, 0.0));

        let expected = EARTH_RADIUS_M * std::f64::consts::PI / 180.0;
        assert!((distance - expected).abs() < 1e-6, "distance = {distance}");
    }

    /// 点が無いとき、すべて 0 で時刻も無い集計になることを確認する。
    #[test]
    fn summarize_returns_zero_for_empty_points() {
        let summary = summarize(&[], 3);

        assert_eq!(summary.point_count, 0);
        assert_eq!(summary.excluded_count, 3);
        assert_eq!(summary.distance_m, 0.0);
        assert!(summary.distance_by_activity.is_empty());
        assert_eq!(summary.moving, TimeDelta::zero());
        assert_eq!(summary.stationary, TimeDelta::zero());
        assert_eq!(summary.first_at, None);
        assert_eq!(summary.last_at, None);
    }

    /// 間隔は前の点の種別に割り当てられ、静止は時間だけ、移動は時間と距離が積算されることを確認する。
    ///
    /// 静止中の GPS の揺れを移動距離に数えないため。
    #[test]
    fn summarize_assigns_intervals_to_the_earlier_point_activity() {
        let points = vec![
            point(0, 0.0, 0.0, Some(Activity::Stationary)),
            point(10, 0.001, 0.0, Some(Activity::Walking)),
            point(20, 0.002, 0.0, Some(Activity::Walking)),
        ];

        let summary = summarize(&points, 0);

        let step = haversine_m((0.001, 0.0), (0.002, 0.0));
        assert_eq!(summary.stationary, TimeDelta::minutes(10));
        assert_eq!(summary.moving, TimeDelta::minutes(10));
        assert!((summary.distance_m - step).abs() < 1e-9);
        assert_eq!(summary.distance_by_activity.len(), 1);
        assert_eq!(summary.distance_by_activity[0].0, Some(Activity::Walking));
        assert_eq!(summary.point_count, 3);
        assert_eq!(summary.first_at, Some(points[0].at));
        assert_eq!(summary.last_at, Some(points[2].at));
    }

    /// 15 分を超える間隔は時間にも距離にも数えないことを確認する。
    ///
    /// 圏外などで途切れていた時間を移動や静止として計上しないため。
    #[test]
    fn summarize_skips_intervals_longer_than_fifteen_minutes() {
        let points = vec![
            point(0, 0.0, 0.0, Some(Activity::Automotive)),
            point(15, 0.01, 0.0, Some(Activity::Automotive)),
            point(31, 0.5, 0.0, Some(Activity::Automotive)),
        ];

        let summary = summarize(&points, 0);

        assert_eq!(summary.moving, TimeDelta::minutes(15));
        let first_leg = haversine_m((0.0, 0.0), (0.01, 0.0));
        assert!((summary.distance_m - first_leg).abs() < 1e-9);
    }

    /// 移動種別ごとの距離が、徒歩、自転車、車、欠損の順で並ぶことを確認する。
    #[test]
    fn summarize_orders_distance_breakdown_by_activity() {
        let points = vec![
            point(0, 0.0, 0.0, None),
            point(1, 0.001, 0.0, Some(Activity::Automotive)),
            point(2, 0.002, 0.0, Some(Activity::Walking)),
            point(3, 0.003, 0.0, Some(Activity::Walking)),
        ];

        let summary = summarize(&points, 0);

        let order: Vec<Option<Activity>> =
            summary.distance_by_activity.iter().map(|(a, _)| *a).collect();
        assert_eq!(
            order,
            vec![Some(Activity::Walking), Some(Activity::Automotive), None]
        );
    }
}
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cargo test -p kgd-domain location::summary`
Expected: FAIL (`cannot find function haversine_m` など)

- [ ] **Step 3: 実装する**

```rust
use chrono::{DateTime, TimeDelta, Utc};

use super::track::{Activity, TrackPoint};

/// ハバサイン距離に使う地球の半径 (メートル)。
const EARTH_RADIUS_M: f64 = 6_371_000.0;

/// 時間と距離を積算する間隔の上限 (秒)。これを超える間隔は途切れとみなす。
const MAX_GAP_SECONDS: i64 = 15 * 60;

/// 距離の内訳を並べる順序。
const BREAKDOWN_ORDER: [Option<Activity>; 4] = [
    Some(Activity::Walking),
    Some(Activity::Cycling),
    Some(Activity::Automotive),
    None,
];

/// 1 日ぶんの軌跡の集計値。
#[derive(Debug, Clone, PartialEq)]
pub struct LocationSummary {
    /// 集計に使った点の数
    pub point_count: usize,
    /// 精度不足で除外した点の数
    pub excluded_count: usize,
    /// 移動距離の合計 (メートル)
    pub distance_m: f64,
    /// 移動種別ごとの距離 (メートル)。距離が 0 の種別は含まない
    pub distance_by_activity: Vec<(Option<Activity>, f64)>,
    /// 移動していた時間
    pub moving: TimeDelta,
    /// 静止していた時間
    pub stationary: TimeDelta,
    /// 最初の記録時刻
    pub first_at: Option<DateTime<Utc>>,
    /// 最後の記録時刻
    pub last_at: Option<DateTime<Utc>>,
}

/// 2 点間の大円距離をメートルで返す。引数は `(緯度, 経度)`。
pub fn haversine_m(a: (f64, f64), b: (f64, f64)) -> f64 {
    let (lat1, lon1) = (a.0.to_radians(), a.1.to_radians());
    let (lat2, lon2) = (b.0.to_radians(), b.1.to_radians());
    let h = ((lat2 - lat1) / 2.0).sin().powi(2)
        + lat1.cos() * lat2.cos() * ((lon2 - lon1) / 2.0).sin().powi(2);
    2.0 * EARTH_RADIUS_M * h.sqrt().asin()
}

/// 時刻順に並んだ点列を集計する。
///
/// 隣り合う 2 点の間隔を前の点の移動種別に割り当てる。静止は時間だけを、
/// それ以外 (欠損を含む) は時間と距離を積算する。15 分を超える間隔は数えない。
pub fn summarize(points: &[TrackPoint], excluded_count: usize) -> LocationSummary {
    let mut moving = TimeDelta::zero();
    let mut stationary = TimeDelta::zero();
    let mut distances: Vec<(Option<Activity>, f64)> =
        BREAKDOWN_ORDER.iter().map(|activity| (*activity, 0.0)).collect();

    for pair in points.windows(2) {
        let (from, to) = (&pair[0], &pair[1]);
        let gap = to.at - from.at;
        if gap < TimeDelta::zero() || gap.num_seconds() > MAX_GAP_SECONDS {
            continue;
        }
        if from.activity == Some(Activity::Stationary) {
            stationary += gap;
            continue;
        }
        moving += gap;
        let step = haversine_m((from.lat, from.lon), (to.lat, to.lon));
        if let Some(entry) = distances.iter_mut().find(|(a, _)| *a == from.activity) {
            entry.1 += step;
        }
    }

    let distance_m: f64 = distances.iter().map(|(_, d)| d).sum();
    distances.retain(|(_, d)| *d > 0.0);

    LocationSummary {
        point_count: points.len(),
        excluded_count,
        distance_m,
        distance_by_activity: distances,
        moving,
        stationary,
        first_at: points.first().map(|p| p.at),
        last_at: points.last().map(|p| p.at),
    }
}
```

- [ ] **Step 4: テストが通ることを確認する**

Run: `cargo test -p kgd-domain location::summary`
Expected: PASS

- [ ] **Step 5: コミットする**

```bash
~/.nix-profile/bin/just validate
git add crates/kgd-domain/src/location crates/kgd-domain/src/lib.rs
git commit -m "feat(domain): 軌跡の距離と時間の集計を追加する" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 4: Web メルカトル投影と表示範囲

**Files:**
- Create: `crates/kgd-domain/src/location/projection.rs`
- Modify: `crates/kgd-domain/src/location/mod.rs`、`crates/kgd-domain/src/lib.rs`
- Test: `crates/kgd-domain/src/location/projection.rs`

**Interfaces:**
- Consumes: Task 2 の `TrackPoint`、Task 3 の `haversine_m`、テストヘルパー `point`
- Produces:
  - `pub const TILE_SIZE: u32 = 256;`
  - `pub fn world_pixel(lat: f64, lon: f64, zoom: u8) -> (f64, f64)`
  - `pub struct Viewport { pub zoom: u8, pub left: f64, pub top: f64, pub width: u32, pub height: u32 }` (`Debug, Clone, Copy, PartialEq`)
  - `impl Viewport { pub fn project(&self, lat: f64, lon: f64) -> (f32, f32); pub fn tiles(&self) -> Vec<TilePlacement> }`
  - `pub struct TilePlacement { pub z: u8, pub x: u32, pub y: u32, pub offset_x: i32, pub offset_y: i32 }` (`Debug, Clone, Copy, PartialEq, Eq`)
  - `pub fn fit_viewport(points: &[TrackPoint], width: u32, height: u32) -> Option<Viewport>` (点が無ければ None)

- [ ] **Step 1: 失敗するテストを書く**

`mod.rs` に `mod projection;` と `pub use projection::{TILE_SIZE, TilePlacement, Viewport, fit_viewport, world_pixel};` を追加し、`lib.rs` でも再公開する。

`projection.rs`:

```rust
//! Web メルカトル投影と、地図画像の表示範囲・ズームの決定。

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
                TilePlacement { z: 1, x: 0, y: 0, offset_x: 0, offset_y: 0 },
                TilePlacement { z: 1, x: 1, y: 0, offset_x: 256, offset_y: 0 },
                TilePlacement { z: 1, x: 0, y: 1, offset_x: 0, offset_y: 256 },
                TilePlacement { z: 1, x: 1, y: 1, offset_x: 256, offset_y: 256 },
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
                TilePlacement { z: 1, x: 1, y: 0, offset_x: -128, offset_y: 0 },
                TilePlacement { z: 1, x: 0, y: 0, offset_x: 128, offset_y: 0 },
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
            vec![TilePlacement { z: 0, x: 0, y: 0, offset_x: 0, offset_y: 256 }]
        );
    }
}
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cargo test -p kgd-domain location::projection`
Expected: FAIL (`cannot find function world_pixel` など)

- [ ] **Step 3: 実装する**

```rust
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

    let span_m = haversine_m((south, west), (north, west)).max(haversine_m((south, west), (south, east)));
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
```

- [ ] **Step 4: テストが通ることを確認する**

Run: `cargo test -p kgd-domain location::projection`
Expected: PASS

- [ ] **Step 5: コミットする**

```bash
~/.nix-profile/bin/just validate
git add crates/kgd-domain/src/location crates/kgd-domain/src/lib.rs
git commit -m "feat(domain): Web メルカトル投影と地図の表示範囲の決定を追加する" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 5: レポートの文言

**Files:**
- Create: `crates/kgd-domain/src/location/report_text.rs`
- Modify: `crates/kgd-domain/src/location/mod.rs`、`crates/kgd-domain/src/lib.rs`
- Test: `crates/kgd-domain/src/location/report_text.rs`

**Interfaces:**
- Consumes: Task 2 の `Activity`、Task 3 の `LocationSummary`
- Produces:
  - `pub const OSM_ATTRIBUTION: &str = "© OpenStreetMap contributors";`
  - `pub struct LocationReportText { pub heading: String, pub distance: String, pub durations: String, pub points: String, pub times: String }` (`Debug, Clone, PartialEq, Eq`)
  - `impl LocationReportText { pub fn to_plain_text(&self) -> String }`
  - `pub fn format_location_report(date: NaiveDate, range: (DateTime<Utc>, DateTime<Utc>), summary: &LocationSummary, timezone: Tz) -> LocationReportText`
  - `pub fn format_empty_location_report(date: NaiveDate) -> String`

各フィールドの例 (スラッシュコマンドの embed では `heading` をタイトル、残りをフィールドの値に使う):

| フィールド | 例 |
|---|---|
| `heading` | `位置ログ 2026-09-28 (08:00〜翌 08:00)` |
| `distance` | `12.3 km (徒歩 5.1 km / 車 7.2 km)` |
| `durations` | `移動 1 時間 20 分 / 静止 21 時間 5 分` |
| `points` | `1,234 点 (精度不足で 2 点を除外)` |
| `times` | `最初 08:03 / 最後 07:58` |

- [ ] **Step 1: 失敗するテストを書く**

`mod.rs` に `mod report_text;` と `pub use report_text::{LocationReportText, OSM_ATTRIBUTION, format_empty_location_report, format_location_report};` を追加し、`lib.rs` でも再公開する。

`report_text.rs`:

```rust
//! 位置ログのレポートの文言。
//!
//! 日報に載せる本文 (application の定時ジョブ) とスラッシュコマンドの embed (presentation) の
//! 両方が使うため、両者が依存できる domain に置く。

#[cfg(test)]
mod tests {
    use chrono::TimeZone as _;

    use super::*;

    /// Asia/Tokyo の時刻を UTC で作る。
    fn jst(day: u32, hour: u32, min: u32) -> DateTime<Utc> {
        chrono_tz::Asia::Tokyo
            .with_ymd_and_hms(2026, 9, day, hour, min, 0)
            .unwrap()
            .to_utc()
    }

    /// 設計書の例と同じ集計から、同じ本文が組み立てられることを確認する。
    #[test]
    fn format_location_report_builds_the_full_text() {
        let summary = LocationSummary {
            point_count: 1234,
            excluded_count: 2,
            distance_m: 12_300.0,
            distance_by_activity: vec![
                (Some(Activity::Walking), 5_100.0),
                (Some(Activity::Automotive), 7_200.0),
            ],
            moving: TimeDelta::minutes(80),
            stationary: TimeDelta::minutes(21 * 60 + 5),
            first_at: Some(jst(28, 8, 3)),
            last_at: Some(jst(29, 7, 58)),
        };
        let date = NaiveDate::from_ymd_opt(2026, 9, 28).unwrap();

        let text = format_location_report(
            date,
            (jst(28, 8, 0), jst(29, 8, 0)),
            &summary,
            chrono_tz::Asia::Tokyo,
        );

        assert_eq!(
            text.to_plain_text(),
            "位置ログ 2026-09-28 (08:00〜翌 08:00)\n\
             移動距離 12.3 km (徒歩 5.1 km / 車 7.2 km)\n\
             移動 1 時間 20 分 / 静止 21 時間 5 分\n\
             記録 1,234 点 (精度不足で 2 点を除外)\n\
             最初 08:03 / 最後 07:58\n\
             © OpenStreetMap contributors"
        );
    }

    /// 進行中の日報日 (同じ暦日で終わる範囲) では「翌」を付けず、
    /// 除外が無ければ除外の注記を省き、内訳が無ければ合計だけを出すことを確認する。
    #[test]
    fn format_location_report_handles_partial_day_without_breakdown() {
        let summary = LocationSummary {
            point_count: 3,
            excluded_count: 0,
            distance_m: 0.0,
            distance_by_activity: vec![],
            moving: TimeDelta::zero(),
            stationary: TimeDelta::minutes(20),
            first_at: Some(jst(29, 8, 10)),
            last_at: Some(jst(29, 8, 30)),
        };
        let date = NaiveDate::from_ymd_opt(2026, 9, 29).unwrap();

        let text = format_location_report(
            date,
            (jst(29, 8, 0), jst(29, 14, 32)),
            &summary,
            chrono_tz::Asia::Tokyo,
        );

        assert_eq!(text.heading, "位置ログ 2026-09-29 (08:00〜14:32)");
        assert_eq!(text.distance, "0.0 km");
        assert_eq!(text.durations, "移動 0 分 / 静止 20 分");
        assert_eq!(text.points, "3 点");
        assert_eq!(text.times, "最初 08:10 / 最後 08:30");
    }

    /// 記録が無い日の本文が 1 行になることを確認する。
    #[test]
    fn format_empty_location_report_is_a_single_line() {
        let date = NaiveDate::from_ymd_opt(2026, 9, 28).unwrap();

        assert_eq!(format_empty_location_report(date), "位置ログ 2026-09-28 記録なし");
    }
}
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cargo test -p kgd-domain location::report_text`
Expected: FAIL (`cannot find function format_location_report` など)

- [ ] **Step 3: 実装する**

```rust
use chrono::{DateTime, NaiveDate, TimeDelta, Utc};
use chrono_tz::Tz;

use super::{summary::LocationSummary, track::Activity};

/// OpenStreetMap のタイルを使うときに必須の帰属表示。
pub const OSM_ATTRIBUTION: &str = "© OpenStreetMap contributors";

/// 位置ログのレポートの文言。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocationReportText {
    /// 見出し (日付と範囲)
    pub heading: String,
    /// 移動距離と内訳
    pub distance: String,
    /// 移動と静止の時間
    pub durations: String,
    /// 記録点数と除外数
    pub points: String,
    /// 最初と最後の記録時刻
    pub times: String,
}

impl LocationReportText {
    /// 日報に載せるプレーンテキストの本文を返す。末尾に帰属表示を付ける。
    pub fn to_plain_text(&self) -> String {
        format!(
            "{}\n移動距離 {}\n{}\n記録 {}\n{}\n{}",
            self.heading, self.distance, self.durations, self.points, self.times, OSM_ATTRIBUTION
        )
    }
}

/// 集計値からレポートの文言を組み立てる。
///
/// 時刻は `timezone` で表示する。範囲の終わりが始まりの翌日なら「翌」を付ける。
pub fn format_location_report(
    date: NaiveDate,
    range: (DateTime<Utc>, DateTime<Utc>),
    summary: &LocationSummary,
    timezone: Tz,
) -> LocationReportText {
    let start = range.0.with_timezone(&timezone);
    let end = range.1.with_timezone(&timezone);
    let end_label = if end.date_naive() > start.date_naive() {
        format!("翌 {}", end.format("%H:%M"))
    } else {
        end.format("%H:%M").to_string()
    };

    let distance = if summary.distance_by_activity.is_empty() {
        format_km(summary.distance_m)
    } else {
        let breakdown: Vec<String> = summary
            .distance_by_activity
            .iter()
            .map(|(activity, meters)| format!("{} {}", activity_label(*activity), format_km(*meters)))
            .collect();
        format!("{} ({})", format_km(summary.distance_m), breakdown.join(" / "))
    };

    let points = if summary.excluded_count > 0 {
        format!(
            "{} 点 (精度不足で {} 点を除外)",
            group_thousands(summary.point_count),
            group_thousands(summary.excluded_count)
        )
    } else {
        format!("{} 点", group_thousands(summary.point_count))
    };

    let times = match (summary.first_at, summary.last_at) {
        (Some(first), Some(last)) => format!(
            "最初 {} / 最後 {}",
            first.with_timezone(&timezone).format("%H:%M"),
            last.with_timezone(&timezone).format("%H:%M")
        ),
        _ => "-".to_string(),
    };

    LocationReportText {
        heading: format!(
            "位置ログ {} ({}〜{})",
            date.format("%Y-%m-%d"),
            start.format("%H:%M"),
            end_label
        ),
        distance,
        durations: format!(
            "移動 {} / 静止 {}",
            format_duration(summary.moving),
            format_duration(summary.stationary)
        ),
        points,
        times,
    }
}

/// 記録が無い日の本文を返す。
pub fn format_empty_location_report(date: NaiveDate) -> String {
    format!("位置ログ {} 記録なし", date.format("%Y-%m-%d"))
}

/// メートルを小数 1 桁のキロメートル表記にする。
fn format_km(meters: f64) -> String {
    format!("{:.1} km", meters / 1000.0)
}

/// 時間を「N 時間 M 分」または「M 分」にする。
fn format_duration(duration: TimeDelta) -> String {
    let minutes = duration.num_minutes();
    let (hours, minutes) = (minutes / 60, minutes % 60);
    if hours > 0 {
        format!("{hours} 時間 {minutes} 分")
    } else {
        format!("{minutes} 分")
    }
}

/// 移動種別の表示名。
fn activity_label(activity: Option<Activity>) -> &'static str {
    match activity {
        Some(Activity::Walking) => "徒歩",
        Some(Activity::Cycling) => "自転車",
        Some(Activity::Automotive) => "車",
        Some(Activity::Stationary) => "静止",
        None => "不明",
    }
}

/// 3 桁ごとにカンマで区切る。
fn group_thousands(value: usize) -> String {
    let digits = value.to_string();
    let mut grouped = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index) % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    grouped
}
```

- [ ] **Step 4: テストが通ることを確認する**

Run: `cargo test -p kgd-domain location::report_text`
Expected: PASS

- [ ] **Step 5: コミットする**

```bash
~/.nix-profile/bin/just validate
git add crates/kgd-domain/src/location crates/kgd-domain/src/lib.rs
git commit -m "feat(domain): 位置ログのレポートの文言を組み立てる関数を追加する" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 6: 日報への投稿の型と段落ブロック

**Files:**
- Create: `crates/kgd-domain/src/diary_post.rs`
- Modify: `crates/kgd-domain/src/blocks.rs`、`crates/kgd-domain/src/url_rules/mod.rs`、`crates/kgd-domain/src/url_rules/json.rs`、`crates/kgd-domain/src/lib.rs`
- Test: `crates/kgd-domain/src/diary_post.rs`、`crates/kgd-domain/src/blocks.rs`

**Interfaces:**
- Consumes: なし
- Produces:
  - `pub struct DiaryPostImage { pub filename: String, pub content_type: String, pub bytes: Vec<u8> }` (`Debug, Clone, PartialEq, Eq`)
  - `pub struct DiaryPost { pub key: String, pub date: NaiveDate, pub text: String, pub images: Vec<DiaryPostImage> }` (`Debug, Clone, PartialEq, Eq`)
  - `pub struct DiaryPostRecord { pub key: String, pub diary_date: NaiveDate, pub notion_posted_at: Option<DateTime<Utc>>, pub thread_message_id: Option<u64>, pub thread_posted_at: Option<DateTime<Utc>>, pub skipped_at: Option<DateTime<Utc>> }` (`Debug, Clone, PartialEq, Eq`)
  - `impl DiaryPostRecord { pub fn notion_done(&self) -> bool; pub fn thread_done(&self) -> bool; pub fn is_done(&self) -> bool }`
  - `pub fn paragraph_block_json(text: &str) -> serde_json::Value`
  - いずれも `kgd_domain::` 直下から参照できる

- [ ] **Step 1: 失敗するテストを書く**

`lib.rs` に `mod diary_post;` と `pub use diary_post::{DiaryPost, DiaryPostImage, DiaryPostRecord};` を追加し、`pub use blocks::{...}` に `paragraph_block_json` を足す。

`crates/kgd-domain/src/diary_post.rs`:

```rust
//! bot が作った内容を日報日のスレッドと Notion ページへ載せるための型。

#[cfg(test)]
mod tests {
    use chrono::TimeZone as _;

    use super::*;

    /// 何も済んでいない記録を作る。
    fn record() -> DiaryPostRecord {
        DiaryPostRecord {
            key: "location-report:2026-09-28".to_string(),
            diary_date: NaiveDate::from_ymd_opt(2026, 9, 28).unwrap(),
            notion_posted_at: None,
            thread_message_id: None,
            thread_posted_at: None,
            skipped_at: None,
        }
    }

    /// Notion とスレッドの両方が済んで初めて完了になることを確認する。
    #[test]
    fn is_done_requires_both_notion_and_thread() {
        let at = Utc.with_ymd_and_hms(2026, 9, 28, 23, 0, 0).unwrap();
        let mut notion_only = record();
        notion_only.notion_posted_at = Some(at);
        let mut both = notion_only.clone();
        both.thread_posted_at = Some(at);
        both.thread_message_id = Some(1);

        assert!(!record().is_done());
        assert!(!notion_only.is_done());
        assert!(notion_only.notion_done());
        assert!(!notion_only.thread_done());
        assert!(both.is_done());
    }

    /// スキップ済みの記録は、どちらにも載せていなくても完了とみなすことを確認する。
    ///
    /// 日報が無い日に毎 tick 探し直さないため。
    #[test]
    fn is_done_when_skipped() {
        let mut skipped = record();
        skipped.skipped_at = Some(Utc.with_ymd_and_hms(2026, 9, 28, 23, 0, 0).unwrap());

        assert!(skipped.is_done());
    }
}
```

`crates/kgd-domain/src/blocks.rs` の `mod tests` に追加する。

```rust
    /// 段落ブロック JSON が type paragraph で、本文を rich_text に持つことを確認する。
    #[test]
    fn paragraph_block_json_holds_text() {
        let block = paragraph_block_json("位置ログ\n移動距離 1.0 km");

        assert_eq!(block["type"], "paragraph");
        assert_eq!(
            block["paragraph"]["rich_text"][0]["text"]["content"],
            "位置ログ\n移動距離 1.0 km"
        );
    }

    /// Notion の上限 (2000 文字) を超える本文が複数の rich_text 要素に分割されることを確認する。
    #[test]
    fn paragraph_block_json_splits_long_text() {
        let text = "あ".repeat(2500);

        let block = paragraph_block_json(&text);

        assert_eq!(block["paragraph"]["rich_text"].as_array().unwrap().len(), 2);
    }
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cargo test -p kgd-domain diary_post` と `cargo test -p kgd-domain blocks`
Expected: FAIL (`cannot find type DiaryPostRecord`、`cannot find function paragraph_block_json`)

- [ ] **Step 3: 実装する**

`diary_post.rs` の `#[cfg(test)]` より上:

```rust
use chrono::{DateTime, NaiveDate, Utc};

/// 日報に添付する画像。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiaryPostImage {
    /// ファイル名
    pub filename: String,
    /// Content-Type (例: image/png)
    pub content_type: String,
    /// 画像のバイト列
    pub bytes: Vec<u8>,
}

/// 日報日のスレッドと Notion ページへ載せる bot の投稿。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiaryPost {
    /// 投稿を一意に識別するキー (例: "location-report:2026-09-28")
    pub key: String,
    /// 載せる先の日報日
    pub date: NaiveDate,
    /// 本文 (プレーンテキスト)。Discord の上限 2000 文字以内であること
    pub text: String,
    /// 添付する画像
    pub images: Vec<DiaryPostImage>,
}

/// 日報への投稿の進み具合の記録。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiaryPostRecord {
    /// 投稿のキー
    pub key: String,
    /// 載せる先の日報日
    pub diary_date: NaiveDate,
    /// Notion ページへ載せた時刻
    pub notion_posted_at: Option<DateTime<Utc>>,
    /// スレッドへ投稿したメッセージ ID
    pub thread_message_id: Option<u64>,
    /// スレッドへ投稿した時刻
    pub thread_posted_at: Option<DateTime<Utc>>,
    /// 日報が無いためスキップした時刻
    pub skipped_at: Option<DateTime<Utc>>,
}

impl DiaryPostRecord {
    /// Notion ページへ載せ終えているか。
    pub fn notion_done(&self) -> bool {
        self.notion_posted_at.is_some()
    }

    /// スレッドへ載せ終えているか。
    pub fn thread_done(&self) -> bool {
        self.thread_posted_at.is_some()
    }

    /// これ以上何もしなくてよいか (両方済んだ、またはスキップ済み)。
    pub fn is_done(&self) -> bool {
        self.skipped_at.is_some() || (self.notion_done() && self.thread_done())
    }
}
```

`url_rules/json.rs` の `plain_text_chunks_json` を `pub(crate)` にし、`url_rules/mod.rs` に `pub(crate) use json::plain_text_chunks_json;` を追加する。

`blocks.rs` に追加する。

```rust
use crate::url_rules::plain_text_chunks_json;

/// プレーンテキストの段落ブロック JSON を生成する。
///
/// Notion の rich_text 1 要素あたりの上限を超える本文は、複数の要素に分割する。
pub fn paragraph_block_json(text: &str) -> serde_json::Value {
    serde_json::json!({
        "object": "block",
        "type": "paragraph",
        "paragraph": {
            "rich_text": plain_text_chunks_json(text)
        }
    })
}
```

- [ ] **Step 4: テストが通ることを確認する**

Run: `cargo test -p kgd-domain diary_post` と `cargo test -p kgd-domain blocks`
Expected: PASS

- [ ] **Step 5: コミットする**

```bash
~/.nix-profile/bin/just validate
git add crates/kgd-domain/src
git commit -m "feat(domain): 日報への bot の投稿を表す型と段落ブロックを追加する" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 7: 日報日の範囲の点を読み出す

**Files:**
- Create: `crates/kgd-infrastructure/migrations/20260929_000001_index_owntracks_messages_tst.sql`
- Modify: `crates/kgd-application/src/ports/location.rs`
- Modify: `crates/kgd-infrastructure/src/location_store/mod.rs`
- Modify: `crates/kgd-presentation/src/owntracks/tests.rs`、`crates/kgd-presentation/Cargo.toml` (手書きのスタブ)
- Modify: `crates/kgd/src/import.rs`、`crates/kgd/Cargo.toml` (テスト内の手書きのスタブ)
- Test: `crates/kgd-infrastructure/src/location_store/mod.rs` (既存の `mod tests`)

**Interfaces:**
- Consumes: Task 2 の `TrackPoint`、`Activity::from_motion`
- Produces:
  - `LocationRepository::locations_between(&self, start: DateTime<Utc>, end: DateTime<Utc>) -> Result<Vec<TrackPoint>>` (開始を含み終了を含まない。緯度経度を持つ location だけを時刻順で返す)

- [ ] **Step 1: 失敗するテストを書く**

`location_store/mod.rs` の `mod tests` に追加する。行から点への変換は DB を伴わない純粋関数として切り出してテストする。

```rust
    /// 行の motion 文字列が移動種別へ変換され、精度がそのまま残ることを確認する。
    #[test]
    fn to_track_point_maps_motion_and_accuracy() {
        use chrono::{TimeZone as _, Utc};
        use kgd_domain::Activity;

        let at = Utc.with_ymd_and_hms(2026, 9, 28, 0, 0, 0).unwrap();

        let walking = to_track_point((at, 35.0, 139.0, Some(12), Some("walking".to_string())));
        let missing = to_track_point((at, 35.0, 139.0, None, None));

        assert_eq!(walking.activity, Some(Activity::Walking));
        assert_eq!(walking.accuracy_m, Some(12));
        assert_eq!(missing.activity, None);
        assert_eq!(missing.accuracy_m, None);
    }
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cargo test -p kgd-infrastructure location_store`
Expected: FAIL (`cannot find function to_track_point`)

- [ ] **Step 3: ポートにメソッドを追加する**

`crates/kgd-application/src/ports/location.rs`:

```rust
//! OwnTracks メッセージの永続化を抽象化するポート。

use anyhow::Result;
use chrono::{DateTime, Utc};

use kgd_domain::{OwnTracksMessage, TrackPoint};

/// OwnTracks メッセージの永続化を抽象化するポート。
#[cfg_attr(test, mockall::automock)]
#[async_trait::async_trait]
pub trait LocationRepository: Send + Sync {
    /// メッセージをまとめて保存し、実際に挿入された件数を返す。
    ///
    /// 既に同じメッセージが保存されている場合は挿入せず、件数にも含めない。
    async fn insert_messages(&self, messages: &[OwnTracksMessage]) -> Result<usize>;

    /// 指定した範囲 (開始を含み終了を含まない) の位置の点を時刻順に返す。
    ///
    /// 対象は緯度経度を持つ location メッセージに限る。
    async fn locations_between(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<TrackPoint>>;
}
```

- [ ] **Step 4: マイグレーションとストアを実装する**

`crates/kgd-infrastructure/migrations/20260929_000001_index_owntracks_messages_tst.sql`:

```sql
-- 日次レポートは端末を問わず時刻の範囲で location を取り出す
CREATE INDEX IF NOT EXISTS idx_owntracks_messages_location_tst
    ON owntracks_messages(tst)
    WHERE msg_type = 'location';
```

`location_store/mod.rs` に追加する (`use` に `chrono::{DateTime, Utc}` と `kgd_domain::{Activity, TrackPoint}` を足す)。

```rust
/// 位置の点として読み出す行 (tst, lat, lon, acc, motion)。
type LocationRow = (DateTime<Utc>, f64, f64, Option<i32>, Option<String>);

/// 読み出した行を軌跡の点へ変換する。
fn to_track_point((at, lat, lon, accuracy_m, motion): LocationRow) -> TrackPoint {
    TrackPoint {
        at,
        lat,
        lon,
        accuracy_m,
        activity: motion.as_deref().and_then(Activity::from_motion),
    }
}
```

`impl LocationRepository for LocationStore` に追加する。

```rust
    async fn locations_between(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<TrackPoint>> {
        let rows: Vec<LocationRow> = sqlx::query_as(
            r#"
            SELECT tst, lat, lon, acc, motion
            FROM owntracks_messages
            WHERE msg_type = 'location'
              AND tst >= $1
              AND tst < $2
              AND lat IS NOT NULL
              AND lon IS NOT NULL
            ORDER BY tst
            "#,
        )
        .bind(start)
        .bind(end)
        .fetch_all(&self.pool)
        .await
        .context("Failed to fetch owntracks locations")?;

        Ok(rows.into_iter().map(to_track_point).collect())
    }
```

- [ ] **Step 5: 手書きのスタブに追従させる**

kgd-application のモックは `#[cfg(test)]` で生成されるため、他のクレートのテストは `LocationRepository` を手書きのスタブで実装している。次の 2 つに `locations_between` を足す。どちらのテストも範囲の読み出しを使わないので、空を返せばよい。

- `crates/kgd-presentation/src/owntracks/tests.rs` の `impl LocationRepository for StubLocationRepository`
- `crates/kgd/src/import.rs` のテスト内の `impl LocationRepository for RecordingRepository`

```rust
    async fn locations_between(
        &self,
        _start: chrono::DateTime<chrono::Utc>,
        _end: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<kgd_domain::TrackPoint>> {
        Ok(Vec::new())
    }
```

`chrono` を参照するため、`crates/kgd-presentation/Cargo.toml` の `[dependencies]` に `chrono.workspace = true` を (Task 15 の実装でも使う)、`crates/kgd/Cargo.toml` の `[dev-dependencies]` に `chrono.workspace = true` を追加する。

- [ ] **Step 6: テストが通ることを確認する**

`cargo check` と `just validate` の clippy は `#[cfg(test)]` のコードをコンパイルしないため、テストを含めて確認する。

Run: `cargo check --workspace --all-targets`、`cargo test -p kgd-infrastructure location_store`、`cargo test -p kgd-application`
Expected: PASS (application の既存テストは `MockLocationRepository` の自動生成に追従する)

- [ ] **Step 7: コミットする**

```bash
~/.nix-profile/bin/just validate
git add Cargo.lock crates/kgd-application/src/ports/location.rs crates/kgd-infrastructure crates/kgd-presentation crates/kgd
git commit -m "feat: 日報日の範囲の位置の点を読み出せるようにする" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 8: 日報への投稿の記録

**Files:**
- Create: `crates/kgd-application/src/ports/diary_post.rs`
- Modify: `crates/kgd-application/src/ports/mod.rs`
- Create: `crates/kgd-infrastructure/migrations/20260929_000002_create_diary_posts.sql`
- Create: `crates/kgd-infrastructure/src/diary_post_store.rs`
- Modify: `crates/kgd-infrastructure/src/lib.rs`
- Test: `crates/kgd-infrastructure/src/diary_post_store.rs`

**Interfaces:**
- Consumes: Task 6 の `DiaryPostRecord`
- Produces:
  - `pub trait DiaryPostRepository: Send + Sync` (`#[cfg_attr(test, mockall::automock)]`、`MockDiaryPostRepository` を `ports` から再公開)
    - `async fn get(&self, key: &str) -> Result<Option<DiaryPostRecord>>`
    - `async fn mark_notion_posted(&self, key: &str, date: NaiveDate, at: DateTime<Utc>) -> Result<()>`
    - `async fn mark_thread_posted(&self, key: &str, date: NaiveDate, message_id: u64, at: DateTime<Utc>) -> Result<()>`
    - `async fn mark_skipped(&self, key: &str, date: NaiveDate, at: DateTime<Utc>) -> Result<()>`
  - `pub struct DiaryPostStore` (`kgd_infrastructure::DiaryPostStore`、`DiaryPostStore::new(pool: PgPool)`)

- [ ] **Step 1: 失敗するテストを書く**

`crates/kgd-infrastructure/src/diary_post_store.rs` を作り、行の変換のテストを書く。

```rust
//! 日報への bot の投稿の進み具合を永続化するストア。

#[cfg(test)]
mod tests {
    use chrono::{NaiveDate, TimeZone as _, Utc};

    use super::*;

    /// 行が記録へ変換され、メッセージ ID が u64 に戻ることを確認する。
    #[test]
    fn diary_post_row_converts_into_record() {
        let at = Utc.with_ymd_and_hms(2026, 9, 28, 23, 0, 0).unwrap();
        let row = DiaryPostRow {
            post_key: "location-report:2026-09-28".to_string(),
            diary_date: NaiveDate::from_ymd_opt(2026, 9, 28).unwrap(),
            notion_posted_at: Some(at),
            thread_message_id: Some(1_234_567_890_123_456_789),
            thread_posted_at: Some(at),
            skipped_at: None,
        };

        let record = DiaryPostRecord::from(row);

        assert_eq!(record.key, "location-report:2026-09-28");
        assert_eq!(record.thread_message_id, Some(1_234_567_890_123_456_789));
        assert!(record.is_done());
    }
}
```

`lib.rs` に `mod diary_post_store;` と `pub use diary_post_store::DiaryPostStore;` を追加する。

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cargo test -p kgd-infrastructure diary_post_store`
Expected: FAIL (`cannot find struct DiaryPostRow`)

- [ ] **Step 3: ポートを追加する**

`crates/kgd-application/src/ports/diary_post.rs`:

```rust
//! 日報への bot の投稿の進み具合を永続化するポート。

use anyhow::Result;
use chrono::{DateTime, NaiveDate, Utc};

use kgd_domain::DiaryPostRecord;

/// 日報への bot の投稿の進み具合を永続化するポート。
///
/// 段ごとに完了を記録し、途中で失敗しても残りの段だけをやり直せるようにする。
#[cfg_attr(test, mockall::automock)]
#[async_trait::async_trait]
pub trait DiaryPostRepository: Send + Sync {
    /// キーに対応する記録を返す。まだ無ければ `None`。
    async fn get(&self, key: &str) -> Result<Option<DiaryPostRecord>>;

    /// Notion ページへ載せ終えたことを記録する。
    async fn mark_notion_posted(&self, key: &str, date: NaiveDate, at: DateTime<Utc>)
    -> Result<()>;

    /// スレッドへ投稿し終えたことを、投稿したメッセージ ID とともに記録する。
    async fn mark_thread_posted(
        &self,
        key: &str,
        date: NaiveDate,
        message_id: u64,
        at: DateTime<Utc>,
    ) -> Result<()>;

    /// 日報が無いためスキップしたことを記録する。
    async fn mark_skipped(&self, key: &str, date: NaiveDate, at: DateTime<Utc>) -> Result<()>;
}
```

`ports/mod.rs` に `mod diary_post;`、`pub use diary_post::DiaryPostRepository;`、`#[cfg(test)] pub use diary_post::MockDiaryPostRepository;` を追加する。

- [ ] **Step 4: マイグレーションとストアを実装する**

`crates/kgd-infrastructure/migrations/20260929_000002_create_diary_posts.sql`:

```sql
-- bot が日報日のスレッドと Notion ページへ載せた投稿の進み具合
CREATE TABLE IF NOT EXISTS diary_posts (
    -- 投稿を一意に識別するキー (例: location-report:2026-09-28)
    post_key TEXT PRIMARY KEY,
    -- 載せる先の日報日
    diary_date DATE NOT NULL,
    -- Notion ページへ載せた時刻
    notion_posted_at TIMESTAMPTZ,
    -- スレッドへ投稿したメッセージ ID
    thread_message_id BIGINT,
    -- スレッドへ投稿した時刻
    thread_posted_at TIMESTAMPTZ,
    -- 日報が無いためスキップした時刻
    skipped_at TIMESTAMPTZ
);
```

`diary_post_store.rs` の `#[cfg(test)]` より上:

```rust
use anyhow::{Context as _, Result};
use chrono::{DateTime, NaiveDate, Utc};
use sqlx::{FromRow, PgPool};

use kgd_application::ports::DiaryPostRepository;
use kgd_domain::DiaryPostRecord;

/// 日報への投稿の進み具合を管理するストア。
#[derive(Clone)]
pub struct DiaryPostStore {
    pool: PgPool,
}

impl DiaryPostStore {
    /// 既存のプールからストアを作る。
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

/// diary_posts テーブルの行。
#[derive(Debug, Clone, FromRow)]
struct DiaryPostRow {
    /// 投稿のキー
    post_key: String,
    /// 載せる先の日報日
    diary_date: NaiveDate,
    /// Notion ページへ載せた時刻
    notion_posted_at: Option<DateTime<Utc>>,
    /// スレッドへ投稿したメッセージ ID
    thread_message_id: Option<i64>,
    /// スレッドへ投稿した時刻
    thread_posted_at: Option<DateTime<Utc>>,
    /// スキップした時刻
    skipped_at: Option<DateTime<Utc>>,
}

impl From<DiaryPostRow> for DiaryPostRecord {
    fn from(row: DiaryPostRow) -> Self {
        Self {
            key: row.post_key,
            diary_date: row.diary_date,
            notion_posted_at: row.notion_posted_at,
            thread_message_id: row.thread_message_id.map(|id| id as u64),
            thread_posted_at: row.thread_posted_at,
            skipped_at: row.skipped_at,
        }
    }
}

#[async_trait::async_trait]
impl DiaryPostRepository for DiaryPostStore {
    async fn get(&self, key: &str) -> Result<Option<DiaryPostRecord>> {
        let row: Option<DiaryPostRow> = sqlx::query_as(
            r#"
            SELECT post_key, diary_date, notion_posted_at, thread_message_id,
                   thread_posted_at, skipped_at
            FROM diary_posts
            WHERE post_key = $1
            "#,
        )
        .bind(key)
        .fetch_optional(&self.pool)
        .await
        .context("Failed to fetch diary post")?;

        Ok(row.map(DiaryPostRecord::from))
    }

    async fn mark_notion_posted(
        &self,
        key: &str,
        date: NaiveDate,
        at: DateTime<Utc>,
    ) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO diary_posts (post_key, diary_date, notion_posted_at)
            VALUES ($1, $2, $3)
            ON CONFLICT (post_key) DO UPDATE SET notion_posted_at = EXCLUDED.notion_posted_at
            "#,
        )
        .bind(key)
        .bind(date)
        .bind(at)
        .execute(&self.pool)
        .await
        .context("Failed to record notion post")?;
        Ok(())
    }

    async fn mark_thread_posted(
        &self,
        key: &str,
        date: NaiveDate,
        message_id: u64,
        at: DateTime<Utc>,
    ) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO diary_posts (post_key, diary_date, thread_message_id, thread_posted_at)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (post_key) DO UPDATE SET
                thread_message_id = EXCLUDED.thread_message_id,
                thread_posted_at = EXCLUDED.thread_posted_at
            "#,
        )
        .bind(key)
        .bind(date)
        .bind(message_id as i64)
        .bind(at)
        .execute(&self.pool)
        .await
        .context("Failed to record thread post")?;
        Ok(())
    }

    async fn mark_skipped(&self, key: &str, date: NaiveDate, at: DateTime<Utc>) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO diary_posts (post_key, diary_date, skipped_at)
            VALUES ($1, $2, $3)
            ON CONFLICT (post_key) DO UPDATE SET skipped_at = EXCLUDED.skipped_at
            "#,
        )
        .bind(key)
        .bind(date)
        .bind(at)
        .execute(&self.pool)
        .await
        .context("Failed to record skipped diary post")?;
        Ok(())
    }
}
```

- [ ] **Step 5: テストが通ることを確認する**

Run: `cargo test -p kgd-infrastructure diary_post_store` と `cargo test -p kgd-application`
Expected: PASS

- [ ] **Step 6: コミットする**

```bash
~/.nix-profile/bin/just validate
git add crates/kgd-application/src/ports crates/kgd-infrastructure
git commit -m "feat: 日報への bot の投稿の進み具合を記録するテーブルとストアを追加する" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 9: 画像付きのメッセージ送信

**Files:**
- Modify: `crates/kgd-application/src/ports/discord.rs`
- Modify: `crates/kgd-infrastructure/src/discord_gateway/mod.rs`

**Interfaces:**
- Consumes: Task 6 の `DiaryPostImage`
- Produces:
  - `DiscordGateway::send_text_with_images(&self, channel_id: u64, content: &str, images: &[DiaryPostImage]) -> Result<u64>`

アダプタは serenity の呼び出しを並べるだけで判断を持たないため、単体テストは書かない ([architecture.md](../../architecture.md) のテスト戦略)。`MockDiscordGateway` は自動生成で追従する。

- [ ] **Step 1: ポートにメソッドを追加する**

`ports/discord.rs` の `use kgd_domain::{...}` に `DiaryPostImage` を足し、`send_text` の直後に追加する。

```rust
    /// 本文と画像を添付したメッセージを送信し、メッセージ ID を返す。
    async fn send_text_with_images(
        &self,
        channel_id: u64,
        content: &str,
        images: &[DiaryPostImage],
    ) -> Result<u64>;
```

- [ ] **Step 2: ビルドが失敗することを確認する**

Run: `cargo check -p kgd-infrastructure`
Expected: FAIL (`not all trait items implemented, missing: send_text_with_images`)

- [ ] **Step 3: 実装する**

`discord_gateway/mod.rs` の serenity の `use` に `CreateAttachment` を、`kgd_domain` の `use` に `DiaryPostImage` を足し、`send_text` の直後に追加する。

```rust
    async fn send_text_with_images(
        &self,
        channel_id: u64,
        content: &str,
        images: &[DiaryPostImage],
    ) -> Result<u64> {
        let files = images
            .iter()
            .map(|image| CreateAttachment::bytes(image.bytes.clone(), image.filename.clone()));
        let message = CreateMessage::new().content(content).add_files(files);
        let sent = ChannelId::new(channel_id)
            .send_message(&self.http, message)
            .await
            .context("Failed to send message with images")?;
        Ok(sent.id.get())
    }
```

- [ ] **Step 4: ビルドとテストが通ることを確認する**

手書きの `DiscordGateway` の実装は `SerenityGateway` 以外に無い (`grep -rn "impl DiscordGateway for" crates/` で確認できる)。テストのコードも含めて確認する。

Run: `cargo check --workspace --all-targets` と `cargo test -p kgd-application`
Expected: PASS

- [ ] **Step 5: コミットする**

```bash
~/.nix-profile/bin/just validate
git add crates/kgd-application/src/ports/discord.rs crates/kgd-infrastructure/src/discord_gateway/mod.rs
git commit -m "feat: 画像を添付したメッセージを送信できるようにする" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 10: タイルに軌跡を重ねる描画

**Files:**
- Create: `crates/kgd-application/src/ports/map.rs`
- Modify: `crates/kgd-application/src/ports/mod.rs`
- Modify: `Cargo.toml` (workspace)、`crates/kgd-infrastructure/Cargo.toml`
- Create: `crates/kgd-infrastructure/src/map_renderer/mod.rs`
- Create: `crates/kgd-infrastructure/src/map_renderer/tests.rs`
- Modify: `crates/kgd-infrastructure/src/lib.rs`

**Interfaces:**
- Consumes: Task 2 の `Activity`、`TrackSegment`、Task 4 の `Viewport`、`TilePlacement`
- Produces:
  - `pub trait MapRenderer: Send + Sync { async fn render(&self, viewport: &Viewport, segments: &[TrackSegment]) -> Result<Vec<u8>>; }` (`#[cfg_attr(test, mockall::automock)]`、`MockMapRenderer` を `ports` から再公開)
  - `pub struct TileMapRenderer` (`kgd_infrastructure::TileMapRenderer`)
    - `pub fn new(user_agent: &str, cache_dir: impl Into<PathBuf>) -> Result<Self>` (OSM の公式タイルを使う)
    - `pub(crate) fn with_base_url(user_agent: &str, cache_dir: impl Into<PathBuf>, base_url: &str) -> Result<Self>` (テスト用)

- [ ] **Step 1: ポートと依存を追加する**

`crates/kgd-application/src/ports/map.rs`:

```rust
//! 地図画像の描画を抽象化するポート。

use anyhow::Result;

use kgd_domain::{TrackSegment, Viewport};

/// 地図画像の描画を抽象化するポート。
#[cfg_attr(test, mockall::automock)]
#[async_trait::async_trait]
pub trait MapRenderer: Send + Sync {
    /// 表示範囲の地図に軌跡の区間を重ねた画像を PNG で返す。
    async fn render(&self, viewport: &Viewport, segments: &[TrackSegment]) -> Result<Vec<u8>>;
}
```

`ports/mod.rs` に `mod map;`、`pub use map::MapRenderer;`、`#[cfg(test)] pub use map::MockMapRenderer;` を追加する。

workspace の `Cargo.toml` の `[workspace.dependencies]` に追加する。

```toml
# Map rendering (location report)
tiny-skia = "0.12"
```

`crates/kgd-infrastructure/Cargo.toml` の `[dependencies]` に `tiny-skia.workspace = true` を足し、tokio の features を `["fs", "net"]` にする。テストで一時ディレクトリを使うため、次を追加する。

```toml
[dev-dependencies]
tempfile.workspace = true
```

- [ ] **Step 2: 失敗するテストを書く**

`crates/kgd-infrastructure/src/lib.rs` に `mod map_renderer;` と `pub use map_renderer::TileMapRenderer;` を追加する。

`crates/kgd-infrastructure/src/map_renderer/tests.rs`:

```rust
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
            recorded.lock().unwrap().push((uri.path().to_string(), agent));
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
```

- [ ] **Step 3: テストが失敗することを確認する**

Run: `cargo test -p kgd-infrastructure map_renderer`
Expected: FAIL (`cannot find struct TileMapRenderer`)

- [ ] **Step 4: 実装する**

`crates/kgd-infrastructure/src/map_renderer/mod.rs`:

```rust
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
                Err(error) => warn!(?error, path = %path.display(), "Ignoring broken cached map tile"),
            }
        }

        let bytes = match self.fetch_tile(tile).await {
            Ok(bytes) => bytes,
            Err(error) => {
                warn!(?error, z = tile.z, x = tile.x, y = tile.y, "Failed to fetch map tile");
                return None;
            }
        };
        let pixmap = match Pixmap::decode_png(&bytes) {
            Ok(pixmap) => pixmap,
            Err(error) => {
                warn!(?error, z = tile.z, x = tile.x, y = tile.y, "Failed to decode map tile");
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
                canvas.fill_path(&circle, &paint, FillRule::Winding, Transform::identity(), None);
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
```

- [ ] **Step 5: テストが通ることを確認する**

Run: `cargo test -p kgd-infrastructure map_renderer` と `cargo test -p kgd-application`
Expected: PASS

- [ ] **Step 6: ライセンスと重複を確認する**

Run: `~/.nix-profile/bin/just deny`
Expected: PASS (tiny-skia とその依存が許可リストに収まる)。`multiple-versions` の警告が出たら、重複しているクレート名をコミットメッセージの本文に書き残す

- [ ] **Step 7: コミットする**

```bash
~/.nix-profile/bin/just validate
git add Cargo.toml Cargo.lock crates/kgd-application/src/ports crates/kgd-infrastructure
git commit -m "feat: OpenStreetMap のタイルに軌跡を重ねて描く描画器を追加する" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 11: レポートを作るユースケース

**Files:**
- Create: `crates/kgd-application/src/build_location_report.rs`
- Modify: `crates/kgd-application/src/lib.rs`
- Test: `crates/kgd-application/src/build_location_report.rs`

**Interfaces:**
- Consumes: Task 1 の `DiaryCalendar::day_range`、Task 2 から 4 の `filter_accurate`、`split_segments`、`summarize`、`LocationSummary`、`fit_viewport`、Task 7 の `LocationRepository::locations_between`、Task 10 の `MapRenderer`
- Produces:
  - `pub struct LocationReportSettings { pub calendar: DiaryCalendar, pub max_accuracy_m: i32, pub image_width: u32, pub image_height: u32 }` (`Debug, Clone, Copy`)
  - `pub struct LocationReport { pub date: NaiveDate, pub range: (DateTime<Utc>, DateTime<Utc>), pub summary: LocationSummary, pub image: Option<Vec<u8>> }` (`Debug, Clone, PartialEq`)
  - `pub struct BuildLocationReportUseCase`、`pub fn new(repo: Arc<dyn LocationRepository>, renderer: Arc<dyn MapRenderer>, settings: LocationReportSettings) -> Self`
  - `pub async fn build(&self, date: NaiveDate, until: Option<DateTime<Utc>>) -> Result<LocationReport>`
  - `lib.rs` から `BuildLocationReportUseCase, LocationReport, LocationReportSettings` を公開する

- [ ] **Step 1: 失敗するテストを書く**

`lib.rs` に `mod build_location_report;` と `pub use build_location_report::{BuildLocationReportUseCase, LocationReport, LocationReportSettings};` を追加する。

`build_location_report.rs`:

```rust
//! 日報日の位置ログから地図画像と集計値を作るユースケース。

#[cfg(test)]
mod tests {
    use chrono::TimeZone as _;

    use kgd_domain::TrackPoint;

    use crate::ports::{MockLocationRepository, MockMapRenderer};

    use super::*;

    /// Asia/Tokyo の時刻を UTC で作る。
    fn jst(day: u32, hour: u32, min: u32) -> DateTime<Utc> {
        chrono_tz::Asia::Tokyo
            .with_ymd_and_hms(2026, 9, day, hour, min, 0)
            .unwrap()
            .to_utc()
    }

    /// 精度を指定した点を作る。
    fn point(at: DateTime<Utc>, accuracy_m: i32) -> TrackPoint {
        TrackPoint {
            at,
            lat: 35.68,
            lon: 139.76,
            accuracy_m: Some(accuracy_m),
            activity: None,
        }
    }

    fn settings() -> LocationReportSettings {
        LocationReportSettings {
            calendar: DiaryCalendar::new(chrono_tz::Asia::Tokyo, 8),
            max_accuracy_m: 200,
            image_width: 1024,
            image_height: 1024,
        }
    }

    fn date() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 9, 28).unwrap()
    }

    /// 日報日の範囲 (開始を含み終了を含まない) をそのままリポジトリへ渡すことを確認する。
    #[tokio::test]
    async fn build_queries_the_half_open_diary_day_range() {
        let mut repo = MockLocationRepository::new();
        repo.expect_locations_between()
            .withf(|start, end| *start == jst(28, 8, 0) && *end == jst(29, 8, 0))
            .times(1)
            .returning(|_, _| Ok(vec![]));
        let mut renderer = MockMapRenderer::new();
        renderer.expect_render().times(0);
        let use_case = BuildLocationReportUseCase::new(Arc::new(repo), Arc::new(renderer), settings());

        let report = use_case.build(date(), None).await.unwrap();

        assert_eq!(report.range, (jst(28, 8, 0), jst(29, 8, 0)));
        assert_eq!(report.image, None);
        assert_eq!(report.summary.point_count, 0);
    }

    /// until を渡すと範囲の終わりがそこで切り詰められることを確認する。
    ///
    /// スラッシュコマンドで進行中の日報日を「今まで」の分だけ見るため。
    #[tokio::test]
    async fn build_truncates_the_range_at_until() {
        let mut repo = MockLocationRepository::new();
        repo.expect_locations_between()
            .withf(|start, end| *start == jst(28, 8, 0) && *end == jst(28, 14, 32))
            .times(1)
            .returning(|_, _| Ok(vec![]));
        let use_case = BuildLocationReportUseCase::new(
            Arc::new(repo),
            Arc::new(MockMapRenderer::new()),
            settings(),
        );

        let report = use_case.build(date(), Some(jst(28, 14, 32))).await.unwrap();

        assert_eq!(report.range.1, jst(28, 14, 32));
    }

    /// until が日報日の終わりより後なら、日報日の終わりで止めることを確認する。
    #[tokio::test]
    async fn build_keeps_day_end_when_until_is_later() {
        let mut repo = MockLocationRepository::new();
        repo.expect_locations_between()
            .withf(|_, end| *end == jst(29, 8, 0))
            .times(1)
            .returning(|_, _| Ok(vec![]));
        let use_case = BuildLocationReportUseCase::new(
            Arc::new(repo),
            Arc::new(MockMapRenderer::new()),
            settings(),
        );

        let report = use_case.build(date(), Some(jst(30, 0, 0))).await.unwrap();

        assert_eq!(report.range.1, jst(29, 8, 0));
    }

    /// until が日報日の始まりより前なら、リポジトリを呼ばずに空のレポートを返すことを確認する。
    #[tokio::test]
    async fn build_skips_query_when_until_precedes_day_start() {
        let mut repo = MockLocationRepository::new();
        repo.expect_locations_between().times(0);
        let use_case = BuildLocationReportUseCase::new(
            Arc::new(repo),
            Arc::new(MockMapRenderer::new()),
            settings(),
        );

        let report = use_case.build(date(), Some(jst(28, 7, 0))).await.unwrap();

        assert_eq!(report.range, (jst(28, 8, 0), jst(28, 8, 0)));
        assert_eq!(report.summary.point_count, 0);
    }

    /// 精度不足の点を除いて集計し、残った点で地図を描くことを確認する。
    #[tokio::test]
    async fn build_renders_map_from_accurate_points() {
        let mut repo = MockLocationRepository::new();
        repo.expect_locations_between()
            .returning(|_, _| Ok(vec![point(jst(28, 9, 0), 10), point(jst(28, 9, 1), 1414)]));
        let mut renderer = MockMapRenderer::new();
        renderer
            .expect_render()
            .withf(|viewport, segments| {
                viewport.width == 1024 && segments.len() == 1 && segments[0].points.len() == 1
            })
            .times(1)
            .returning(|_, _| Ok(vec![1, 2, 3]));
        let use_case = BuildLocationReportUseCase::new(Arc::new(repo), Arc::new(renderer), settings());

        let report = use_case.build(date(), None).await.unwrap();

        assert_eq!(report.image, Some(vec![1, 2, 3]));
        assert_eq!(report.summary.point_count, 1);
        assert_eq!(report.summary.excluded_count, 1);
    }

    /// 点がすべて精度不足で除外されたら、地図を描かず画像を持たないことを確認する。
    #[tokio::test]
    async fn build_returns_no_image_when_all_points_are_inaccurate() {
        let mut repo = MockLocationRepository::new();
        repo.expect_locations_between()
            .returning(|_, _| Ok(vec![point(jst(28, 9, 0), 1414), point(jst(28, 9, 1), 900)]));
        let mut renderer = MockMapRenderer::new();
        renderer.expect_render().times(0);
        let use_case = BuildLocationReportUseCase::new(Arc::new(repo), Arc::new(renderer), settings());

        let report = use_case.build(date(), None).await.unwrap();

        assert_eq!(report.image, None);
        assert_eq!(report.summary.point_count, 0);
        assert_eq!(report.summary.excluded_count, 2);
    }
}
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cargo test -p kgd-application build_location_report`
Expected: FAIL (`cannot find struct BuildLocationReportUseCase`)

- [ ] **Step 3: 実装する**

```rust
use std::sync::Arc;

use anyhow::Result;
use chrono::{DateTime, NaiveDate, Utc};

use kgd_domain::{
    DiaryCalendar, LocationSummary, filter_accurate, fit_viewport, split_segments, summarize,
};

use super::ports::{LocationRepository, MapRenderer};

/// レポートの作り方の設定。
#[derive(Debug, Clone, Copy)]
pub struct LocationReportSettings {
    /// 日報日の区切り方
    pub calendar: DiaryCalendar,
    /// これを超える水平精度 (メートル) の点を除く
    pub max_accuracy_m: i32,
    /// 地図画像の幅 (ピクセル)
    pub image_width: u32,
    /// 地図画像の高さ (ピクセル)
    pub image_height: u32,
}

/// 日報日 1 日ぶんの位置ログのレポート。
#[derive(Debug, Clone, PartialEq)]
pub struct LocationReport {
    /// 対象の日報日
    pub date: NaiveDate,
    /// 集計に使った範囲 (開始を含み終了を含まない)
    pub range: (DateTime<Utc>, DateTime<Utc>),
    /// 集計値
    pub summary: LocationSummary,
    /// 地図画像 (PNG)。集計に使える点が無ければ None
    pub image: Option<Vec<u8>>,
}

/// 日報日の位置ログから地図画像と集計値を作るユースケース。
///
/// 届け先 (日報、スラッシュコマンド、将来の CLI) を知らず、レポートを返すだけにする。
pub struct BuildLocationReportUseCase {
    /// 位置情報リポジトリポート
    repo: Arc<dyn LocationRepository>,
    /// 地図描画ポート
    renderer: Arc<dyn MapRenderer>,
    /// レポートの作り方の設定
    settings: LocationReportSettings,
}

impl BuildLocationReportUseCase {
    /// 新しい BuildLocationReportUseCase を作成する。
    pub fn new(
        repo: Arc<dyn LocationRepository>,
        renderer: Arc<dyn MapRenderer>,
        settings: LocationReportSettings,
    ) -> Self {
        Self {
            repo,
            renderer,
            settings,
        }
    }

    /// 指定した日報日のレポートを作る。
    ///
    /// `until` を渡すと範囲の終わりをそこで切り詰める (日報日の始まりより前にはならない)。
    pub async fn build(
        &self,
        date: NaiveDate,
        until: Option<DateTime<Utc>>,
    ) -> Result<LocationReport> {
        let (start, day_end) = self.settings.calendar.day_range(date);
        let end = until.map_or(day_end, |until| until.clamp(start, day_end));

        let raw = if end > start {
            self.repo.locations_between(start, end).await?
        } else {
            Vec::new()
        };
        let (points, excluded) = filter_accurate(raw, self.settings.max_accuracy_m);
        let summary = summarize(&points, excluded);

        let image = match fit_viewport(
            &points,
            self.settings.image_width,
            self.settings.image_height,
        ) {
            Some(viewport) => Some(
                self.renderer
                    .render(&viewport, &split_segments(&points))
                    .await?,
            ),
            None => None,
        };

        Ok(LocationReport {
            date,
            range: (start, end),
            summary,
            image,
        })
    }
}
```

- [ ] **Step 4: テストが通ることを確認する**

Run: `cargo test -p kgd-application build_location_report`
Expected: PASS

- [ ] **Step 5: コミットする**

```bash
~/.nix-profile/bin/just validate
git add crates/kgd-application/src
git commit -m "feat(application): 日報日の位置ログのレポートを作るユースケースを追加する" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 12: bot の投稿を日報へ載せるユースケース

**Files:**
- Create: `crates/kgd-application/src/publish_diary_post.rs`
- Modify: `crates/kgd-application/src/lib.rs`
- Test: `crates/kgd-application/src/publish_diary_post.rs`

**Interfaces:**
- Consumes: Task 1 の `DiaryCalendar::start_of`、Task 6 の `DiaryPost`、`DiaryPostRecord`、`paragraph_block_json`、Task 8 の `DiaryPostRepository`、Task 9 の `send_text_with_images`、既存の `DiaryRepository::get_by_date`、`NotionApi::{upload_file, append_blocks}`、`DiscordGateway::{thread_state, reopen_thread, close_thread}`、`Clock`、`image_block_json`
- Produces:
  - `pub enum PublishOutcome { Published, AlreadyDone, NoDiary }` (`Debug, Clone, Copy, PartialEq, Eq`)
  - `pub struct PublishDiaryPostUseCase`、`pub fn new(diary: Arc<dyn DiaryRepository>, posts: Arc<dyn DiaryPostRepository>, notion: Arc<dyn NotionApi>, discord: Arc<dyn DiscordGateway>, clock: Arc<dyn Clock>, calendar: DiaryCalendar) -> Self`
  - `pub async fn is_done(&self, key: &str) -> Result<bool>`
  - `pub async fn publish(&self, post: DiaryPost) -> Result<PublishOutcome>`
  - `lib.rs` から `PublishDiaryPostUseCase, PublishOutcome` を公開する

- [ ] **Step 1: 失敗するテストを書く**

`lib.rs` に `mod publish_diary_post;` と `pub use publish_diary_post::{PublishDiaryPostUseCase, PublishOutcome};` を追加する。

`publish_diary_post.rs`:

```rust
//! bot が作った内容を日報日のスレッドと Notion ページへ載せるユースケース。

#[cfg(test)]
mod tests {
    use anyhow::anyhow;
    use mockall::Sequence;

    use kgd_domain::{DiaryPostImage, ThreadState};

    use crate::{
        ports::{
            MockDiaryPostRepository, MockDiaryRepository, MockDiscordGateway, MockNotionApi,
        },
        test_support::{entry, fixed_clock, utc},
    };

    use super::*;

    const THREAD_ID: u64 = 10;

    fn calendar() -> DiaryCalendar {
        DiaryCalendar::new(chrono_tz::Asia::Tokyo, 8)
    }

    fn date() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 9, 28).unwrap()
    }

    fn post() -> DiaryPost {
        DiaryPost {
            key: "location-report:2026-09-28".to_string(),
            date: date(),
            text: "位置ログ 2026-09-28".to_string(),
            images: vec![DiaryPostImage {
                filename: "location-2026-09-28.png".to_string(),
                content_type: "image/png".to_string(),
                bytes: vec![1, 2, 3],
            }],
        }
    }

    fn record(notion: bool, thread: bool) -> DiaryPostRecord {
        let at = utc(2026, 9, 28, 23, 0);
        DiaryPostRecord {
            key: post().key,
            diary_date: date(),
            notion_posted_at: notion.then_some(at),
            thread_message_id: thread.then_some(1),
            thread_posted_at: thread.then_some(at),
            skipped_at: None,
        }
    }

    fn thread_state(closed: bool) -> ThreadState {
        ThreadState {
            is_public_thread: true,
            archived: closed,
            locked: closed,
        }
    }

    /// その日の日報エントリを返す DiaryRepository。
    fn diary_with_entry() -> MockDiaryRepository {
        let mut diary = MockDiaryRepository::new();
        let expected = calendar().start_of(date());
        diary
            .expect_get_by_date()
            .withf(move |d| *d == expected)
            .returning(move |d| Ok(Some(entry(THREAD_ID, d))));
        diary
    }

    /// 画像 1 枚のアップロードと追記に成功する NotionApi。
    fn notion_ok() -> MockNotionApi {
        let mut notion = MockNotionApi::new();
        notion
            .expect_upload_file()
            .withf(|filename, content_type, _| {
                filename == "location-2026-09-28.png" && content_type == "image/png"
            })
            .times(1)
            .returning(|_, _, _| Ok("upload-1".to_string()));
        notion
            .expect_append_blocks()
            .withf(|page_id, children| {
                page_id == format!("page-{THREAD_ID}")
                    && children.len() == 2
                    && children[0]["type"] == "paragraph"
                    && children[1]["type"] == "image"
            })
            .times(1)
            .returning(|_, _| Ok(vec!["b1".to_string(), "b2".to_string()]));
        notion
    }

    fn use_case(
        diary: MockDiaryRepository,
        posts: MockDiaryPostRepository,
        notion: MockNotionApi,
        discord: MockDiscordGateway,
    ) -> PublishDiaryPostUseCase {
        PublishDiaryPostUseCase::new(
            Arc::new(diary),
            Arc::new(posts),
            Arc::new(notion),
            Arc::new(discord),
            Arc::new(fixed_clock(utc(2026, 9, 28, 23, 1))),
            calendar(),
        )
    }

    /// 開いているスレッドへは再開もクローズもせずに投稿し、Notion、スレッドの順に記録することを確認する。
    #[tokio::test]
    async fn publish_posts_to_notion_then_thread_and_records_both() {
        let mut seq = Sequence::new();
        let mut posts = MockDiaryPostRepository::new();
        posts.expect_get().returning(|_| Ok(None));
        posts
            .expect_mark_notion_posted()
            .times(1)
            .in_sequence(&mut seq)
            .returning(|_, _, _| Ok(()));
        let mut discord = MockDiscordGateway::new();
        discord
            .expect_thread_state()
            .returning(|_| Ok(Some(thread_state(false))));
        discord.expect_reopen_thread().times(0);
        discord.expect_close_thread().times(0);
        discord
            .expect_send_text_with_images()
            .withf(|channel_id, content, images| {
                *channel_id == THREAD_ID && content == "位置ログ 2026-09-28" && images.len() == 1
            })
            .times(1)
            .in_sequence(&mut seq)
            .returning(|_, _, _| Ok(555));
        posts
            .expect_mark_thread_posted()
            .withf(|key, _, message_id, _| key == "location-report:2026-09-28" && *message_id == 555)
            .times(1)
            .in_sequence(&mut seq)
            .returning(|_, _, _, _| Ok(()));

        let outcome = use_case(diary_with_entry(), posts, notion_ok(), discord)
            .publish(post())
            .await
            .unwrap();

        assert_eq!(outcome, PublishOutcome::Published);
    }

    /// 完了済みの記録があれば、何も載せずに AlreadyDone を返すことを確認する。
    #[tokio::test]
    async fn publish_returns_already_done_without_side_effects() {
        let mut posts = MockDiaryPostRepository::new();
        posts.expect_get().returning(|_| Ok(Some(record(true, true))));
        let mut diary = MockDiaryRepository::new();
        diary.expect_get_by_date().times(0);

        let outcome = use_case(diary, posts, MockNotionApi::new(), MockDiscordGateway::new())
            .publish(post())
            .await
            .unwrap();

        assert_eq!(outcome, PublishOutcome::AlreadyDone);
    }

    /// 日報が無い日はスキップとして記録し、どこにも載せないことを確認する。
    #[tokio::test]
    async fn publish_marks_skipped_when_no_diary() {
        let mut posts = MockDiaryPostRepository::new();
        posts.expect_get().returning(|_| Ok(None));
        posts
            .expect_mark_skipped()
            .withf(|key, date, _| key == "location-report:2026-09-28" && *date == self::date())
            .times(1)
            .returning(|_, _, _| Ok(()));
        let mut diary = MockDiaryRepository::new();
        diary.expect_get_by_date().returning(|_| Ok(None));

        let outcome = use_case(diary, posts, MockNotionApi::new(), MockDiscordGateway::new())
            .publish(post())
            .await
            .unwrap();

        assert_eq!(outcome, PublishOutcome::NoDiary);
    }

    /// Notion だけ済んでいる記録からは、スレッドへの投稿だけをやり直すことを確認する。
    ///
    /// 途中で失敗した後の再試行で Notion に二重に載せないため。
    #[tokio::test]
    async fn publish_resumes_from_thread_when_notion_is_done() {
        let mut posts = MockDiaryPostRepository::new();
        posts.expect_get().returning(|_| Ok(Some(record(true, false))));
        posts.expect_mark_notion_posted().times(0);
        posts
            .expect_mark_thread_posted()
            .times(1)
            .returning(|_, _, _, _| Ok(()));
        let mut discord = MockDiscordGateway::new();
        discord
            .expect_thread_state()
            .returning(|_| Ok(Some(thread_state(false))));
        discord
            .expect_send_text_with_images()
            .times(1)
            .returning(|_, _, _| Ok(555));

        let outcome = use_case(diary_with_entry(), posts, MockNotionApi::new(), discord)
            .publish(post())
            .await
            .unwrap();

        assert_eq!(outcome, PublishOutcome::Published);
    }

    /// クローズ済みのスレッドは再開してから投稿し、投稿後にクローズへ戻すことを確認する。
    #[tokio::test]
    async fn publish_reopens_closed_thread_and_closes_it_again() {
        let mut seq = Sequence::new();
        let mut posts = MockDiaryPostRepository::new();
        posts.expect_get().returning(|_| Ok(Some(record(true, false))));
        posts
            .expect_mark_thread_posted()
            .returning(|_, _, _, _| Ok(()));
        let mut discord = MockDiscordGateway::new();
        discord
            .expect_thread_state()
            .returning(|_| Ok(Some(thread_state(true))));
        discord
            .expect_reopen_thread()
            .times(1)
            .in_sequence(&mut seq)
            .returning(|_| Ok(true));
        discord
            .expect_send_text_with_images()
            .times(1)
            .in_sequence(&mut seq)
            .returning(|_, _, _| Ok(555));
        discord
            .expect_close_thread()
            .withf(|thread_id| *thread_id == THREAD_ID)
            .times(1)
            .in_sequence(&mut seq)
            .returning(|_| Ok(()));

        let outcome = use_case(diary_with_entry(), posts, MockNotionApi::new(), discord)
            .publish(post())
            .await
            .unwrap();

        assert_eq!(outcome, PublishOutcome::Published);
    }

    /// 投稿に失敗してもクローズへ戻し、スレッドの完了は記録しないことを確認する。
    ///
    /// 開いたまま残すと、閉じたはずの日報に人が書き込めてしまうため。
    #[tokio::test]
    async fn publish_closes_thread_even_when_send_fails() {
        let mut posts = MockDiaryPostRepository::new();
        posts.expect_get().returning(|_| Ok(Some(record(true, false))));
        posts.expect_mark_thread_posted().times(0);
        let mut discord = MockDiscordGateway::new();
        discord
            .expect_thread_state()
            .returning(|_| Ok(Some(thread_state(true))));
        discord.expect_reopen_thread().returning(|_| Ok(true));
        discord
            .expect_send_text_with_images()
            .returning(|_, _, _| Err(anyhow!("discord is down")));
        discord.expect_close_thread().times(1).returning(|_| Ok(()));

        let result = use_case(diary_with_entry(), posts, MockNotionApi::new(), discord)
            .publish(post())
            .await;

        assert!(result.is_err());
    }

    /// 投稿に成功した後でクローズへ戻せなくても、投稿済みとして記録することを確認する。
    ///
    /// ここで失敗を返すと、次の tick で同じ内容をスレッドへもう一度投稿してしまうため。
    #[tokio::test]
    async fn publish_records_thread_post_even_when_reclose_fails() {
        let mut posts = MockDiaryPostRepository::new();
        posts.expect_get().returning(|_| Ok(Some(record(true, false))));
        posts
            .expect_mark_thread_posted()
            .times(1)
            .returning(|_, _, _, _| Ok(()));
        let mut discord = MockDiscordGateway::new();
        discord
            .expect_thread_state()
            .returning(|_| Ok(Some(thread_state(true))));
        discord.expect_reopen_thread().returning(|_| Ok(true));
        discord
            .expect_send_text_with_images()
            .returning(|_, _, _| Ok(555));
        discord
            .expect_close_thread()
            .returning(|_| Err(anyhow!("missing permission")));

        let outcome = use_case(diary_with_entry(), posts, MockNotionApi::new(), discord)
            .publish(post())
            .await
            .unwrap();

        assert_eq!(outcome, PublishOutcome::Published);
    }

    /// 再開できなかったときは投稿せずに失敗を返すことを確認する。
    #[tokio::test]
    async fn publish_fails_without_posting_when_reopen_fails() {
        let mut posts = MockDiaryPostRepository::new();
        posts.expect_get().returning(|_| Ok(Some(record(true, false))));
        let mut discord = MockDiscordGateway::new();
        discord
            .expect_thread_state()
            .returning(|_| Ok(Some(thread_state(true))));
        discord.expect_reopen_thread().returning(|_| Ok(false));
        discord.expect_send_text_with_images().times(0);
        discord.expect_close_thread().times(0);

        let result = use_case(diary_with_entry(), posts, MockNotionApi::new(), discord)
            .publish(post())
            .await;

        assert!(result.is_err());
    }

    /// is_done が記録の完了状態をそのまま返すことを確認する。
    #[tokio::test]
    async fn is_done_reflects_the_record() {
        let mut posts = MockDiaryPostRepository::new();
        posts
            .expect_get()
            .withf(|key| key == "done")
            .returning(|_| Ok(Some(record(true, true))));
        posts
            .expect_get()
            .withf(|key| key == "partial")
            .returning(|_| Ok(Some(record(true, false))));
        posts
            .expect_get()
            .withf(|key| key == "missing")
            .returning(|_| Ok(None));
        let use_case = use_case(
            MockDiaryRepository::new(),
            posts,
            MockNotionApi::new(),
            MockDiscordGateway::new(),
        );

        assert!(use_case.is_done("done").await.unwrap());
        assert!(!use_case.is_done("partial").await.unwrap());
        assert!(!use_case.is_done("missing").await.unwrap());
    }
}
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cargo test -p kgd-application publish_diary_post`
Expected: FAIL (`cannot find struct PublishDiaryPostUseCase`)

- [ ] **Step 3: 実装する**

```rust
use std::sync::Arc;

use anyhow::{Context as _, Result, ensure};
use chrono::NaiveDate;
use tracing::{info, warn};

use kgd_domain::{
    DiaryCalendar, DiaryPost, DiaryPostRecord, image_block_json, paragraph_block_json,
};

use super::ports::{Clock, DiaryPostRepository, DiaryRepository, DiscordGateway, NotionApi};

/// 日報への投稿の結果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PublishOutcome {
    /// 今回の呼び出しで載せ終えた
    Published,
    /// 以前に載せ終えていた、またはスキップ済みだった
    AlreadyDone,
    /// 日報が無いためスキップとして記録した
    NoDiary,
}

/// bot が作った内容を日報日のスレッドと Notion ページへ載せるユースケース。
///
/// Notion、スレッドの順に載せ、段ごとに完了を記録する。途中で失敗しても、
/// 次の呼び出しで残りの段だけをやり直す。
pub struct PublishDiaryPostUseCase {
    /// 日報リポジトリポート
    diary: Arc<dyn DiaryRepository>,
    /// 投稿の記録ポート
    posts: Arc<dyn DiaryPostRepository>,
    /// Notion API ポート
    notion: Arc<dyn NotionApi>,
    /// Discord ポート
    discord: Arc<dyn DiscordGateway>,
    /// 時刻ポート
    clock: Arc<dyn Clock>,
    /// 日報日の区切り方
    calendar: DiaryCalendar,
}

impl PublishDiaryPostUseCase {
    /// 新しい PublishDiaryPostUseCase を作成する。
    pub fn new(
        diary: Arc<dyn DiaryRepository>,
        posts: Arc<dyn DiaryPostRepository>,
        notion: Arc<dyn NotionApi>,
        discord: Arc<dyn DiscordGateway>,
        clock: Arc<dyn Clock>,
        calendar: DiaryCalendar,
    ) -> Self {
        Self {
            diary,
            posts,
            notion,
            discord,
            clock,
            calendar,
        }
    }

    /// キーの投稿がもう何もしなくてよい状態かを返す。
    ///
    /// 呼び出し側が、重い内容の生成を省くために使う。
    pub async fn is_done(&self, key: &str) -> Result<bool> {
        Ok(self
            .posts
            .get(key)
            .await?
            .is_some_and(|record| record.is_done()))
    }

    /// 投稿を日報日のスレッドと Notion ページへ載せる。
    pub async fn publish(&self, post: DiaryPost) -> Result<PublishOutcome> {
        let record = self.posts.get(&post.key).await?;
        if record.as_ref().is_some_and(DiaryPostRecord::is_done) {
            return Ok(PublishOutcome::AlreadyDone);
        }

        let Some(entry) = self.diary.get_by_date(self.calendar.start_of(post.date)).await? else {
            info!(key = %post.key, date = %post.date, "No diary entry for the post, skipping");
            self.posts
                .mark_skipped(&post.key, post.date, self.clock.now())
                .await?;
            return Ok(PublishOutcome::NoDiary);
        };

        if !record.as_ref().is_some_and(DiaryPostRecord::notion_done) {
            self.append_to_notion(&entry.page_id, &post).await?;
            self.posts
                .mark_notion_posted(&post.key, post.date, self.clock.now())
                .await?;
        }

        if !record.as_ref().is_some_and(DiaryPostRecord::thread_done) {
            let message_id = self.post_to_thread(entry.thread_id, &post).await?;
            self.record_thread_post(&post.key, post.date, message_id)
                .await?;
        }

        Ok(PublishOutcome::Published)
    }

    /// 本文の段落と画像のブロックを Notion ページの末尾へ追加する。
    async fn append_to_notion(&self, page_id: &str, post: &DiaryPost) -> Result<()> {
        let mut children = vec![paragraph_block_json(&post.text)];
        for image in &post.images {
            let upload_id = self
                .notion
                .upload_file(&image.filename, &image.content_type, image.bytes.clone())
                .await
                .context("Failed to upload diary post image to Notion")?;
            children.push(image_block_json(&upload_id));
        }
        self.notion
            .append_blocks(page_id, children)
            .await
            .context("Failed to append diary post to Notion")?;
        Ok(())
    }

    /// スレッドへ投稿し、メッセージ ID を返す。
    ///
    /// クローズ済みなら再開してから投稿し、投稿の成否にかかわらずクローズへ戻す。
    /// 戻せなかった場合は警告に留める。失敗を返すと、次の呼び出しで二重に投稿してしまうため。
    async fn post_to_thread(&self, thread_id: u64, post: &DiaryPost) -> Result<u64> {
        let state = self
            .discord
            .thread_state(thread_id)
            .await?
            .with_context(|| format!("Diary thread {thread_id} is not accessible"))?;

        let reopened = state.is_closed();
        if reopened {
            ensure!(
                self.discord.reopen_thread(thread_id).await?,
                "Failed to reopen diary thread {thread_id}"
            );
        }

        let sent = self
            .discord
            .send_text_with_images(thread_id, &post.text, &post.images)
            .await;

        if reopened && let Err(error) = self.discord.close_thread(thread_id).await {
            warn!(?error, thread_id, "Failed to close diary thread again after posting");
        }

        sent
    }

    /// スレッドへの投稿を記録する。
    async fn record_thread_post(&self, key: &str, date: NaiveDate, message_id: u64) -> Result<()> {
        self.posts
            .mark_thread_posted(key, date, message_id, self.clock.now())
            .await
    }
}
```

`if reopened && let Err(error) = ...` は Rust 2024 の let chains (1.88 以降で安定) を使う。workspace の `rust-version` は 1.92 なので使える。

- [ ] **Step 4: テストが通ることを確認する**

Run: `cargo test -p kgd-application publish_diary_post`
Expected: PASS

- [ ] **Step 5: コミットする**

```bash
~/.nix-profile/bin/just validate
git add crates/kgd-application/src
git commit -m "feat(application): bot の投稿を日報のスレッドと Notion へ載せるユースケースを追加する" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 13: 定時ジョブ

**Files:**
- Create: `crates/kgd-application/src/daily_location_report.rs`
- Modify: `crates/kgd-application/src/lib.rs`
- Test: `crates/kgd-application/src/daily_location_report.rs`

**Interfaces:**
- Consumes: Task 1 の `DiaryCalendar::previous_date`、Task 5 の `format_location_report`、`format_empty_location_report`、Task 6 の `DiaryPost`、`DiaryPostImage`、Task 11 の `BuildLocationReportUseCase`、`LocationReport`、Task 12 の `PublishDiaryPostUseCase`、既存の `ScheduledJob`、`Clock`
- Produces:
  - `pub struct DailyLocationReportJob`、`pub fn new(build: Arc<BuildLocationReportUseCase>, publish: Arc<PublishDiaryPostUseCase>, clock: Arc<dyn Clock>, calendar: DiaryCalendar) -> Self`
  - `impl ScheduledJob for DailyLocationReportJob` (`name()` は `"daily_location_report"`)
  - `lib.rs` から `DailyLocationReportJob` を公開する

- [ ] **Step 1: 失敗するテストを書く**

`lib.rs` に `mod daily_location_report;` と `pub use daily_location_report::DailyLocationReportJob;` を追加する。

`daily_location_report.rs`:

```rust
//! 前の日報日の位置ログのレポートを、その日の日報へ載せる定時ジョブ。

#[cfg(test)]
mod tests {
    use chrono::{DateTime, TimeDelta, TimeZone as _, Utc};

    use kgd_domain::{DiaryPostRecord, LocationSummary};

    use crate::{
        LocationReportSettings,
        ports::{
            MockDiaryPostRepository, MockDiaryRepository, MockDiscordGateway, MockLocationRepository,
            MockMapRenderer, MockNotionApi,
        },
        test_support::{entry, fixed_clock},
    };

    use super::*;

    /// Asia/Tokyo の時刻を UTC で作る。
    fn jst(day: u32, hour: u32, min: u32) -> DateTime<Utc> {
        chrono_tz::Asia::Tokyo
            .with_ymd_and_hms(2026, 9, day, hour, min, 0)
            .unwrap()
            .to_utc()
    }

    fn calendar() -> DiaryCalendar {
        DiaryCalendar::new(chrono_tz::Asia::Tokyo, 8)
    }

    fn build_use_case(repo: MockLocationRepository) -> Arc<BuildLocationReportUseCase> {
        Arc::new(BuildLocationReportUseCase::new(
            Arc::new(repo),
            Arc::new(MockMapRenderer::new()),
            LocationReportSettings {
                calendar: calendar(),
                max_accuracy_m: 200,
                image_width: 1024,
                image_height: 1024,
            },
        ))
    }

    fn publish_use_case(
        diary: MockDiaryRepository,
        posts: MockDiaryPostRepository,
        notion: MockNotionApi,
        discord: MockDiscordGateway,
    ) -> Arc<PublishDiaryPostUseCase> {
        Arc::new(PublishDiaryPostUseCase::new(
            Arc::new(diary),
            Arc::new(posts),
            Arc::new(notion),
            Arc::new(discord),
            Arc::new(fixed_clock(jst(29, 9, 0))),
            calendar(),
        ))
    }

    /// 前の日報日の投稿が済んでいれば、レポートを作らずに終えることを確認する。
    ///
    /// 毎分の tick で地図を描き直さないため。
    #[tokio::test]
    async fn tick_skips_building_when_already_done() {
        let mut posts = MockDiaryPostRepository::new();
        posts
            .expect_get()
            .withf(|key| key == "location-report:2026-09-28")
            .returning(|key| {
                Ok(Some(DiaryPostRecord {
                    key: key.to_string(),
                    diary_date: NaiveDate::from_ymd_opt(2026, 9, 28).unwrap(),
                    notion_posted_at: None,
                    thread_message_id: None,
                    thread_posted_at: None,
                    skipped_at: Some(jst(29, 8, 1)),
                }))
            });
        let mut repo = MockLocationRepository::new();
        repo.expect_locations_between().times(0);
        let job = DailyLocationReportJob::new(
            build_use_case(repo),
            publish_use_case(
                MockDiaryRepository::new(),
                posts,
                MockNotionApi::new(),
                MockDiscordGateway::new(),
            ),
            Arc::new(fixed_clock(jst(29, 9, 0))),
            calendar(),
        );

        job.tick().await.unwrap();
    }

    /// 記録が無い日は「記録なし」の本文だけを、画像なしで日報へ載せることを確認する。
    #[tokio::test]
    async fn tick_publishes_empty_report_when_no_points() {
        let mut posts = MockDiaryPostRepository::new();
        posts.expect_get().times(2).returning(|_| Ok(None));
        posts.expect_mark_notion_posted().returning(|_, _, _| Ok(()));
        posts
            .expect_mark_thread_posted()
            .returning(|_, _, _, _| Ok(()));
        let mut repo = MockLocationRepository::new();
        repo.expect_locations_between()
            .withf(|start, end| *start == jst(28, 8, 0) && *end == jst(29, 8, 0))
            .returning(|_, _| Ok(vec![]));
        let mut diary = MockDiaryRepository::new();
        diary
            .expect_get_by_date()
            .returning(|d| Ok(Some(entry(10, d))));
        let mut notion = MockNotionApi::new();
        notion.expect_upload_file().times(0);
        notion
            .expect_append_blocks()
            .withf(|_, children| {
                children.len() == 1
                    && children[0]["paragraph"]["rich_text"][0]["text"]["content"]
                        == "位置ログ 2026-09-28 記録なし"
            })
            .times(1)
            .returning(|_, _| Ok(vec!["b1".to_string()]));
        let mut discord = MockDiscordGateway::new();
        discord.expect_thread_state().returning(|_| {
            Ok(Some(kgd_domain::ThreadState {
                is_public_thread: true,
                archived: false,
                locked: false,
            }))
        });
        discord
            .expect_send_text_with_images()
            .withf(|_, content, images| content == "位置ログ 2026-09-28 記録なし" && images.is_empty())
            .times(1)
            .returning(|_, _, _| Ok(555));
        let job = DailyLocationReportJob::new(
            build_use_case(repo),
            publish_use_case(diary, posts, notion, discord),
            Arc::new(fixed_clock(jst(29, 9, 0))),
            calendar(),
        );

        job.tick().await.unwrap();
    }

    /// 画像のあるレポートは、本文と PNG 1 枚の投稿に変換されることを確認する。
    #[test]
    fn to_diary_post_attaches_png_with_report_text() {
        let date = NaiveDate::from_ymd_opt(2026, 9, 28).unwrap();
        let report = LocationReport {
            date,
            range: (jst(28, 8, 0), jst(29, 8, 0)),
            summary: LocationSummary {
                point_count: 2,
                excluded_count: 0,
                distance_m: 1_000.0,
                distance_by_activity: vec![],
                moving: TimeDelta::minutes(10),
                stationary: TimeDelta::zero(),
                first_at: Some(jst(28, 9, 0)),
                last_at: Some(jst(28, 9, 10)),
            },
            image: Some(vec![1, 2, 3]),
        };

        let post = to_diary_post(location_report_key(date), &report, chrono_tz::Asia::Tokyo);

        assert_eq!(post.key, "location-report:2026-09-28");
        assert_eq!(post.date, date);
        assert!(post.text.starts_with("位置ログ 2026-09-28 (08:00〜翌 08:00)\n"));
        assert!(post.text.ends_with("© OpenStreetMap contributors"));
        assert_eq!(post.images.len(), 1);
        assert_eq!(post.images[0].filename, "location-2026-09-28.png");
        assert_eq!(post.images[0].content_type, "image/png");
        assert_eq!(post.images[0].bytes, vec![1, 2, 3]);
    }
}
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cargo test -p kgd-application daily_location_report`
Expected: FAIL (`cannot find struct DailyLocationReportJob`)

- [ ] **Step 3: 実装する**

```rust
use std::sync::Arc;

use anyhow::Result;
use chrono::NaiveDate;
use chrono_tz::Tz;
use tracing::info;

use kgd_domain::{
    DiaryCalendar, DiaryPost, DiaryPostImage, format_empty_location_report, format_location_report,
};

use super::{
    BuildLocationReportUseCase, LocationReport, PublishDiaryPostUseCase, ScheduledJob,
    ports::Clock,
};

/// 前の日報日の位置ログのレポートを、その日の日報へ載せる定時ジョブ。
///
/// 対象は常に「1 つ前の日報日」だけとし、完了するまで tick ごとに再試行する。
pub struct DailyLocationReportJob {
    /// レポートを作るユースケース
    build: Arc<BuildLocationReportUseCase>,
    /// 日報へ載せるユースケース
    publish: Arc<PublishDiaryPostUseCase>,
    /// 時刻ポート
    clock: Arc<dyn Clock>,
    /// 日報日の区切り方
    calendar: DiaryCalendar,
}

impl DailyLocationReportJob {
    /// 新しい DailyLocationReportJob を作成する。
    pub fn new(
        build: Arc<BuildLocationReportUseCase>,
        publish: Arc<PublishDiaryPostUseCase>,
        clock: Arc<dyn Clock>,
        calendar: DiaryCalendar,
    ) -> Self {
        Self {
            build,
            publish,
            clock,
            calendar,
        }
    }
}

#[async_trait::async_trait]
impl ScheduledJob for DailyLocationReportJob {
    fn name(&self) -> &'static str {
        "daily_location_report"
    }

    async fn tick(&self) -> Result<()> {
        let date = self.calendar.previous_date(self.clock.now());
        let key = location_report_key(date);
        if self.publish.is_done(&key).await? {
            return Ok(());
        }

        let report = self.build.build(date, None).await?;
        let post = to_diary_post(key, &report, *self.calendar.timezone());
        let outcome = self.publish.publish(post).await?;
        info!(%date, ?outcome, "Daily location report handled");
        Ok(())
    }
}

/// 日次レポートの投稿キーを返す。
fn location_report_key(date: NaiveDate) -> String {
    format!("location-report:{}", date.format("%Y-%m-%d"))
}

/// レポートを日報への投稿に変換する。画像が無ければ「記録なし」の本文だけにする。
fn to_diary_post(key: String, report: &LocationReport, timezone: Tz) -> DiaryPost {
    match &report.image {
        Some(png) => DiaryPost {
            key,
            date: report.date,
            text: format_location_report(report.date, report.range, &report.summary, timezone)
                .to_plain_text(),
            images: vec![DiaryPostImage {
                filename: format!("location-{}.png", report.date.format("%Y-%m-%d")),
                content_type: "image/png".to_string(),
                bytes: png.clone(),
            }],
        },
        None => DiaryPost {
            key,
            date: report.date,
            text: format_empty_location_report(report.date),
            images: Vec::new(),
        },
    }
}
```

- [ ] **Step 4: テストが通ることを確認する**

Run: `cargo test -p kgd-application daily_location_report`
Expected: PASS

- [ ] **Step 5: コミットする**

```bash
~/.nix-profile/bin/just validate
git add crates/kgd-application/src
git commit -m "feat(application): 前の日報日の位置ログを日報へ載せる定時ジョブを追加する" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 14: 設定と定時ジョブの配線

**Files:**
- Modify: `crates/kgd/src/config/mod.rs`、`crates/kgd/src/config/defaults.rs`、`crates/kgd/src/config/tests.rs`
- Modify: `crates/kgd/src/bootstrap.rs`
- Modify: `config.example.toml`、`compose.yml`、`Dockerfile`

**Interfaces:**
- Consumes: Task 8 の `DiaryPostStore`、Task 10 の `TileMapRenderer`、Task 11 の `BuildLocationReportUseCase`、`LocationReportSettings`、Task 12 の `PublishDiaryPostUseCase`、Task 13 の `DailyLocationReportJob`
- Produces:
  - `LocationConfig` の追加フィールド: `daily_report_enabled: bool` (既定 true)、`max_accuracy_m: i32` (既定 200)、`image_width: u32` (既定 1024)、`image_height: u32` (既定 1024)、`tile_cache_dir: PathBuf` (既定 `/var/cache/kgd/tiles`)
  - bootstrap のローカル変数 `location_report: Option<Arc<BuildLocationReportUseCase>>` (Task 15 がコントローラへ渡す)

- [ ] **Step 1: 失敗するテストを書く**

`crates/kgd/src/config/tests.rs` に追加する。

```rust
/// [location] にレポートの項目を書かなければ既定値になることを確認する。
#[test]
fn location_report_settings_have_defaults() {
    let toml_text = format!(
        "{}\n[location]\nusername = \"ekuinox\"\npassword = \"secret\"\n",
        minimal_config_toml("")
    );

    let config: Config = toml::from_str(&toml_text).expect("should parse");
    let location = config.location.expect("should be present");

    assert!(location.daily_report_enabled);
    assert_eq!(location.max_accuracy_m, 200);
    assert_eq!(location.image_width, 1024);
    assert_eq!(location.image_height, 1024);
    assert_eq!(location.tile_cache_dir, PathBuf::from("/var/cache/kgd/tiles"));
}

/// 地図画像の大きさに 0 を書くと検証で弾かれることを確認する。
///
/// 0 のままだと描画のたびに失敗し、定時ジョブが毎分エラーになるため。
#[test]
fn validate_rejects_zero_image_size() {
    let toml_text = format!(
        "{}\n[location]\nusername = \"ekuinox\"\npassword = \"secret\"\nimage_width = 0\n",
        minimal_config_toml("")
    );

    let config: Config = toml::from_str(&toml_text).expect("should parse");
    let error = config.validate().expect_err("should be rejected");
    assert!(error.to_string().contains("image_width"));
}
```

テストファイル冒頭の `use std::net::SocketAddr;` を `use std::{net::SocketAddr, path::PathBuf};` にする。

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cargo test -p kgd config`
Expected: FAIL (`no field daily_report_enabled on type LocationConfig`)

- [ ] **Step 3: 設定を実装する**

`defaults.rs` に追加する (`use std::{net::SocketAddr, path::PathBuf, time::Duration};` にする)。

```rust
pub(super) fn default_daily_report_enabled() -> bool {
    true
}

pub(super) fn default_max_accuracy_m() -> i32 {
    200
}

pub(super) fn default_image_size() -> u32 {
    1024
}

pub(super) fn default_tile_cache_dir() -> PathBuf {
    PathBuf::from("/var/cache/kgd/tiles")
}
```

`config/mod.rs` の `LocationConfig` にフィールドを追加する (`use std::{fs, net::SocketAddr, path::{Path, PathBuf}, time::Duration};` にする)。

```rust
    /// 前の日報日のレポートを日報へ自動で載せるか（デフォルト: true）
    #[serde(default = "default_daily_report_enabled")]
    pub daily_report_enabled: bool,
    /// これを超える水平精度 (メートル) の点をレポートから除く（デフォルト: 200）
    #[serde(default = "default_max_accuracy_m")]
    pub max_accuracy_m: i32,
    /// 地図画像の幅 (ピクセル)（デフォルト: 1024）
    #[serde(default = "default_image_size")]
    pub image_width: u32,
    /// 地図画像の高さ (ピクセル)（デフォルト: 1024）
    #[serde(default = "default_image_size")]
    pub image_height: u32,
    /// 地図タイルのキャッシュ先（デフォルト: /var/cache/kgd/tiles）
    #[serde(default = "default_tile_cache_dir")]
    pub tile_cache_dir: PathBuf,
```

`Config::validate` の末尾 (`Ok(())` の前) に追加する。

```rust
        if let Some(location) = &self.location {
            ensure!(
                location.image_width > 0 && location.image_height > 0,
                "location.image_width and location.image_height must be positive, but got {}x{}",
                location.image_width,
                location.image_height
            );
        }
```

既存のテストで `LocationConfig { ... }` を構造体リテラルで作っている箇所があれば、新しいフィールドを既定値で埋める。

- [ ] **Step 4: テストが通ることを確認する**

Run: `cargo test -p kgd config`
Expected: PASS

- [ ] **Step 5: bootstrap に配線する**

`crates/kgd/src/bootstrap.rs` の `use` を次のように広げる。

```rust
use kgd_application::{
    AutoCloseJob, BuildLocationReportUseCase, DailyLocationReportJob, DiaryLifecycleSettings,
    DiaryMaintenanceSettings, HourlySyncJob, LocationReportSettings, ManageDiaryLifecycleUseCase,
    PublishDiaryPostUseCase, RecordLocationUseCase, RelaySettings,
    RelayWriteChannelMessageUseCase, RunDiaryMaintenanceUseCase, SyncDiaryMessageUseCase,
    WakeServerUseCase,
    ports::{
        AttachmentDownloader, Clock, DiaryPostRepository, DiaryRepository, DiscordGateway,
        ImageConverter, LocationRepository, MapRenderer, NotionApi, OgpClient, WolSender,
    },
    run_relay_worker,
};
use kgd_infrastructure::{
    DiaryPostStore, DiaryStore, HeifConverter, LocationStore, NotionClient, OgpFetcher,
    ReqwestDownloader, Scheduler, SerenityGateway, SystemClock, TileMapRenderer, UdpWolSender,
    bind_http, connect_pool, serve_http,
};
```

`let calendar = DiaryCalendar::new(...);` の直後に追加する。`notion_client`、`gateway`、`clock` はこの後で lifecycle と relay へムーブされるため、ここでは `clone()` して使う。

```rust
    // 位置ログのレポート。`[location]` が無ければ作らない。
    // スラッシュコマンドと定時ジョブの両方が同じユースケースを使う。
    // (Task 15 でコマンドへ渡すまでは未使用のため、先頭に _ を付けておく)
    let mut _location_report: Option<Arc<BuildLocationReportUseCase>> = None;
    let mut location_report_job: Option<Arc<DailyLocationReportJob>> = None;
    if let Some(location_config) = &config.location {
        let location_store: Arc<dyn LocationRepository> =
            Arc::new(LocationStore::new(pool.clone()));
        // OSM のタイル利用規約は識別可能な User-Agent を求める
        let user_agent = format!("kgd/{} (+https://github.com/ekuinox/kgd)", version::VERSION);
        let renderer: Arc<dyn MapRenderer> = Arc::new(
            TileMapRenderer::new(&user_agent, &location_config.tile_cache_dir)
                .context("Failed to create map renderer")?,
        );
        let build = Arc::new(BuildLocationReportUseCase::new(
            location_store,
            renderer,
            LocationReportSettings {
                calendar,
                max_accuracy_m: location_config.max_accuracy_m,
                image_width: location_config.image_width,
                image_height: location_config.image_height,
            },
        ));
        if location_config.daily_report_enabled {
            let publish = Arc::new(PublishDiaryPostUseCase::new(
                diary_store.clone(),
                Arc::new(DiaryPostStore::new(pool.clone())) as Arc<dyn DiaryPostRepository>,
                notion_client.clone(),
                gateway.clone(),
                clock.clone(),
                calendar,
            ));
            location_report_job = Some(Arc::new(DailyLocationReportJob::new(
                build.clone(),
                publish,
                clock.clone(),
                calendar,
            )));
        }
        _location_report = Some(build);
    }
```

スケジューラへの登録を次のようにする。

```rust
    let mut scheduler = Scheduler::new(diary_interval);
    scheduler.register(Arc::new(AutoCloseJob(maintenance.clone())));
    scheduler.register(Arc::new(HourlySyncJob(maintenance)));
    if let Some(job) = location_report_job {
        scheduler.register(job);
    }
    tokio::spawn(scheduler.run());
```

- [ ] **Step 6: 設定例とコンテナを更新する**

`config.example.toml` のコメントアウトされた `[location]` の節の末尾に追加する (パース結果が変わらないよう、すべてコメントのままにする)。

```toml
# Post the previous diary day's location report to the diary thread and Notion page
# (default: true)
# daily_report_enabled = true
# Drop points whose horizontal accuracy exceeds this many meters (default: 200)
# max_accuracy_m = 200
# Map image size in pixels (default: 1024 x 1024)
# image_width = 1024
# image_height = 1024
# Directory to cache OpenStreetMap tiles (default: /var/cache/kgd/tiles)
# tile_cache_dir = "/var/cache/kgd/tiles"
```

`Dockerfile` の `runtime-base` ステージで、`RUN useradd -r -s /bin/false kgd` の直後に追加する。新しい名前付きボリュームには、イメージ内のディレクトリの所有者が引き継がれる。

```dockerfile
# 地図タイルのキャッシュ先。名前付きボリュームに kgd ユーザーの所有を引き継がせる
RUN mkdir -p /var/cache/kgd/tiles && chown -R kgd /var/cache/kgd
```

`compose.yml` の `kgd` サービスの `volumes` に追加し、トップレベルの `volumes` に `tiles:` を足す。

```yaml
      - tiles:/var/cache/kgd/tiles
```

- [ ] **Step 7: ビルドとテストが通ることを確認する**

Run: `cargo test -p kgd` と `cargo check --workspace`
Expected: PASS

- [ ] **Step 8: コミットする**

```bash
~/.nix-profile/bin/just validate
git add crates/kgd config.example.toml compose.yml Dockerfile
git commit -m "feat: 位置ログの日次レポートを設定から有効にして定時ジョブへ登録する" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 15: `/location report` コマンド

**Files:**
- Modify: `crates/kgd-presentation/Cargo.toml`
- Create: `crates/kgd-presentation/src/presenter/location.rs`
- Modify: `crates/kgd-presentation/src/presenter/mod.rs`
- Create: `crates/kgd-presentation/src/discord/location_commands.rs`
- Modify: `crates/kgd-presentation/src/discord/mod.rs`、`crates/kgd-presentation/src/discord/commands.rs`、`crates/kgd-presentation/src/discord/events.rs`、`crates/kgd-presentation/src/lib.rs`
- Modify: `crates/kgd/src/bootstrap.rs`
- Test: `crates/kgd-presentation/src/presenter/location.rs`

**Interfaces:**
- Consumes: Task 1 の `DiaryCalendar::local_date`、Task 5 の `format_location_report`、`format_empty_location_report`、`OSM_ATTRIBUTION`、Task 11 の `BuildLocationReportUseCase`、`LocationReport`、既存の `EmbedSpec`、`EmbedField`、`render_embed`、`Clock`
- Produces:
  - `pub enum ReportDateError { InvalidFormat, NotStarted }` と `fn message(&self) -> &'static str`
  - `pub fn resolve_report_date(input: Option<&str>, calendar: &DiaryCalendar, now: DateTime<Utc>) -> Result<NaiveDate, ReportDateError>`
  - `pub fn present_location_report(report: &LocationReport, calendar: &DiaryCalendar) -> EmbedSpec`
  - `pub struct LocationReportCommand { pub build: Arc<BuildLocationReportUseCase>, pub calendar: DiaryCalendar, pub clock: Arc<dyn Clock> }` (`kgd_presentation::LocationReportCommand`)
  - `DiscordController::new` の末尾に引数 `location_report: Option<LocationReportCommand>` を追加する

- [ ] **Step 1: 失敗するテストを書く**

`crates/kgd-presentation/Cargo.toml` の `[dev-dependencies]` に `chrono-tz.workspace = true` を追加する (`chrono` は Task 7 で追加済み)。

`presenter/mod.rs` に `mod location;` と `pub(crate) use location::{present_location_report, resolve_report_date};` を追加する。

`presenter/location.rs`:

```rust
//! 位置ログのレポートの embed と、コマンドの日付入力の解釈。

#[cfg(test)]
mod tests {
    use chrono::{TimeDelta, TimeZone as _};

    use kgd_domain::LocationSummary;

    use super::*;

    /// Asia/Tokyo の時刻を UTC で作る。
    fn jst(day: u32, hour: u32, min: u32) -> DateTime<Utc> {
        chrono_tz::Asia::Tokyo
            .with_ymd_and_hms(2026, 9, day, hour, min, 0)
            .unwrap()
            .to_utc()
    }

    fn calendar() -> DiaryCalendar {
        DiaryCalendar::new(chrono_tz::Asia::Tokyo, 8)
    }

    /// 日付を省略すると、現在の日報日になることを確認する。
    ///
    /// day_start_hour より前は前日の日報日が続いている。
    #[test]
    fn resolve_report_date_defaults_to_current_diary_day() {
        assert_eq!(
            resolve_report_date(None, &calendar(), jst(29, 7, 0)),
            Ok(NaiveDate::from_ymd_opt(2026, 9, 28).unwrap())
        );
        assert_eq!(
            resolve_report_date(None, &calendar(), jst(29, 9, 0)),
            Ok(NaiveDate::from_ymd_opt(2026, 9, 29).unwrap())
        );
    }

    /// YYYY-MM-DD で書いた過去の日付と、今の日報日をそのまま受け付けることを確認する。
    #[test]
    fn resolve_report_date_accepts_past_and_current_dates() {
        assert_eq!(
            resolve_report_date(Some("2026-09-01"), &calendar(), jst(29, 9, 0)),
            Ok(NaiveDate::from_ymd_opt(2026, 9, 1).unwrap())
        );
        assert_eq!(
            resolve_report_date(Some(" 2026-09-29 "), &calendar(), jst(29, 9, 0)),
            Ok(NaiveDate::from_ymd_opt(2026, 9, 29).unwrap())
        );
    }

    /// 書式の違う日付を弾くことを確認する。
    #[test]
    fn resolve_report_date_rejects_invalid_format() {
        assert_eq!(
            resolve_report_date(Some("2026/09/28"), &calendar(), jst(29, 9, 0)),
            Err(ReportDateError::InvalidFormat)
        );
        assert_eq!(
            resolve_report_date(Some("yesterday"), &calendar(), jst(29, 9, 0)),
            Err(ReportDateError::InvalidFormat)
        );
    }

    /// まだ始まっていない日報日を弾くことを確認する。
    #[test]
    fn resolve_report_date_rejects_future_date() {
        assert_eq!(
            resolve_report_date(Some("2026-09-29"), &calendar(), jst(29, 7, 0)),
            Err(ReportDateError::NotStarted)
        );
    }

    /// embed が見出しをタイトルに、集計を各フィールドに、帰属表示をフッターに持つことを確認する。
    #[test]
    fn present_location_report_builds_embed() {
        let date = NaiveDate::from_ymd_opt(2026, 9, 28).unwrap();
        let report = LocationReport {
            date,
            range: (jst(28, 8, 0), jst(29, 8, 0)),
            summary: LocationSummary {
                point_count: 2,
                excluded_count: 0,
                distance_m: 1_000.0,
                distance_by_activity: vec![],
                moving: TimeDelta::minutes(10),
                stationary: TimeDelta::zero(),
                first_at: Some(jst(28, 9, 0)),
                last_at: Some(jst(28, 9, 10)),
            },
            image: Some(vec![1]),
        };

        let embed = present_location_report(&report, &calendar());

        assert_eq!(embed.title, "位置ログ 2026-09-28 (08:00〜翌 08:00)");
        let names: Vec<&str> = embed.fields.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, vec!["移動距離", "時間", "記録", "時刻"]);
        assert_eq!(embed.fields[0].value, "1.0 km");
        assert_eq!(embed.footer.as_deref(), Some("© OpenStreetMap contributors"));
    }
}
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cargo test -p kgd-presentation presenter::location`
Expected: FAIL (`cannot find function resolve_report_date`)

- [ ] **Step 3: Presenter を実装する**

```rust
use chrono::{DateTime, NaiveDate, Utc};

use kgd_application::LocationReport;
use kgd_domain::{DiaryCalendar, OSM_ATTRIBUTION, format_location_report};

use super::{EmbedField, EmbedSpec};

/// 位置ログの embed の色。
const LOCATION_COLOR: u32 = 0x2e9e44;

/// コマンドの日付入力を解釈できなかった理由。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportDateError {
    /// YYYY-MM-DD の形式でない
    InvalidFormat,
    /// まだ始まっていない日報日
    NotStarted,
}

impl ReportDateError {
    /// 本人へ返す文言。
    pub fn message(&self) -> &'static str {
        match self {
            ReportDateError::InvalidFormat => "日付は YYYY-MM-DD の形式で指定してください",
            ReportDateError::NotStarted => "まだ始まっていない日報日です",
        }
    }
}

/// コマンドの日付入力から対象の日報日を決める。省略時は現在の日報日。
pub fn resolve_report_date(
    input: Option<&str>,
    calendar: &DiaryCalendar,
    now: DateTime<Utc>,
) -> Result<NaiveDate, ReportDateError> {
    let current = calendar.local_date(now);
    let Some(input) = input else {
        return Ok(current);
    };
    let date = NaiveDate::parse_from_str(input.trim(), "%Y-%m-%d")
        .map_err(|_| ReportDateError::InvalidFormat)?;
    if date > current {
        return Err(ReportDateError::NotStarted);
    }
    Ok(date)
}

/// 位置ログのレポートの embed を組み立てる。
///
/// 文言は日報に載せる本文と同じ `format_location_report` から作る。
pub fn present_location_report(report: &LocationReport, calendar: &DiaryCalendar) -> EmbedSpec {
    let text = format_location_report(
        report.date,
        report.range,
        &report.summary,
        *calendar.timezone(),
    );
    let field = |name: &str, value: String| EmbedField {
        name: name.to_string(),
        value,
        inline: false,
    };
    EmbedSpec {
        title: text.heading,
        color: LOCATION_COLOR,
        fields: vec![
            field("移動距離", text.distance),
            field("時間", text.durations),
            field("記録", text.points),
            field("時刻", text.times),
        ],
        footer: Some(OSM_ATTRIBUTION.to_string()),
    }
}
```

- [ ] **Step 4: テストが通ることを確認する**

Run: `cargo test -p kgd-presentation presenter::location`
Expected: PASS

- [ ] **Step 5: コマンドを実装する**

`discord/mod.rs` に追加する。

```rust
mod location_commands;
```

```rust
/// `/location report` に必要な依存。
///
/// `[location]` が無いときは作らず、コマンドも登録しない。
#[derive(Clone)]
pub struct LocationReportCommand {
    /// レポートを作るユースケース
    pub build: Arc<BuildLocationReportUseCase>,
    /// 日報日の区切り方
    pub calendar: DiaryCalendar,
    /// 時刻ポート
    pub clock: Arc<dyn Clock>,
}
```

`DiscordController` にフィールド `pub(crate) location_report: Option<LocationReportCommand>,` (doc コメント「位置ログのレポートコマンド (未設定なら None)」) を追加し、`new` の末尾の引数と初期化に加える。`use` に `kgd_application::{BuildLocationReportUseCase, ports::Clock}` と `kgd_domain::DiaryCalendar` を足す。`lib.rs` の `pub use discord::{...}` に `LocationReportCommand` を足す。

`discord/location_commands.rs`:

```rust
//! 位置ログのスラッシュコマンドの処理。

use anyhow::{Context as _, Result, bail};
use serenity::{
    all::{
        CommandDataOptionValue, CommandInteraction, CreateAttachment, CreateInteractionResponse,
        CreateInteractionResponseMessage, EditInteractionResponse,
    },
    client::Context as SerenityContext,
};

use kgd_domain::format_empty_location_report;

use crate::presenter::{present_location_report, render_embed, resolve_report_date};

use super::DiscordController;

impl DiscordController {
    /// `/location` を処理する。
    pub(crate) async fn handle_location(
        &self,
        ctx: &SerenityContext,
        command: &CommandInteraction,
    ) -> Result<()> {
        let Some(location) = &self.location_report else {
            return Ok(());
        };
        let subcommand = command
            .data
            .options
            .first()
            .context("Subcommand not provided")?;
        if subcommand.name != "report" {
            return Ok(());
        }
        let CommandDataOptionValue::SubCommand(options) = &subcommand.value else {
            bail!("Unexpected option value for /location report");
        };
        let input = options
            .iter()
            .find(|option| option.name == "date")
            .and_then(|option| option.value.as_str());

        let now = location.clock.now();
        let date = match resolve_report_date(input, &location.calendar, now) {
            Ok(date) => date,
            Err(error) => {
                let response = CreateInteractionResponseMessage::new()
                    .content(error.message())
                    .ephemeral(true);
                command
                    .create_response(&ctx.http, CreateInteractionResponse::Message(response))
                    .await?;
                return Ok(());
            }
        };

        // 描画に数秒かかりうるため、先に本人だけに見える形で応答を保留する
        command.defer_ephemeral(&ctx.http).await?;

        let report = location.build.build(date, Some(now)).await?;
        let response = match &report.image {
            Some(png) => {
                let filename = format!("location-{}.png", date.format("%Y-%m-%d"));
                let embed = render_embed(&present_location_report(&report, &location.calendar))
                    .attachment(&filename);
                EditInteractionResponse::new()
                    .embed(embed)
                    .new_attachment(CreateAttachment::bytes(png.clone(), filename))
            }
            None => EditInteractionResponse::new().content(format_empty_location_report(date)),
        };
        command.edit_response(&ctx.http, response).await?;

        Ok(())
    }
}
```

`build` が失敗した場合は `Err` を返す。応答を保留した後なので、既存の `interaction_create` はフォローアップでエラーを伝える。

`discord/commands.rs` の `match command.data.name.as_str()` に `"location" => self.handle_location(ctx, command).await,` を足す。

`discord/events.rs` の `ready` で、日報コマンドの登録の後に追加する。

```rust
        // 位置ログのコマンドは [location] があるときだけ登録する
        if self.location_report.is_some() {
            commands.push(
                CreateCommand::new("location")
                    .description("位置ログ")
                    .add_option(
                        CreateCommandOption::new(
                            CommandOptionType::SubCommand,
                            "report",
                            "日報日の位置ログのレポートを本人だけに表示する",
                        )
                        .add_sub_option(CreateCommandOption::new(
                            CommandOptionType::String,
                            "date",
                            "日報日 (YYYY-MM-DD)。省略すると今の日報日",
                        )),
                    ),
            );
        }
```

- [ ] **Step 6: bootstrap からコマンドの依存を渡す**

Task 14 で `_location_report` にした変数名を `location_report` に改め (宣言と代入の 2 か所)、`DiscordController::new(...)` の最後の引数に渡す。`use kgd_presentation::{...}` に `LocationReportCommand` を足す。

```rust
        location_report.map(|build| LocationReportCommand {
            build,
            calendar,
            clock: Arc::new(SystemClock) as Arc<dyn Clock>,
        }),
```

- [ ] **Step 7: ビルドとテストが通ることを確認する**

Run: `cargo test -p kgd-presentation` と `cargo check --workspace`
Expected: PASS

- [ ] **Step 8: コミットする**

```bash
~/.nix-profile/bin/just validate
git add crates/kgd-presentation crates/kgd/src/bootstrap.rs
git commit -m "feat(presentation): 任意の日報日の位置ログを確認する /location report を追加する" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 16: ドキュメント

**Files:**
- Modify: `docs/architecture.md`
- Create: `docs/adr/0009-render-maps-with-tiny-skia.md`
- Create: `docs/adr/0010-do-not-impute-missing-motion.md`
- Create: `docs/adr/0011-publish-bot-posts-to-diary.md`
- Modify: `docs/adr/README.md`

**Interfaces:**
- Consumes: Task 1 から 15 の成果物の名前
- Produces: なし

- [ ] **Step 1: architecture.md を更新する**

「ポートと実装の対応」の表の末尾に追加する。

```markdown
| DiaryPostRepository | DiaryPostStore (sqlx / PostgreSQL) | MockDiaryPostRepository |
| MapRenderer | TileMapRenderer (reqwest + tiny-skia, OSM タイル) | MockMapRenderer |
```

「ユースケース一覧」の表の末尾に追加する。

```markdown
| BuildLocationReportUseCase | 日報日の位置ログから地図画像と集計値を作る (届け先は知らない) |
| PublishDiaryPostUseCase | bot が作った内容を日報日のスレッドと Notion ページへ載せ、段ごとに完了を記録する |
```

「新しいコードを足すときの判断基準」の「ユーザーへ見せる文言」の項の直後に、例外を 1 行足す。

```markdown
- ただし application のジョブと presentation の両方が使う文言 (位置ログのレポートの `format_location_report` など) は、両者が依存できる kgd-domain の純粋関数に置く
```

「定時処理」の図の `ScheduledJob` の参加者を `ScheduledJob<br>(AutoCloseJob / HourlySyncJob / DailyLocationReportJob)` にする。

- [ ] **Step 2: ADR を 3 本書く**

既存の ADR と同じく「ステータス / 文脈 / 決定 / 結果」の 4 節で書く。ステータスはいずれも `受理 (2026-09-29)`。内容は設計書の次の節から起こす。

- `0009-render-maps-with-tiny-skia.md`: 「地図描画に staticmap を採用せず tiny-skia を直接使う」。文脈は前回の設計書 (`docs/superpowers/specs/2026-09-21-owntracks-location-tracking-design.md`) の「地図描画」節 (attohttpc が MPL-2.0 で許可リスト外、User-Agent を差し替えられない、tiny-skia のバージョン重複、更新停止)。結果には「タイルの取得失敗は灰色で塗って描画を続ける」を含める
- `0010-do-not-impute-missing-motion.md`: 「移動種別の欠損を補完しない」。根拠は前回の設計書の「移動種別による色分け」節の実測値 (11202 点中 2436 点 (22%) が欠損、直前の値で埋めたときの妥当率 58%)。結果には「区間分割の関数だけを変えれば後から補完を導入できる」を含める
- `0011-publish-bot-posts-to-diary.md`: 「bot が作った内容は PublishDiaryPostUseCase を通して日報へ載せる」。文脈は bot の投稿がメッセージ同期と毎時の走査で無視されること (`is_bot`)、今後も定時投稿が増える見込み。決定は Notion、スレッドの順に載せ、`diary_posts` に段ごとの完了時刻を記録すること、クローズ済みのスレッドは再開して投稿後に戻し、戻せなくても投稿済みとして記録すること。結果には「日報が無い日はスキップとして記録する」「対象を 1 つ前の日報日に限るため、24 時間以上止まった日は投稿されない」を含める

`docs/adr/README.md` の表に 3 行を足す。

```markdown
| [0009](0009-render-maps-with-tiny-skia.md) | 地図描画に staticmap を採用せず tiny-skia を直接使う | 受理 |
| [0010](0010-do-not-impute-missing-motion.md) | 移動種別の欠損を補完しない | 受理 |
| [0011](0011-publish-bot-posts-to-diary.md) | bot が作った内容は PublishDiaryPostUseCase を通して日報へ載せる | 受理 |
```

- [ ] **Step 3: 全体のチェックを通す**

Run: `~/.nix-profile/bin/just ci`
Expected: PASS (fmt-check / check / clippy / deny / machete / test)

- [ ] **Step 4: コミットする**

```bash
git add docs
git commit -m "docs: 位置ログの日次レポートの構成と判断を記録する" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

## 完了の定義

- `~/.nix-profile/bin/just ci` が通る
- `[location]` を書いた設定で kgd を起動すると `/location report` が登録され、日付を省略すると今の日報日の今までの分が本人だけに表示される
- 日報日が切り替わった後の最初の tick で、前の日報日のレポートがその日の日報スレッドと Notion ページに 1 回だけ載る。再起動しても二重に載らない
- `[location]` を書かない既存の設定ファイルがそのまま動く

## デプロイ後に実機で確かめること

- クローズ済み (archived と locked) の日報スレッドへ、再開せずに bot が投稿できるか。できるなら、`PublishDiaryPostUseCase::post_to_thread` から再開とクローズを省く変更を別途行う
- 地図タイルのキャッシュが名前付きボリュームに書かれ、コンテナを作り直しても残るか

## この計画に含まれないもの

- 死活監視 (別の設計書で扱う)
- 全データの閲覧 (別の設計書で扱う)
- CLI からのレポート出力 (`BuildLocationReportUseCase` を配線すれば追加できる)
- 日報スレッドと Notion ページの自動作成
