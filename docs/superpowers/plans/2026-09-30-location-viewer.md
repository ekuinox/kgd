# 位置ログのブラウザビューア 実装計画

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** カレンダーで選んだ期間の位置ログを、LAN の中からブラウザで地図と集計グラフとして見られるようにする。

**Architecture:** 期間の点を 1 回読み、暦日 (`diary.timezone` の 0 時区切り) ごとの集計と、上限まで間引いた軌跡にまとめる `BrowseLocationHistoryUseCase` を足す。判断を伴う計算 (暦日の範囲、日ごとの分割、集計の足し合わせ、Visvalingam-Whyatt 法の間引き) は kgd-domain の純粋関数に置く。kgd-presentation の `viewer_router` が OwnTracks と同じ待ち受けの `/viewer/` 以下で API と埋め込んだ画面を配信し、送信元の許可リストと Cloudflare 経由の印で守る。画面は `web/` の React アプリで、API の型は Rust の DTO から schemars と自前の変換スクリプトで valibot のスキーマを生成する。

**Tech Stack:** Rust 2024 / axum 0.8.9 / schemars 1.2 (chrono04) / rust-embed 8.12 (mime-guess) / ipnet 2.12 / chrono 0.4 / mockall 0.13 ・ aube 2.6.1 / Node 22 (22.18 以上) / React 19.3 / Vite 8.3 / TypeScript 7.0 / valibot 1.5 / MapLibre GL JS 6.11 + @vis.gl/react-maplibre 8.1 / Recharts 3.10 / @daypicker/react 10.0 / Biome 2.5 / Vitest 5.0

**Spec:** `docs/superpowers/specs/2026-09-30-location-viewer-design.md`

## Global Constraints

- 依存方向は kgd-domain ← kgd-application ← kgd-infrastructure / kgd-presentation ← kgd (binary)。kgd-domain と kgd-application に IO ライブラリ (serenity / sqlx / reqwest / axum / schemars / rust-embed) を入れてはならない
- `cargo test -p kgd-application` が sqlx / serenity / libheif をビルドせずに通る性質を保つ
- schemars への依存は kgd-presentation に閉じる。API の DTO は kgd-presentation の `viewer/dto.rs` に置く
- 新規依存は workspace の `[workspace.dependencies]` に定義し、各 crate では `ipnet.workspace = true` の形で参照する。値が 1 つのときはインラインテーブルを使わない
- 追加してよい Rust のライセンスは MIT / Apache-2.0 / Apache-2.0 WITH LLVM-exception / BSD-2-Clause / BSD-3-Clause / ISC / Zlib / Unicode-3.0 / CDLA-Permissive-2.0 のみ (`deny.toml`)。`allow-git` は空のまま
- 判断・変換のロジックは kgd-domain の純粋関数とし、同一ファイル内の `#[cfg(test)] mod tests` でテストする。ユースケースはポートのモック (mockall) でテストする。crate の外からはモックを使えないため、presentation のテストは手書きのスタブを使う
- 非同期テストは `#[tokio::test]`。テスト関数名は `<対象>_<動詞>_<条件>` のスネークケース英文。doc コメントは日本語で「何を確認するか」を 1 文、必要なら空行を挟んで「なぜそうあるべきか」
- 構造体・列挙子・フィールドには doc コメントを付ける。`use` は std / 外部クレート / `crate` / `super` のブロックに分ける
- 1 日の区切りは `diary.timezone` の 0 時 (`DiaryCalendar::new(timezone, 0)`)。範囲は開始を含み終了を含まない半開区間。日報日の `day_start_hour` は使わない
- `[location.viewer]` の既定値: `allowed_cidrs = ["10.0.0.0/8", "172.16.0.0/12", "192.168.0.0/16", "fd00::/8"]` (loopback を含めない)、`max_track_points = 20000`
- API は `GET /viewer/api/history?from=YYYY-MM-DD&to=YYYY-MM-DD` (どちらも含む)。`from > to` と 10 年 (3660 日) を超える範囲は 400、DB のエラーは 500 と `{"error": "internal error"}`、ガードでの拒否は 403
- Cloudflare 経由の印は `Cf-Connecting-IP`、`Cf-Ray`、`Cdn-Loop` に `cloudflare` を含むもの。送信元はソケットの相手アドレスだけを使い、IPv4 射影アドレスは IPv4 に戻す
- 線の色は walking `#2e9e44`、cycling `#f08c1a`、automotive `#1f6fd1`、stationary `#c0392b`、unknown `#808080`。表示名は 徒歩 / 自転車 / 車 / 静止 / 不明
- 地図のスタイルは `https://tiles.openfreemap.org/styles/liberty`。Vite の `base` は `/viewer/`
- `web/src/api/schema.json` と `web/src/api/schema.gen.ts` は生成物であり、手で編集しない。作り直すのは `just gen-api`
- 画面のコードは Biome で整形する。`aube run lint` が整形の差分で落ちたら `aube run format` をかけてから見直す
- `web/scripts/` の `.ts` は Node が直接実行するため、`enum`、`namespace`、コンストラクタの引数プロパティを使わない。`.ts` の import には拡張子を付ける
- Rust のビルドとテストは nix を外した PATH で行う: `export PATH="$HOME/.cargo/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin"`。`just` は `~/.nix-profile/bin/just` をフルパスで呼ぶ。worktree で初めてビルドする前に `git submodule update --init crates/heif-sys/libheif` を実行する。`node` と `aube` は mise で入れ、必要なら `mise exec -- ` を前に付けて呼ぶ
- Docker は `/usr/bin/docker` をフルパスで呼ぶ
- Rust を変えたコミットの前に `~/.nix-profile/bin/just validate` (fmt / check / clippy -D warnings) を通す。画面を変えたコミットの前に `~/.nix-profile/bin/just web-check` を通す (Task 10 以降)
- コミットメッセージは日本語。末尾に `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>` を付ける。この環境には署名鍵が無いため `git -c commit.gpgsign=false commit` を使う
- コミットは各タスクの手順どおりに行う。push、PR の作成、本番の設定と cloudflared の変更は、利用者の許可を得てから行う

## Review Focus

- URL を手で書き換えて年の桁を誤る (`from=0026-09-01`): 数十万日ぶんの集計を作らずに 400 を返す (Task 7 のテスト `history_rejects_ranges_longer_than_ten_years`)
- 1 年ぶん (約 40 万点) の期間を選ぶ: 20000 点まで間引いて数秒以内に返る (Task 2 のテスト `simplify_segments_handles_a_year_of_points`)
- 範囲を素早く切り替える: 前のリクエストは取り消され、古い応答が新しい範囲の表示を上書きしない (Task 12 のテスト `rejects when the request is aborted` と、`useHistory` の `controller.signal.aborted` の確認)
- ブックマークなどから `/viewer` (末尾のスラッシュ無し) を開く: `/viewer/` へ転送され、画面の読み込みがずれない (Task 8 のテスト `viewer_without_trailing_slash_redirects`)
- 同じホストの cloudflared から届くリクエスト (loopback、Cf ヘッダ付き): `/pub` はこれまでどおり受け付け、`/viewer/` 以下は 403 (Task 7 のテスト `pub_still_accepts_loopback_requests_when_merged` と `history_rejects_loopback_peers`)

---

## ファイル構成

| ファイル | 責務 |
|---|---|
| `crates/kgd-domain/src/location/history.rs` (新規) | 暦日の範囲の変換、暦日ごとの分割、集計の足し合わせ |
| `crates/kgd-domain/src/location/simplify.rs` (新規) | Visvalingam-Whyatt 法による軌跡の間引き |
| `crates/kgd-domain/src/location/mod.rs` / `lib.rs` (変更) | 公開 |
| `crates/kgd-application/src/browse_location_history.rs` (新規) | `BrowseLocationHistoryUseCase`、`LocationHistory` |
| `crates/kgd/src/config/mod.rs` / `defaults.rs` / `tests.rs` (変更) | `[location.viewer]` の設定項目と検証 |
| `crates/kgd-presentation/src/viewer/mod.rs` (新規) | `viewer_router`、`ViewerSettings` |
| `crates/kgd-presentation/src/viewer/guard.rs` (新規) | 送信元と Cloudflare の印による判定、ミドルウェア |
| `crates/kgd-presentation/src/viewer/dto.rs` (新規) | API の入出力の型 (serde と schemars) |
| `crates/kgd-presentation/src/viewer/presenter.rs` (新規) | `LocationHistory` から DTO への変換 |
| `crates/kgd-presentation/src/viewer/api.rs` (新規) | `/viewer/api/history` のハンドラと API の 404 |
| `crates/kgd-presentation/src/viewer/assets.rs` (新規) | 埋め込んだ画面の配信 |
| `crates/kgd-presentation/src/viewer/tests.rs` (新規) | ルータのテスト |
| `crates/kgd-infrastructure/src/http_server.rs` (変更) | 接続元のアドレスを渡す起動 |
| `crates/kgd/src/bootstrap.rs` (変更) | 配線 |
| `web/` (新規) | React、Vite、TypeScript の画面、型の生成スクリプト |
| `mise.toml` / `Justfile` / `.gitignore` / `.dockerignore` / `Dockerfile` / `.github/workflows/build.yml` (変更) | ビルドと CI |
| `config.example.toml` / `cloudflared.example/config.yml` / `README.md` (変更) | 設定例、公開範囲、デプロイ手順 |
| `docs/architecture.md` / `docs/adr/0013-*.md` / `docs/adr/0014-*.md` / `docs/adr/README.md` (変更・新規) | ドキュメント |

---

### Task 1: 暦日の範囲と暦日ごとの集計

**Files:**
- Create: `crates/kgd-domain/src/location/history.rs`
- Modify: `crates/kgd-domain/src/location/mod.rs`
- Modify: `crates/kgd-domain/src/lib.rs`

**Interfaces:**
- Consumes: 既存の `DiaryCalendar::new(timezone, day_start_hour)`、`DiaryCalendar::day_range(date) -> (DateTime<Utc>, DateTime<Utc>)`、`DiaryCalendar::local_date(at) -> NaiveDate`、`LocationSummary`、`TrackPoint`
- Produces:
  - `pub fn calendar_day_range(timezone: Tz, from: NaiveDate, to: NaiveDate) -> (DateTime<Utc>, DateTime<Utc>)`
  - `pub fn group_by_calendar_day(points: Vec<TrackPoint>, timezone: Tz, from: NaiveDate, to: NaiveDate) -> Vec<(NaiveDate, Vec<TrackPoint>)>`
  - `pub fn sum_summaries<'a>(summaries: impl IntoIterator<Item = &'a LocationSummary>) -> LocationSummary`
  - いずれも `kgd_domain::` から公開する

0 時区切りの暦日は、`day_start_hour` を 0 にした `DiaryCalendar` と同じである。
サマータイムの抜けや重なりの扱いも `DiaryCalendar` に揃うため、自前で計算せずに使う。

- [ ] **Step 1: 失敗するテストを書く**

`crates/kgd-domain/src/location/history.rs` を作る。

```rust
//! 暦日 (タイムゾーンの 0 時区切り) 単位での位置ログの切り出しと集計の足し合わせ。

use chrono::{DateTime, NaiveDate, TimeDelta, Utc};
use chrono_tz::Tz;

use crate::diary::DiaryCalendar;

use super::{summary::LocationSummary, track::TrackPoint};

#[cfg(test)]
mod tests {
    use chrono::TimeZone as _;

    use crate::location::track::Activity;

    use super::*;

    fn date(month: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, month, day).unwrap()
    }

    /// Asia/Tokyo の時刻を UTC で作る。
    fn jst(month: u32, day: u32, hour: u32, min: u32) -> DateTime<Utc> {
        chrono_tz::Asia::Tokyo
            .with_ymd_and_hms(2026, month, day, hour, min, 0)
            .unwrap()
            .to_utc()
    }

    fn point_at(at: DateTime<Utc>) -> TrackPoint {
        TrackPoint {
            at,
            lat: 35.68,
            lon: 139.76,
            accuracy_m: Some(10),
            activity: Some(Activity::Walking),
        }
    }

    fn summary(point_count: usize) -> LocationSummary {
        LocationSummary {
            point_count,
            excluded_count: 0,
            distance_m: 0.0,
            distance_by_activity: Vec::new(),
            moving: TimeDelta::zero(),
            stationary: TimeDelta::zero(),
            first_at: None,
            last_at: None,
        }
    }

    /// 範囲が開始日の 0 時から終了日の翌日の 0 時までになることを確認する。
    ///
    /// 終了日もその日を含むため。Asia/Tokyo の 0 時は UTC の前日 15 時にあたる。
    #[test]
    fn calendar_day_range_spans_from_first_midnight_to_midnight_after_last_day() {
        let (start, end) = calendar_day_range(chrono_tz::Asia::Tokyo, date(9, 1), date(9, 2));

        assert_eq!(start, Utc.with_ymd_and_hms(2026, 8, 31, 15, 0, 0).unwrap());
        assert_eq!(end, Utc.with_ymd_and_hms(2026, 9, 2, 15, 0, 0).unwrap());
    }

    /// 0 時の直前と直後の点が別の日に入り、点の無い日も空の列として含まれることを確認する。
    #[test]
    fn group_by_calendar_day_splits_at_local_midnight_and_keeps_empty_days() {
        let before = point_at(jst(9, 1, 23, 59));
        let after = point_at(jst(9, 2, 0, 0));

        let days = group_by_calendar_day(
            vec![before.clone(), after.clone()],
            chrono_tz::Asia::Tokyo,
            date(9, 1),
            date(9, 3),
        );

        assert_eq!(
            days,
            vec![
                (date(9, 1), vec![before]),
                (date(9, 2), vec![after]),
                (date(9, 3), vec![]),
            ]
        );
    }

    /// 範囲の外の点は捨てることを確認する。
    ///
    /// リポジトリは範囲内の点だけを返す約束だが、日付の添字がはみ出してパニックしないようにする。
    #[test]
    fn group_by_calendar_day_drops_points_outside_the_range() {
        let days = group_by_calendar_day(
            vec![point_at(jst(8, 31, 12, 0)), point_at(jst(9, 2, 12, 0))],
            chrono_tz::Asia::Tokyo,
            date(9, 1),
            date(9, 1),
        );

        assert_eq!(days, vec![(date(9, 1), vec![])]);
    }

    /// 各項目の和をとり、最初と最後の記録時刻は最小と最大をとることを確認する。
    #[test]
    fn sum_summaries_adds_counts_and_durations_and_takes_min_max_times() {
        let first = LocationSummary {
            point_count: 3,
            excluded_count: 1,
            distance_m: 100.0,
            distance_by_activity: vec![(Some(Activity::Walking), 100.0)],
            moving: TimeDelta::minutes(10),
            stationary: TimeDelta::minutes(5),
            first_at: Some(jst(9, 1, 8, 0)),
            last_at: Some(jst(9, 1, 20, 0)),
        };
        let second = LocationSummary {
            point_count: 2,
            excluded_count: 2,
            distance_m: 250.0,
            distance_by_activity: vec![
                (Some(Activity::Walking), 50.0),
                (Some(Activity::Automotive), 200.0),
            ],
            moving: TimeDelta::minutes(20),
            stationary: TimeDelta::minutes(1),
            first_at: Some(jst(9, 2, 9, 0)),
            last_at: Some(jst(9, 2, 22, 0)),
        };

        let total = sum_summaries([&first, &summary(0), &second]);

        assert_eq!(total.point_count, 5);
        assert_eq!(total.excluded_count, 3);
        assert_eq!(total.distance_m, 350.0);
        assert_eq!(
            total.distance_by_activity,
            vec![
                (Some(Activity::Walking), 150.0),
                (Some(Activity::Automotive), 200.0),
            ]
        );
        assert_eq!(total.moving, TimeDelta::minutes(30));
        assert_eq!(total.stationary, TimeDelta::minutes(6));
        assert_eq!(total.first_at, Some(jst(9, 1, 8, 0)));
        assert_eq!(total.last_at, Some(jst(9, 2, 22, 0)));
    }

    /// 集計が 1 つも無いとき、すべて 0 で時刻も無い集計になることを確認する。
    #[test]
    fn sum_summaries_returns_zero_for_no_summaries() {
        assert_eq!(sum_summaries([]), summary(0));
    }
}
```

`crates/kgd-domain/src/location/mod.rs` に `mod history;` を足し、公開を足す。

```rust
mod history;
mod projection;
mod report_text;
mod summary;
mod track;

pub use history::{calendar_day_range, group_by_calendar_day, sum_summaries};
```

`crates/kgd-domain/src/lib.rs` の `pub use location::{...}` に `calendar_day_range`、`group_by_calendar_day`、`sum_summaries` を足す (rustfmt の並び順に従う)。

- [ ] **Step 2: テストが失敗することを確かめる**

Run: `cargo test -p kgd-domain location::history`
Expected: FAIL (`cannot find function calendar_day_range` などのコンパイルエラー)

- [ ] **Step 3: 実装する**

`history.rs` の `#[cfg(test)]` の前に書く。

```rust
/// 開始日から終了日まで (どちらも含む) の範囲を、UTC の半開区間で返す。
///
/// 開始は開始日の 0 時、終了は終了日の翌日の 0 時 (含まない) とする。
pub fn calendar_day_range(
    timezone: Tz,
    from: NaiveDate,
    to: NaiveDate,
) -> (DateTime<Utc>, DateTime<Utc>) {
    let calendar = midnight_calendar(timezone);
    let (start, _) = calendar.day_range(from);
    let (_, end) = calendar.day_range(to);
    (start, end)
}

/// 時刻順の点列を暦日ごとに分ける。
///
/// `from` から `to` までのすべての日を日付順に返し、点の無い日は空の列にする。
/// 範囲の外の点は捨てる。
pub fn group_by_calendar_day(
    points: Vec<TrackPoint>,
    timezone: Tz,
    from: NaiveDate,
    to: NaiveDate,
) -> Vec<(NaiveDate, Vec<TrackPoint>)> {
    let calendar = midnight_calendar(timezone);
    let mut days: Vec<(NaiveDate, Vec<TrackPoint>)> = from
        .iter_days()
        .take_while(|date| *date <= to)
        .map(|date| (date, Vec::new()))
        .collect();
    for point in points {
        let offset = calendar
            .local_date(point.at)
            .signed_duration_since(from)
            .num_days();
        if let Some((_, bucket)) = usize::try_from(offset)
            .ok()
            .and_then(|index| days.get_mut(index))
        {
            bucket.push(point);
        }
    }
    days
}

/// 日ごとの集計を足し合わせて、期間全体の集計を作る。
///
/// 点数、除外数、距離、移動種別ごとの距離、移動時間、静止時間は和をとり、
/// 最初と最後の記録時刻は最小と最大をとる。
pub fn sum_summaries<'a>(
    summaries: impl IntoIterator<Item = &'a LocationSummary>,
) -> LocationSummary {
    let mut total = LocationSummary {
        point_count: 0,
        excluded_count: 0,
        distance_m: 0.0,
        distance_by_activity: Vec::new(),
        moving: TimeDelta::zero(),
        stationary: TimeDelta::zero(),
        first_at: None,
        last_at: None,
    };
    for summary in summaries {
        total.point_count += summary.point_count;
        total.excluded_count += summary.excluded_count;
        total.distance_m += summary.distance_m;
        total.moving += summary.moving;
        total.stationary += summary.stationary;
        for (activity, distance) in &summary.distance_by_activity {
            match total
                .distance_by_activity
                .iter_mut()
                .find(|(known, _)| known == activity)
            {
                Some(entry) => entry.1 += distance,
                None => total.distance_by_activity.push((*activity, *distance)),
            }
        }
        total.first_at = earlier(total.first_at, summary.first_at);
        total.last_at = later(total.last_at, summary.last_at);
    }
    total
}

/// 0 時で日を区切る暦を返す。
fn midnight_calendar(timezone: Tz) -> DiaryCalendar {
    DiaryCalendar::new(timezone, 0)
}

/// 早いほうの時刻を返す。片方が無ければもう片方を返す。
fn earlier(a: Option<DateTime<Utc>>, b: Option<DateTime<Utc>>) -> Option<DateTime<Utc>> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    }
}

/// 遅いほうの時刻を返す。片方が無ければもう片方を返す。
fn later(a: Option<DateTime<Utc>>, b: Option<DateTime<Utc>>) -> Option<DateTime<Utc>> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (a, b) => a.or(b),
    }
}
```

`crate::diary::DiaryCalendar` が `diary` モジュールの外から見えない場合は、`use crate::DiaryCalendar;` に替える (`lib.rs` で再公開されている)。

- [ ] **Step 4: テストが通ることを確かめる**

Run: `cargo test -p kgd-domain location::history`
Expected: PASS (5 件)

- [ ] **Step 5: コミットする**

```bash
~/.nix-profile/bin/just validate
git add crates/kgd-domain/src/location/history.rs crates/kgd-domain/src/location/mod.rs crates/kgd-domain/src/lib.rs
git -c commit.gpgsign=false commit -m "feat: 位置ログを暦日ごとに分けて集計を足し合わせる関数を追加する

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 2: 点数の上限に合わせた軌跡の間引き

**Files:**
- Create: `crates/kgd-domain/src/location/simplify.rs`
- Modify: `crates/kgd-domain/src/location/mod.rs`
- Modify: `crates/kgd-domain/src/lib.rs`

**Interfaces:**
- Consumes: 既存の `TrackPoint`、`TrackSegment`
- Produces:
  - `pub fn simplify_segments(segments: Vec<TrackSegment>, max_points: usize) -> Vec<TrackSegment>`
  - `pub fn count_points(segments: &[TrackSegment]) -> usize`
  - いずれも `kgd_domain::` から公開する

点数は区間の点の数の和で数える。
`split_segments` は区間の境目の点を両方の区間に入れるため、境目の点は 2 回数える。
上限もこの数え方に対して守る。

- [ ] **Step 1: 失敗するテストを書く**

`crates/kgd-domain/src/location/simplify.rs` を作る。

```rust
//! 点数の上限に合わせた軌跡の間引き (Visvalingam-Whyatt 法)。

use std::{cmp::Ordering, collections::BinaryHeap};

use super::track::{TrackPoint, TrackSegment};

#[cfg(test)]
mod tests {
    use crate::location::track::{Activity, tests::point};

    use super::*;

    /// 緯度方向にジグザグする点列の区間を作る。
    fn zigzag(activity: Option<Activity>, start_minute: i64, count: usize) -> TrackSegment {
        TrackSegment {
            activity,
            points: (0..count)
                .map(|i| {
                    let lat = 35.0 + if i % 2 == 0 { 0.0 } else { 0.001 * (i as f64) };
                    point(start_minute + i as i64, lat, 139.0 + 0.001 * i as f64, activity)
                })
                .collect(),
        }
    }

    /// 点数が上限以下なら何も変えないことを確認する。
    #[test]
    fn simplify_segments_keeps_segments_within_the_limit() {
        let segments = vec![zigzag(Some(Activity::Walking), 0, 5)];

        assert_eq!(simplify_segments(segments.clone(), 5), segments);
    }

    /// 点数の合計が上限ちょうどまで減ることを確認する。
    #[test]
    fn simplify_segments_reduces_points_to_the_limit() {
        let segments = vec![
            zigzag(Some(Activity::Walking), 0, 60),
            zigzag(Some(Activity::Automotive), 100, 40),
        ];

        let simplified = simplify_segments(segments, 20);

        assert_eq!(count_points(&simplified), 20);
    }

    /// 区間の始点と終点が必ず残り、区間の数と移動種別が変わらないことを確認する。
    ///
    /// 移動種別の色の切れ目の位置を変えないため。
    #[test]
    fn simplify_segments_keeps_segment_endpoints_and_activities() {
        let segments = vec![
            zigzag(Some(Activity::Walking), 0, 30),
            zigzag(None, 100, 30),
        ];

        let simplified = simplify_segments(segments.clone(), 8);

        assert_eq!(simplified.len(), 2);
        for (before, after) in segments.iter().zip(&simplified) {
            assert_eq!(after.activity, before.activity);
            assert_eq!(after.points.first(), before.points.first());
            assert_eq!(after.points.last(), before.points.last());
        }
    }

    /// 形への影響が最も小さい点 (直線上の点) から取り除くことを確認する。
    #[test]
    fn simplify_segments_removes_the_least_significant_point_first() {
        let straight = point(1, 35.0, 139.001, None);
        let spike = point(3, 35.01, 139.003, None);
        let segment = TrackSegment {
            activity: None,
            points: vec![
                point(0, 35.0, 139.0, None),
                straight.clone(),
                point(2, 35.0, 139.002, None),
                spike.clone(),
                point(4, 35.0, 139.004, None),
            ],
        };

        let simplified = simplify_segments(vec![segment], 4);

        assert!(!simplified[0].points.contains(&straight));
        assert!(simplified[0].points.contains(&spike));
    }

    /// 始点と終点だけで上限を超えるときは、それ以上は間引かずに返すことを確認する。
    #[test]
    fn simplify_segments_stops_when_only_endpoints_remain() {
        let segments: Vec<TrackSegment> = (0..5)
            .map(|i| zigzag(Some(Activity::Walking), i * 10, 3))
            .collect();

        let simplified = simplify_segments(segments, 4);

        assert_eq!(count_points(&simplified), 10);
        assert!(simplified.iter().all(|segment| segment.points.len() == 2));
    }

    /// 1 年ぶん (約 40 万点) の軌跡も上限まで間引けることを確認する。
    ///
    /// 1 日に 1000 点ほど記録されるため、長い期間を選ぶとこの規模になる。
    /// 区間ごとに間引くと面積を何度も計算し直すことになるが、優先度付きキューで
    /// 全体を一度に扱うため、デバッグビルドのテストでも数秒以内に終わる。
    #[test]
    fn simplify_segments_handles_a_year_of_points() {
        let segments: Vec<TrackSegment> = (0..400)
            .map(|i| {
                let activity = if i % 2 == 0 { Some(Activity::Walking) } else { None };
                zigzag(activity, i * 2000, 1000)
            })
            .collect();

        let simplified = simplify_segments(segments, 20000);

        assert_eq!(count_points(&simplified), 20000);
    }
}
```

`location/mod.rs` に `mod simplify;` と `pub use simplify::{count_points, simplify_segments};` を足し、`lib.rs` の `pub use location::{...}` にも `count_points` と `simplify_segments` を足す。

- [ ] **Step 2: テストが失敗することを確かめる**

Run: `cargo test -p kgd-domain location::simplify`
Expected: FAIL (`cannot find function simplify_segments` などのコンパイルエラー)

- [ ] **Step 3: 実装する**

`simplify.rs` の `#[cfg(test)]` の前に書く。

```rust
/// 区間の点の数の和を返す。区間の境目で共有する点は両方の区間で数える。
pub fn count_points(segments: &[TrackSegment]) -> usize {
    segments.iter().map(|segment| segment.points.len()).sum()
}

/// 点数の合計が `max_points` 以下になるまで、Visvalingam-Whyatt 法で点を間引く。
///
/// すべての区間の内側の点を 1 つの優先度付きキューで扱い、前後の点と作る三角形の
/// 面積が最も小さい点から取り除く。区間の始点と終点は取り除かないため、
/// 始点と終点だけで上限を超えるときはそこで止める。点数が上限以下なら何もしない。
pub fn simplify_segments(segments: Vec<TrackSegment>, max_points: usize) -> Vec<TrackSegment> {
    let mut remaining = count_points(&segments);
    if remaining <= max_points {
        return segments;
    }

    let mut links: Vec<Vec<Link>> = segments
        .iter()
        .map(|segment| {
            let len = segment.points.len();
            (0..len)
                .map(|index| Link {
                    prev: index.checked_sub(1),
                    next: (index + 1 < len).then_some(index + 1),
                    removed: false,
                    version: 0,
                })
                .collect()
        })
        .collect();

    let mut heap = BinaryHeap::new();
    for (segment_index, segment) in segments.iter().enumerate() {
        let points = &segment.points;
        for index in 1..points.len().saturating_sub(1) {
            heap.push(Candidate {
                area: triangle_area(&points[index - 1], &points[index], &points[index + 1]),
                segment: segment_index,
                index,
                version: 0,
            });
        }
    }

    while remaining > max_points {
        let Some(candidate) = heap.pop() else {
            break;
        };
        let segment_links = &mut links[candidate.segment];
        let link = segment_links[candidate.index];
        if link.removed || link.version != candidate.version {
            continue;
        }
        let (Some(prev), Some(next)) = (link.prev, link.next) else {
            continue;
        };
        segment_links[candidate.index].removed = true;
        segment_links[prev].next = Some(next);
        segment_links[next].prev = Some(prev);
        remaining -= 1;

        let points = &segments[candidate.segment].points;
        for neighbor in [prev, next] {
            let neighbor_link = segment_links[neighbor];
            let (Some(before), Some(after)) = (neighbor_link.prev, neighbor_link.next) else {
                continue;
            };
            let version = neighbor_link.version + 1;
            segment_links[neighbor].version = version;
            // 取り除いた点より小さい面積にしない (Visvalingam-Whyatt 法の慣例)。
            // 隣の点が取り除いた点より先に消えて、形の崩れる順序が逆転するのを防ぐ。
            let area = triangle_area(&points[before], &points[neighbor], &points[after])
                .max(candidate.area);
            heap.push(Candidate {
                area,
                segment: candidate.segment,
                index: neighbor,
                version,
            });
        }
    }

    segments
        .into_iter()
        .zip(links)
        .map(|(segment, links)| TrackSegment {
            activity: segment.activity,
            points: segment
                .points
                .into_iter()
                .zip(links)
                .filter(|(_, link)| !link.removed)
                .map(|(point, _)| point)
                .collect(),
        })
        .collect()
}

/// 区間内での点の前後のつながり。
#[derive(Debug, Clone, Copy)]
struct Link {
    /// 残っている直前の点の添字
    prev: Option<usize>,
    /// 残っている直後の点の添字
    next: Option<usize>,
    /// 取り除いたかどうか
    removed: bool,
    /// 面積を計算し直した回数。古い候補を見分けるのに使う
    version: u64,
}

/// 取り除く候補の点。面積が小さいほど先に取り出す。
#[derive(Debug)]
struct Candidate {
    /// 前後の点と作る三角形の面積
    area: f64,
    /// 区間の添字
    segment: usize,
    /// 区間内での点の添字
    index: usize,
    /// 候補を作ったときの `Link::version`
    version: u64,
}

impl Ord for Candidate {
    fn cmp(&self, other: &Self) -> Ordering {
        // BinaryHeap は最大のものを先に返すため、面積の比較を逆にする。
        // 面積が同じなら前の区間、前の点を先に返し、結果を決定的にする。
        other
            .area
            .total_cmp(&self.area)
            .then_with(|| other.segment.cmp(&self.segment))
            .then_with(|| other.index.cmp(&self.index))
    }
}

impl PartialOrd for Candidate {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for Candidate {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for Candidate {}

/// 3 点が作る三角形の面積を返す。
///
/// 経度に中央の点の緯度の余弦を掛けて、狭い範囲を平面とみなした座標で計算する。
/// 大小の比較にだけ使うため、単位は度の 2 乗のままにする。
fn triangle_area(a: &TrackPoint, b: &TrackPoint, c: &TrackPoint) -> f64 {
    let scale = b.lat.to_radians().cos();
    let (ax, ay) = (a.lon * scale, a.lat);
    let (bx, by) = (b.lon * scale, b.lat);
    let (cx, cy) = (c.lon * scale, c.lat);
    ((bx - ax) * (cy - ay) - (cx - ax) * (by - ay)).abs() / 2.0
}
```

- [ ] **Step 4: テストが通ることを確かめる**

Run: `cargo test -p kgd-domain location::simplify`
Expected: PASS (6 件)

- [ ] **Step 5: コミットする**

```bash
~/.nix-profile/bin/just validate
git add crates/kgd-domain/src/location/simplify.rs crates/kgd-domain/src/location/mod.rs crates/kgd-domain/src/lib.rs
git -c commit.gpgsign=false commit -m "feat: 点数の上限に合わせて軌跡を間引く関数を追加する

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 3: 期間の位置ログを集めるユースケース

**Files:**
- Create: `crates/kgd-application/src/browse_location_history.rs`
- Modify: `crates/kgd-application/src/lib.rs`
- Modify: `crates/kgd-application/Cargo.toml` (dev-dependencies に `tokio` の `macros` と `rt` が無ければ足す)

**Interfaces:**
- Consumes: `calendar_day_range`、`group_by_calendar_day`、`sum_summaries` (Task 1)、`simplify_segments`、`count_points` (Task 2)、既存の `filter_accurate`、`summarize`、`split_segments`、`LocationRepository::locations_between`
- Produces (`kgd_application::` から公開する):

```rust
pub struct LocationHistorySettings {
    pub timezone: Tz,
    pub max_accuracy_m: i32,
    pub max_track_points: usize,
}

pub struct DailyLocationSummary {
    pub date: NaiveDate,
    pub summary: LocationSummary,
}

pub struct LocationHistory {
    pub from: NaiveDate,
    pub to: NaiveDate,
    pub timezone: Tz,
    pub total: LocationSummary,
    pub days: Vec<DailyLocationSummary>,
    pub segments: Vec<TrackSegment>,
    pub original_points: usize,
    pub returned_points: usize,
}

impl BrowseLocationHistoryUseCase {
    pub fn new(repo: Arc<dyn LocationRepository>, settings: LocationHistorySettings) -> Self;
    pub async fn browse(&self, from: NaiveDate, to: NaiveDate) -> Result<LocationHistory>;
}
```

`browse` は `from <= to` を前提とする。
検証は presentation の責務とし、ユースケースでは行わない。

- [ ] **Step 1: 失敗するテストを書く**

既存の `build_location_report.rs` のテストが `#[tokio::test]` を使っているため、dev-dependencies は足さずに済むはずである。
`cargo test -p kgd-application` で `tokio::test` が見つからなければ、`[dev-dependencies]` に `tokio = { workspace = true, features = ["macros", "rt"] }` を足す。

`crates/kgd-application/src/browse_location_history.rs` を作る。

```rust
//! 暦日で選んだ期間の位置ログを、集計と地図用の軌跡にまとめるユースケース。

use std::sync::Arc;

use anyhow::Result;
use chrono::NaiveDate;
use chrono_tz::Tz;

use kgd_domain::{
    LocationSummary, TrackSegment, calendar_day_range, count_points, filter_accurate,
    group_by_calendar_day, simplify_segments, split_segments, sum_summaries, summarize,
};

use super::ports::LocationRepository;

#[cfg(test)]
mod tests {
    use chrono::{DateTime, TimeZone as _, Utc};
    use mockall::predicate::eq;

    use kgd_domain::{Activity, TrackPoint};

    use crate::ports::MockLocationRepository;

    use super::*;

    fn date(day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 9, day).unwrap()
    }

    /// Asia/Tokyo の時刻を UTC で作る。
    fn jst(day: u32, hour: u32, min: u32) -> DateTime<Utc> {
        chrono_tz::Asia::Tokyo
            .with_ymd_and_hms(2026, 9, day, hour, min, 0)
            .unwrap()
            .to_utc()
    }

    /// 徒歩の点を作る。緯度は分ごとに少しずつずらす。
    fn walk(at: DateTime<Utc>, accuracy_m: i32) -> TrackPoint {
        TrackPoint {
            at,
            lat: 35.0 + f64::from(at.timestamp() as i32 % 3600) * 1e-6,
            lon: 139.0,
            accuracy_m: Some(accuracy_m),
            activity: Some(Activity::Walking),
        }
    }

    fn use_case(repo: MockLocationRepository, max_track_points: usize) -> BrowseLocationHistoryUseCase {
        BrowseLocationHistoryUseCase::new(
            Arc::new(repo),
            LocationHistorySettings {
                timezone: chrono_tz::Asia::Tokyo,
                max_accuracy_m: 200,
                max_track_points,
            },
        )
    }

    /// 開始日の 0 時から終了日の翌日の 0 時までの範囲で、リポジトリを 1 回だけ読むことを確認する。
    #[tokio::test]
    async fn browse_queries_the_calendar_day_range_once() {
        let mut repo = MockLocationRepository::new();
        repo.expect_locations_between()
            .with(eq(jst(1, 0, 0)), eq(jst(3, 0, 0)))
            .times(1)
            .returning(|_, _| Ok(Vec::new()));

        let history = use_case(repo, 100).browse(date(1), date(2)).await.unwrap();

        assert_eq!(history.from, date(1));
        assert_eq!(history.to, date(2));
        assert_eq!(history.timezone, chrono_tz::Asia::Tokyo);
    }

    /// 精度の悪い点は集計と軌跡から外れ、その日の除外数に数えられることを確認する。
    #[tokio::test]
    async fn browse_excludes_inaccurate_points_per_day() {
        let mut repo = MockLocationRepository::new();
        repo.expect_locations_between().returning(|_, _| {
            Ok(vec![
                walk(jst(1, 10, 0), 10),
                walk(jst(1, 10, 1), 500),
                walk(jst(1, 10, 2), 10),
            ])
        });

        let history = use_case(repo, 100).browse(date(1), date(1)).await.unwrap();

        assert_eq!(history.days.len(), 1);
        assert_eq!(history.days[0].summary.point_count, 2);
        assert_eq!(history.days[0].summary.excluded_count, 1);
        assert_eq!(history.original_points, 2);
    }

    /// 点の無い日も含めて日付順に並び、合計が日ごとの集計の和になることを確認する。
    ///
    /// グラフの棒の和と合計を一致させるため。
    #[tokio::test]
    async fn browse_lists_every_day_and_totals_the_daily_summaries() {
        let mut repo = MockLocationRepository::new();
        repo.expect_locations_between().returning(|_, _| {
            Ok(vec![
                walk(jst(1, 10, 0), 10),
                walk(jst(1, 10, 1), 10),
                walk(jst(3, 9, 0), 10),
                walk(jst(3, 9, 1), 10),
            ])
        });

        let history = use_case(repo, 100).browse(date(1), date(3)).await.unwrap();

        let dates: Vec<NaiveDate> = history.days.iter().map(|day| day.date).collect();
        assert_eq!(dates, vec![date(1), date(2), date(3)]);
        assert_eq!(history.days[1].summary.point_count, 0);
        assert_eq!(history.total.point_count, 4);
        let daily_distance: f64 = history.days.iter().map(|day| day.summary.distance_m).sum();
        assert_eq!(history.total.distance_m, daily_distance);
    }

    /// 点数が上限以下なら間引かず、上限を超えたら上限まで間引くことを確認する。
    #[tokio::test]
    async fn browse_simplifies_only_when_points_exceed_the_limit() {
        let points: Vec<TrackPoint> = (0..50).map(|m| walk(jst(1, 10, m), 10)).collect();
        let mut small = MockLocationRepository::new();
        let returned = points.clone();
        small
            .expect_locations_between()
            .returning(move |_, _| Ok(returned.clone()));
        let mut large = MockLocationRepository::new();
        large
            .expect_locations_between()
            .returning(move |_, _| Ok(points.clone()));

        let untouched = use_case(small, 100).browse(date(1), date(1)).await.unwrap();
        let simplified = use_case(large, 10).browse(date(1), date(1)).await.unwrap();

        assert_eq!(untouched.original_points, 50);
        assert_eq!(untouched.returned_points, 50);
        assert_eq!(simplified.original_points, 50);
        assert_eq!(simplified.returned_points, 10);
        assert_eq!(count_points(&simplified.segments), 10);
    }
}
```

`lib.rs` に `mod browse_location_history;` と次の公開を足す。

```rust
pub use browse_location_history::{
    BrowseLocationHistoryUseCase, DailyLocationSummary, LocationHistory, LocationHistorySettings,
};
```

- [ ] **Step 2: テストが失敗することを確かめる**

Run: `cargo test -p kgd-application browse_location_history`
Expected: FAIL (`cannot find struct BrowseLocationHistoryUseCase` などのコンパイルエラー)

- [ ] **Step 3: 実装する**

`browse_location_history.rs` の `#[cfg(test)]` の前に書く。

```rust
/// 期間の位置ログのまとめ方の設定。
#[derive(Debug, Clone, Copy)]
pub struct LocationHistorySettings {
    /// 暦日を区切るタイムゾーン
    pub timezone: Tz,
    /// これを超える水平精度 (メートル) の点を除く
    pub max_accuracy_m: i32,
    /// 地図に返す軌跡の点数の上限
    pub max_track_points: usize,
}

/// 1 日ぶんの集計。
#[derive(Debug, Clone, PartialEq)]
pub struct DailyLocationSummary {
    /// 暦日
    pub date: NaiveDate,
    /// その日の集計
    pub summary: LocationSummary,
}

/// 期間の位置ログのまとめ。
#[derive(Debug, Clone, PartialEq)]
pub struct LocationHistory {
    /// 開始日 (含む)
    pub from: NaiveDate,
    /// 終了日 (含む)
    pub to: NaiveDate,
    /// 暦日を区切ったタイムゾーン
    pub timezone: Tz,
    /// 期間全体の集計 (日ごとの集計の和)
    pub total: LocationSummary,
    /// 日ごとの集計。期間のすべての日を日付順に並べる
    pub days: Vec<DailyLocationSummary>,
    /// 地図に描く軌跡。点数が上限を超えていれば間引いてある
    pub segments: Vec<TrackSegment>,
    /// 間引く前の軌跡の点数 (区間の境目の点は両方の区間で数える)
    pub original_points: usize,
    /// 間引いた後の軌跡の点数 (数え方は `original_points` と同じ)
    pub returned_points: usize,
}

/// 暦日で選んだ期間の位置ログを、集計と地図用の軌跡にまとめるユースケース。
pub struct BrowseLocationHistoryUseCase {
    /// 位置情報リポジトリポート
    repo: Arc<dyn LocationRepository>,
    /// まとめ方の設定
    settings: LocationHistorySettings,
}

impl BrowseLocationHistoryUseCase {
    /// 新しい BrowseLocationHistoryUseCase を作成する。
    pub fn new(repo: Arc<dyn LocationRepository>, settings: LocationHistorySettings) -> Self {
        Self { repo, settings }
    }

    /// `from` から `to` まで (どちらも含む) の位置ログをまとめる。`from <= to` を前提とする。
    ///
    /// 点は 1 回だけ読み、精度の悪い点を日ごとに除いてから集計する。
    /// 期間全体の集計は日ごとの集計の和とし、0 時をまたぐ間隔はどちらの日にも数えない。
    pub async fn browse(&self, from: NaiveDate, to: NaiveDate) -> Result<LocationHistory> {
        let LocationHistorySettings {
            timezone,
            max_accuracy_m,
            max_track_points,
        } = self.settings;
        let (start, end) = calendar_day_range(timezone, from, to);
        let raw = self.repo.locations_between(start, end).await?;

        let mut days = Vec::new();
        let mut kept_points = Vec::new();
        for (date, points) in group_by_calendar_day(raw, timezone, from, to) {
            let (kept, excluded) = filter_accurate(points, max_accuracy_m);
            days.push(DailyLocationSummary {
                date,
                summary: summarize(&kept, excluded),
            });
            kept_points.extend(kept);
        }
        let total = sum_summaries(days.iter().map(|day| &day.summary));

        let segments = split_segments(&kept_points);
        let original_points = count_points(&segments);
        let segments = simplify_segments(segments, max_track_points);
        let returned_points = count_points(&segments);

        Ok(LocationHistory {
            from,
            to,
            timezone,
            total,
            days,
            segments,
            original_points,
            returned_points,
        })
    }
}
```

- [ ] **Step 4: テストが通ることを確かめる**

Run: `cargo test -p kgd-application browse_location_history`
Expected: PASS (4 件)

- [ ] **Step 5: コミットする**

```bash
~/.nix-profile/bin/just validate
git add crates/kgd-application/src/browse_location_history.rs crates/kgd-application/src/lib.rs crates/kgd-application/Cargo.toml
git -c commit.gpgsign=false commit -m "feat: 暦日で選んだ期間の位置ログをまとめるユースケースを追加する

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---
### Task 4: `[location.viewer]` の設定

**Files:**
- Modify: `Cargo.toml` (`[workspace.dependencies]` に `ipnet`)
- Modify: `crates/kgd/Cargo.toml`
- Modify: `crates/kgd/src/config/mod.rs`
- Modify: `crates/kgd/src/config/defaults.rs`
- Modify: `crates/kgd/src/config/tests.rs`
- Modify: `config.example.toml`

**Interfaces:**
- Produces:
  - `LocationConfig` に `pub viewer: Option<ViewerConfig>` (`#[serde(default)]`) を足す
  - `pub struct ViewerConfig { pub allowed_cidrs: Vec<IpNet>, pub max_track_points: usize }`
  - 既定値: `allowed_cidrs` は `10.0.0.0/8`、`172.16.0.0/12`、`192.168.0.0/16`、`fd00::/8`。`max_track_points` は 20000
  - 検証: `max_track_points` は 1 以上、`allowed_cidrs` は空でない

- [ ] **Step 1: 依存を足す**

`Cargo.toml` の `[workspace.dependencies]` の末尾に足す。

```toml
# CIDR (location viewer allowlist)
ipnet = { version = "2.12", features = ["serde"] }
```

`crates/kgd/Cargo.toml` の `[dependencies]` に `ipnet.workspace = true` を足す (アルファベット順)。

- [ ] **Step 2: 失敗するテストを書く**

`crates/kgd/src/config/tests.rs` の末尾に足す。

```rust
/// [location] と、その後ろに続けるテキストから設定 TOML を作る。
fn location_config_toml(extra: &str) -> String {
    format!(
        "{}\n[location]\nusername = \"ekuinox\"\npassword = \"secret\"\n{extra}",
        minimal_config_toml("")
    )
}

/// [location.viewer] を書かなければビューアは無効 (None) になることを確認する。
///
/// kgd を更新しただけでビューアが動き出さないようにするため。
#[test]
fn location_viewer_is_disabled_by_default() {
    let config: Config = toml::from_str(&location_config_toml("")).expect("should parse");

    assert_eq!(config.location.expect("should be present").viewer, None);
}

/// [location.viewer] を書くと、許可リストが LAN のプライベート帯、点数の上限が 20000 になることを確認する。
///
/// 同じホストの cloudflared から届くリクエストを拒否するため、既定の許可リストに loopback を含めない。
#[test]
fn location_viewer_has_private_ranges_and_point_limit_by_default() {
    let config: Config =
        toml::from_str(&location_config_toml("[location.viewer]\n")).expect("should parse");

    let viewer = config
        .location
        .expect("should be present")
        .viewer
        .expect("viewer should be enabled");
    let expected: Vec<IpNet> = ["10.0.0.0/8", "172.16.0.0/12", "192.168.0.0/16", "fd00::/8"]
        .iter()
        .map(|net| net.parse().unwrap())
        .collect();
    assert_eq!(viewer.allowed_cidrs, expected);
    assert_eq!(viewer.max_track_points, 20000);
    assert!(
        !viewer
            .allowed_cidrs
            .iter()
            .any(|net| net.contains(&"127.0.0.1".parse::<std::net::IpAddr>().unwrap()))
    );
}

/// 許可リストと点数の上限を書き換えられることを確認する。
#[test]
fn location_viewer_parses_custom_values() {
    let config: Config = toml::from_str(&location_config_toml(
        "[location.viewer]\nallowed_cidrs = [\"192.168.1.0/24\", \"127.0.0.1/32\"]\nmax_track_points = 500\n",
    ))
    .expect("should parse");

    let viewer = config.location.unwrap().viewer.unwrap();
    assert_eq!(
        viewer.allowed_cidrs,
        vec![
            "192.168.1.0/24".parse::<IpNet>().unwrap(),
            "127.0.0.1/32".parse::<IpNet>().unwrap(),
        ]
    );
    assert_eq!(viewer.max_track_points, 500);
}

/// 点数の上限に 0 を書くと検証で弾かれることを確認する。
#[test]
fn validate_rejects_zero_max_track_points() {
    let config: Config = toml::from_str(&location_config_toml(
        "[location.viewer]\nmax_track_points = 0\n",
    ))
    .expect("should parse");

    let error = config.validate().expect_err("should be rejected");
    assert!(error.to_string().contains("max_track_points"));
}

/// 許可リストを空にすると検証で弾かれることを確認する。
///
/// 空だとどこからも見られず、設定の誤りに気づきにくいため。
#[test]
fn validate_rejects_empty_allowed_cidrs() {
    let config: Config = toml::from_str(&location_config_toml(
        "[location.viewer]\nallowed_cidrs = []\n",
    ))
    .expect("should parse");

    let error = config.validate().expect_err("should be rejected");
    assert!(error.to_string().contains("allowed_cidrs"));
}
```

`tests.rs` の先頭の `use` に `use ipnet::IpNet;` を足す。

- [ ] **Step 3: テストが失敗することを確かめる**

Run: `cargo test -p kgd config::tests`
Expected: FAIL (`no field viewer on type LocationConfig` などのコンパイルエラー)

- [ ] **Step 4: 実装する**

`crates/kgd/src/config/mod.rs` の `use` に `use ipnet::IpNet;` を足し、`LocationConfig` の末尾 (`tile_cache_dir` の後) にフィールドを足す。

```rust
    /// ブラウザで位置ログを見るビューアの設定 (省略時はビューアを無効にする)
    #[serde(default)]
    pub viewer: Option<ViewerConfig>,
```

`LocationConfig` の後ろに `ViewerConfig` を足す。

```rust
/// 位置ログのビューアの設定。
///
/// OwnTracks の受け口と同じ待ち受けで `/viewer/` 以下に置く。
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct ViewerConfig {
    /// ビューアに届いてよい送信元（デフォルト: LAN のプライベート帯。loopback は含まない）
    ///
    /// 同じホストの cloudflared は 127.0.0.1 から接続してくるため、loopback を含めると
    /// トンネル経由のリクエストを送信元では拒否できなくなる。
    #[serde(default = "default_viewer_allowed_cidrs")]
    pub allowed_cidrs: Vec<IpNet>,
    /// 地図に返す軌跡の点数の上限。超えたら形を保って間引く（デフォルト: 20000）
    #[serde(default = "default_viewer_max_track_points")]
    pub max_track_points: usize,
}
```

`validate` の `if let Some(location) = &self.location { ... }` の中、画像サイズの検証の後に足す。

```rust
            if let Some(viewer) = &location.viewer {
                ensure!(
                    viewer.max_track_points > 0,
                    "location.viewer.max_track_points must be at least 1"
                );
                ensure!(
                    !viewer.allowed_cidrs.is_empty(),
                    "location.viewer.allowed_cidrs must not be empty"
                );
            }
```

`crates/kgd/src/config/defaults.rs` の末尾に足す (`use ipnet::IpNet;` も足す)。

```rust
pub(super) fn default_viewer_allowed_cidrs() -> Vec<IpNet> {
    ["10.0.0.0/8", "172.16.0.0/12", "192.168.0.0/16", "fd00::/8"]
        .iter()
        .map(|net| net.parse().expect("default CIDR must be valid"))
        .collect()
}

pub(super) fn default_viewer_max_track_points() -> usize {
    20000
}
```

`config.example.toml` の `# tile_cache_dir = ...` の後に足す。

```toml
# Browser viewer for location history, served at http://<host>:<listen port>/viewer/
# Omit this section to disable the viewer.
# The viewer has no login. It is protected by the source address allowlist below and
# by rejecting requests that came through Cloudflare (Cf-Connecting-IP / Cf-Ray / Cdn-Loop).
# Also restrict the cloudflared ingress for the OwnTracks hostname to ^/(pub|healthz)$.
# [location.viewer]
# Source networks allowed to reach the viewer (default: private LAN ranges, loopback excluded)
# A local cloudflared connects from 127.0.0.1, so do not add loopback in production.
# For local development through the Vite dev server proxy, add "127.0.0.1/32".
# allowed_cidrs = ["10.0.0.0/8", "172.16.0.0/12", "192.168.0.0/16", "fd00::/8"]
# Maximum number of track points returned to the map. Longer tracks are simplified (default: 20000)
# max_track_points = 20000
```

- [ ] **Step 5: テストが通ることを確かめる**

Run: `cargo test -p kgd config::tests`
Expected: PASS (既存の `parse_example_config` も含めてすべて)

- [ ] **Step 6: コミットする**

```bash
~/.nix-profile/bin/just validate
git add Cargo.toml Cargo.lock crates/kgd/Cargo.toml crates/kgd/src/config config.example.toml
git -c commit.gpgsign=false commit -m "feat: 位置ログのビューアの設定 [location.viewer] を追加する

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 5: ビューアのアクセスのガード

**Files:**
- Create: `crates/kgd-presentation/src/viewer/mod.rs`
- Create: `crates/kgd-presentation/src/viewer/guard.rs`
- Modify: `crates/kgd-presentation/src/lib.rs`
- Modify: `crates/kgd-presentation/Cargo.toml`

**Interfaces:**
- Produces (`viewer/guard.rs`、crate 内):
  - `pub(super) enum Denial { ViaCloudflare, OutsideAllowlist }`
  - `pub(super) fn has_cloudflare_marks(headers: &HeaderMap) -> bool`
  - `pub(super) fn decide_access(peer: IpAddr, via_cloudflare: bool, allowed: &[IpNet]) -> Result<(), Denial>`
  - `pub(super) async fn guard(State(allowed): State<Arc<[IpNet]>>, ConnectInfo(peer): ConnectInfo<SocketAddr>, request: Request, next: Next) -> Response`
- Produces (`viewer/mod.rs`、`kgd_presentation::` から公開):
  - `pub struct ViewerSettings { pub allowed_cidrs: Vec<IpNet> }`
  - `viewer_router` は Task 7 で足す。このタスクでは `mod guard;` と `ViewerSettings` だけを置く

- [ ] **Step 1: 依存を足す**

`crates/kgd-presentation/Cargo.toml` の `[dependencies]` に `ipnet.workspace = true` を足す。

- [ ] **Step 2: 失敗するテストを書く**

`crates/kgd-presentation/src/viewer/mod.rs` を作る。

```rust
//! ブラウザで位置ログを見るビューアのコントローラ。
//!
//! OwnTracks の受け口と同じ待ち受けで `/viewer/` 以下に置く。ログインは無く、
//! 送信元の許可リストと Cloudflare 経由の印で守る (ADR-0013)。

use ipnet::IpNet;

mod guard;

/// ビューアの設定。
#[derive(Debug, Clone)]
pub struct ViewerSettings {
    /// ビューアに届いてよい送信元
    pub allowed_cidrs: Vec<IpNet>,
}
```

`crates/kgd-presentation/src/lib.rs` に `mod viewer;` と `pub use viewer::ViewerSettings;` を足す。

`crates/kgd-presentation/src/viewer/guard.rs` を作る。

```rust
//! ビューアに届いたリクエストを、送信元と Cloudflare 経由の印で通すかどうか決める。

use std::{
    net::{IpAddr, SocketAddr},
    sync::Arc,
};

use axum::{
    extract::{ConnectInfo, Request, State},
    http::{HeaderMap, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use ipnet::IpNet;
use tracing::warn;

#[cfg(test)]
mod tests {
    use axum::http::HeaderValue;

    use super::*;

    fn lan() -> Vec<IpNet> {
        ["10.0.0.0/8", "172.16.0.0/12", "192.168.0.0/16", "fd00::/8"]
            .iter()
            .map(|net| net.parse().unwrap())
            .collect()
    }

    fn ip(value: &str) -> IpAddr {
        value.parse().unwrap()
    }

    /// 送信元と Cloudflare の印の組み合わせごとに、通すか拒否の理由が決まることを確認する。
    ///
    /// loopback は既定の許可リストに無いため、同じホストの cloudflared から届くリクエストは拒否される。
    /// IPv4 射影アドレスは IPv4 に戻してから照合する (`[::]` で待ち受けたとき LAN の端末がこの形で届くため)。
    #[test]
    fn decide_access_follows_the_allowlist_and_cloudflare_marks() {
        let cases = [
            ("192.168.1.10", false, Ok(())),
            ("10.1.2.3", false, Ok(())),
            ("fd12::1", false, Ok(())),
            ("::ffff:192.168.1.10", false, Ok(())),
            ("127.0.0.1", false, Err(Denial::OutsideAllowlist)),
            ("::1", false, Err(Denial::OutsideAllowlist)),
            ("203.0.113.5", false, Err(Denial::OutsideAllowlist)),
            ("192.168.1.10", true, Err(Denial::ViaCloudflare)),
            ("127.0.0.1", true, Err(Denial::ViaCloudflare)),
        ];

        for (peer, via_cloudflare, expected) in cases {
            assert_eq!(
                decide_access(ip(peer), via_cloudflare, &lan()),
                expected,
                "peer = {peer}, via_cloudflare = {via_cloudflare}"
            );
        }
    }

    /// Cloudflare のエッジが付けるヘッダのどれか 1 つでもあれば、Cloudflare 経由とみなすことを確認する。
    #[test]
    fn has_cloudflare_marks_detects_each_header() {
        let marked = [
            ("cf-connecting-ip", "203.0.113.5"),
            ("cf-ray", "8c1f2a3b4c5d6e7f-NRT"),
            ("cdn-loop", "cloudflare; loops=1"),
            ("cdn-loop", "Cloudflare"),
        ];
        for (name, value) in marked {
            let mut headers = HeaderMap::new();
            headers.insert(name, HeaderValue::from_static(value));
            assert!(has_cloudflare_marks(&headers), "{name}: {value}");
        }
    }

    /// Cloudflare と関係の無いヘッダだけなら、Cloudflare 経由とみなさないことを確認する。
    #[test]
    fn has_cloudflare_marks_ignores_other_headers() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", HeaderValue::from_static("203.0.113.5"));
        headers.insert("cdn-loop", HeaderValue::from_static("fastly"));

        assert!(!has_cloudflare_marks(&headers));
    }
}
```

`viewer/mod.rs` の `mod guard;` はそのまま使う。

- [ ] **Step 3: テストが失敗することを確かめる**

Run: `cargo test -p kgd-presentation viewer::guard`
Expected: FAIL (`cannot find function decide_access` などのコンパイルエラー)

- [ ] **Step 4: 実装する**

`guard.rs` の `#[cfg(test)]` の前に書く。

```rust
/// Cloudflare のエッジが付けるヘッダのうち、あるだけで Cloudflare 経由とみなすもの。
///
/// エッジが付与するため、インターネット側の利用者には取り除けない。
const CLOUDFLARE_HEADERS: [&str; 2] = ["cf-connecting-ip", "cf-ray"];

/// リクエストを拒否した理由。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Denial {
    /// Cloudflare を経由して届いた
    ViaCloudflare,
    /// 送信元が許可リストに無い
    OutsideAllowlist,
}

/// Cloudflare を経由したことを示すヘッダがあるかを返す。
///
/// `Cf-Connecting-IP` か `Cf-Ray` があるか、`Cdn-Loop` に `cloudflare` が含まれていれば true。
pub(super) fn has_cloudflare_marks(headers: &HeaderMap) -> bool {
    CLOUDFLARE_HEADERS
        .iter()
        .any(|name| headers.contains_key(*name))
        || headers.get_all("cdn-loop").iter().any(|value| {
            value
                .to_str()
                .is_ok_and(|value| value.to_ascii_lowercase().contains("cloudflare"))
        })
}

/// 送信元と Cloudflare 経由の印から、リクエストを通すかどうかを決める。
///
/// Cloudflare 経由なら送信元によらず拒否する。送信元は IPv4 射影アドレスを IPv4 に
/// 戻してから許可リストと照合する。
pub(super) fn decide_access(
    peer: IpAddr,
    via_cloudflare: bool,
    allowed: &[IpNet],
) -> Result<(), Denial> {
    if via_cloudflare {
        return Err(Denial::ViaCloudflare);
    }
    let peer = peer.to_canonical();
    if allowed.iter().any(|net| net.contains(&peer)) {
        Ok(())
    } else {
        Err(Denial::OutsideAllowlist)
    }
}

/// ビューアのルートにかけるミドルウェア。拒否したら 403 を返し、理由を warn のログに残す。
///
/// 送信元はソケットの相手アドレスだけを使い、`X-Forwarded-For` などのヘッダは見ない。
pub(super) async fn guard(
    State(allowed): State<Arc<[IpNet]>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    request: Request,
    next: Next,
) -> Response {
    let via_cloudflare = has_cloudflare_marks(request.headers());
    match decide_access(peer.ip(), via_cloudflare, &allowed) {
        Ok(()) => next.run(request).await,
        Err(denial) => {
            warn!(
                %peer,
                ?denial,
                path = %request.uri().path(),
                "Rejected location viewer request"
            );
            StatusCode::FORBIDDEN.into_response()
        }
    }
}
```

`guard` はこの時点ではどこからも使われないため、`cargo clippy` が dead_code を出す。
Task 7 で使うまでの間は、`viewer/mod.rs` の `mod guard;` に `#[allow(dead_code)] // Task 7 で viewer_router から使う` を付けておき、Task 7 で外す。

- [ ] **Step 5: テストが通ることを確かめる**

Run: `cargo test -p kgd-presentation viewer::guard`
Expected: PASS (3 件)

- [ ] **Step 6: コミットする**

```bash
~/.nix-profile/bin/just validate
git add Cargo.lock crates/kgd-presentation/Cargo.toml crates/kgd-presentation/src/lib.rs crates/kgd-presentation/src/viewer
git -c commit.gpgsign=false commit -m "feat: 送信元と Cloudflare 経由の印でビューアへのアクセスを判定する

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 6: API の型、Presenter、スキーマの書き出し

**Files:**
- Modify: `Cargo.toml` (`[workspace.dependencies]` に `schemars`)
- Modify: `crates/kgd-presentation/Cargo.toml`
- Create: `crates/kgd-presentation/src/viewer/dto.rs`
- Create: `crates/kgd-presentation/src/viewer/presenter.rs`
- Modify: `crates/kgd-presentation/src/viewer/mod.rs`
- Create: `web/src/api/schema.json` (テストが書き出す)

**Interfaces:**
- Consumes: `LocationHistory`、`DailyLocationSummary` (Task 3)、`LocationSummary`、`TrackSegment`、`Activity`
- Produces (`viewer/dto.rs`、crate 内。名前は JSON Schema の `$defs` と TypeScript の型名になる):
  - `HistoryQuery { from: NaiveDate, to: NaiveDate }` (Deserialize)
  - `HistoryResponse { range: HistoryRange, total: HistorySummary, days: Vec<DailySummary>, track: Track, track_meta: TrackMeta }`
  - `HistoryRange { from: NaiveDate, to: NaiveDate, timezone: String }`
  - `HistorySummary { distance_m: f64, distance_by_activity: DistanceByActivity, moving_s: i64, stationary_s: i64, point_count: usize, excluded_count: usize, first_at: Option<DateTime<Utc>>, last_at: Option<DateTime<Utc>> }`
  - `DistanceByActivity { walking: f64, cycling: f64, automotive: f64, unknown: f64 }`
  - `DailySummary { date: NaiveDate, summary: HistorySummary }`
  - `Track { type: FeatureCollectionType, features: Vec<TrackFeature> }`、`TrackFeature { type: FeatureType, properties: TrackProperties, geometry: LineString }`、`TrackProperties { activity: ActivityKind }`、`LineString { type: LineStringType, coordinates: Vec<[f64; 2]> }`
  - `ActivityKind` (`walking` / `cycling` / `automotive` / `stationary` / `unknown`)
  - `TrackMeta { original_points: usize, returned_points: usize, simplified: bool }`
  - `ErrorResponse { error: String }`
- Produces (`viewer/presenter.rs`): `pub(super) fn present_history(history: &LocationHistory) -> HistoryResponse`

1 日ぶんの集計は、設計書の例のように日付と集計項目を平たく並べず、`{ "date": ..., "summary": { ... } }` と入れ子にする。
`#[serde(flatten)]` を使うと JSON Schema が `allOf` などを含む形になり、変換スクリプトの対応範囲が広がるためである。
設計書の例もこの形に合わせて直す (Step 7)。

- [ ] **Step 1: 依存を足す**

`Cargo.toml` の `[workspace.dependencies]` の末尾に足す。

```toml
# JSON Schema of the viewer API (valibot schemas are generated from it)
schemars = { version = "1.2", features = ["chrono04"] }
```

`crates/kgd-presentation/Cargo.toml` の `[dependencies]` に足す。

```toml
chrono = { workspace = true, features = ["serde"] }
schemars.workspace = true
```

既存の `chrono.workspace = true` の行は上の行で置き換える (`NaiveDate` をクエリから読むのに chrono の `serde` 機能が要る)。
Presenter の `Tz::name` は値のメソッドなので、`chrono-tz` は `[dev-dependencies]` のままでよい。

- [ ] **Step 2: 型を書く**

`crates/kgd-presentation/src/viewer/dto.rs` を作る。

```rust
//! ビューアの API の入出力の型。
//!
//! 画面側の valibot のスキーマはこの型から生成する。型を変えたら `just gen-api` で
//! `web/src/api/schema.json` と `web/src/api/schema.gen.ts` を作り直す。

use chrono::{DateTime, NaiveDate, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

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
```

- [ ] **Step 3: スキーマのスナップショットのテストを書く**

`dto.rs` の末尾に足す。

```rust
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
```

`viewer/mod.rs` に `mod dto;` を足す。
型はまだどこからも使われないため、Task 7 で使うまでの間は `#[allow(dead_code)] // Task 7 で API のハンドラから使う` を `mod dto;` に付けておく。

- [ ] **Step 4: スキーマを書き出して中身を確かめる**

Run: `UPDATE_API_SCHEMA=1 cargo test -p kgd-presentation viewer::dto`
Expected: PASS。`web/src/api/schema.json` ができる

Run: `cargo test -p kgd-presentation viewer::dto`
Expected: PASS

`web/src/api/schema.json` を開き、次を目で確かめる。
Task 11 の変換スクリプトはこの形を前提にしている。

- ルートに `"$schema"`、`"title": "ViewerApi"`、`"$defs"` がある
- `NaiveDate` は `{"type": "string", "format": "date"}`、`DateTime<Utc>` は `{"type": "string", "format": "date-time"}`
- `first_at` は `{"type": ["string", "null"], "format": "date-time"}` で、`required` に含まれない
- `ActivityKind` などの列挙型は、列挙子に doc コメントがあるため `oneOf` の各要素が `{"type": "string", "const": ..., "description": ...}` の形
- `coordinates` の要素 (`[f64; 2]`) が `minItems`/`maxItems` を持つ `array` か、`prefixItems` を持つ `array` のどちらか

これと違う形が出ていたら、Task 11 の変換スクリプトとそのテストに、その形の対応を足す。

- [ ] **Step 5: Presenter の失敗するテストを書く**

`crates/kgd-presentation/src/viewer/presenter.rs` を作る。

```rust
//! ユースケースの結果を API の応答の型に変換する。

use kgd_application::LocationHistory;
use kgd_domain::{Activity, LocationSummary, TrackSegment};

use super::dto::{
    ActivityKind, DailySummary, DistanceByActivity, FeatureCollectionType, FeatureType,
    HistoryRange, HistoryResponse, HistorySummary, LineString, LineStringType, Track,
    TrackFeature, TrackMeta, TrackProperties,
};

#[cfg(test)]
mod tests {
    use chrono::{DateTime, NaiveDate, TimeDelta, TimeZone as _, Utc};

    use kgd_application::DailyLocationSummary;
    use kgd_domain::TrackPoint;

    use super::*;

    fn at(minute: i64) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 1, 0, 0, 0).unwrap() + TimeDelta::minutes(minute)
    }

    fn point(minute: i64, lat: f64, lon: f64) -> TrackPoint {
        TrackPoint {
            at: at(minute),
            lat,
            lon,
            accuracy_m: Some(10),
            activity: None,
        }
    }

    fn summary() -> LocationSummary {
        LocationSummary {
            point_count: 3,
            excluded_count: 1,
            distance_m: 1500.0,
            distance_by_activity: vec![
                (Some(Activity::Automotive), 1000.0),
                (None, 500.0),
            ],
            moving: TimeDelta::minutes(20),
            stationary: TimeDelta::seconds(90),
            first_at: Some(at(0)),
            last_at: Some(at(30)),
        }
    }

    fn history(segments: Vec<TrackSegment>, original: usize, returned: usize) -> LocationHistory {
        let date = NaiveDate::from_ymd_opt(2026, 9, 1).unwrap();
        LocationHistory {
            from: date,
            to: date,
            timezone: chrono_tz::Asia::Tokyo,
            total: summary(),
            days: vec![DailyLocationSummary {
                date,
                summary: summary(),
            }],
            segments,
            original_points: original,
            returned_points: returned,
        }
    }

    /// 期間と集計が API の型へ写され、移動種別ごとの距離が 4 つの項目に振り分けられることを確認する。
    #[test]
    fn present_history_maps_range_and_summaries() {
        let response = present_history(&history(Vec::new(), 0, 0));

        assert_eq!(response.range.timezone, "Asia/Tokyo");
        assert_eq!(response.range.from.to_string(), "2026-09-01");
        assert_eq!(
            response.total.distance_by_activity,
            DistanceByActivity {
                walking: 0.0,
                cycling: 0.0,
                automotive: 1000.0,
                unknown: 500.0,
            }
        );
        assert_eq!(response.total.moving_s, 1200);
        assert_eq!(response.total.stationary_s, 90);
        assert_eq!(response.total.first_at, Some(at(0)));
        assert_eq!(response.days.len(), 1);
        assert_eq!(response.days[0].date.to_string(), "2026-09-01");
        assert_eq!(response.days[0].summary, response.total);
    }

    /// 区間が `[経度, 緯度]` の LineString になり、移動種別が属性に入ることを確認する。
    #[test]
    fn present_history_turns_segments_into_line_strings() {
        let segments = vec![TrackSegment {
            activity: Some(Activity::Walking),
            points: vec![point(0, 35.0, 139.0), point(1, 35.1, 139.1)],
        }];

        let response = present_history(&history(segments, 2, 2));

        assert_eq!(response.track.kind, FeatureCollectionType::FeatureCollection);
        let feature = &response.track.features[0];
        assert_eq!(feature.properties.activity, ActivityKind::Walking);
        assert_eq!(
            feature.geometry.coordinates,
            vec![[139.0, 35.0], [139.1, 35.1]]
        );
    }

    /// 点が 1 つだけの区間は、同じ座標を 2 つ並べた LineString にすることを確認する。
    ///
    /// GeoJSON の LineString は 2 点以上を要求するため。
    #[test]
    fn present_history_duplicates_single_point_segments() {
        let segments = vec![TrackSegment {
            activity: None,
            points: vec![point(0, 35.0, 139.0)],
        }];

        let response = present_history(&history(segments, 1, 1));

        let feature = &response.track.features[0];
        assert_eq!(feature.properties.activity, ActivityKind::Unknown);
        assert_eq!(
            feature.geometry.coordinates,
            vec![[139.0, 35.0], [139.0, 35.0]]
        );
    }

    /// 間引いた後の点数が間引く前より少ないときだけ `simplified` が true になることを確認する。
    #[test]
    fn present_history_marks_simplified_only_when_points_were_removed() {
        let simplified = present_history(&history(Vec::new(), 30, 20));
        let untouched = present_history(&history(Vec::new(), 20, 20));

        assert_eq!(
            simplified.track_meta,
            TrackMeta {
                original_points: 30,
                returned_points: 20,
                simplified: true,
            }
        );
        assert!(!untouched.track_meta.simplified);
    }
}
```

`viewer/mod.rs` に `mod presenter;` を足す (Task 7 までは `#[allow(dead_code)] // Task 7 で API のハンドラから使う` を付ける)。

- [ ] **Step 6: Presenter を実装する**

`presenter.rs` の `#[cfg(test)]` の前に書く。

```rust
/// ユースケースの結果を `GET /viewer/api/history` の応答に変換する。
pub(super) fn present_history(history: &LocationHistory) -> HistoryResponse {
    HistoryResponse {
        range: HistoryRange {
            from: history.from,
            to: history.to,
            timezone: history.timezone.name().to_string(),
        },
        total: present_summary(&history.total),
        days: history
            .days
            .iter()
            .map(|day| DailySummary {
                date: day.date,
                summary: present_summary(&day.summary),
            })
            .collect(),
        track: Track {
            kind: FeatureCollectionType::FeatureCollection,
            features: history.segments.iter().map(present_segment).collect(),
        },
        track_meta: TrackMeta {
            original_points: history.original_points,
            returned_points: history.returned_points,
            simplified: history.returned_points < history.original_points,
        },
    }
}

/// 集計を API の型に変換する。
fn present_summary(summary: &LocationSummary) -> HistorySummary {
    let mut distance = DistanceByActivity {
        walking: 0.0,
        cycling: 0.0,
        automotive: 0.0,
        unknown: 0.0,
    };
    for (activity, meters) in &summary.distance_by_activity {
        match activity {
            Some(Activity::Walking) => distance.walking += meters,
            Some(Activity::Cycling) => distance.cycling += meters,
            Some(Activity::Automotive) => distance.automotive += meters,
            // 静止は距離を積算しないため、集計に現れない
            Some(Activity::Stationary) => {}
            None => distance.unknown += meters,
        }
    }
    HistorySummary {
        distance_m: summary.distance_m,
        distance_by_activity: distance,
        moving_s: summary.moving.num_seconds(),
        stationary_s: summary.stationary.num_seconds(),
        point_count: summary.point_count,
        excluded_count: summary.excluded_count,
        first_at: summary.first_at,
        last_at: summary.last_at,
    }
}

/// 区間を GeoJSON の Feature に変換する。点が 1 つなら同じ座標を 2 つ並べる。
fn present_segment(segment: &TrackSegment) -> TrackFeature {
    let mut coordinates: Vec<[f64; 2]> = segment
        .points
        .iter()
        .map(|point| [point.lon, point.lat])
        .collect();
    if let [only] = coordinates[..] {
        coordinates.push(only);
    }
    TrackFeature {
        kind: FeatureType::Feature,
        properties: TrackProperties {
            activity: activity_kind(segment.activity),
        },
        geometry: LineString {
            kind: LineStringType::LineString,
            coordinates,
        },
    }
}

/// 移動種別を API の型に変換する。
fn activity_kind(activity: Option<Activity>) -> ActivityKind {
    match activity {
        Some(Activity::Walking) => ActivityKind::Walking,
        Some(Activity::Cycling) => ActivityKind::Cycling,
        Some(Activity::Automotive) => ActivityKind::Automotive,
        Some(Activity::Stationary) => ActivityKind::Stationary,
        None => ActivityKind::Unknown,
    }
}
```

- [ ] **Step 7: テストを流し、設計書の例を直す**

Run: `cargo test -p kgd-presentation viewer::`
Expected: PASS (dto 1 件、presenter 4 件、guard 3 件)

`docs/superpowers/specs/2026-09-30-location-viewer-design.md` の「レスポンス」の例の `days` を次の形に直し、`original_points` の説明に「区間の境目の点は両方の区間で数える」を足す。

```json
  "days": [
    { "date": "2026-09-01", "summary": { "distance_m": 4000.0, "...": "total と同じ項目" } }
  ],
```

- [ ] **Step 8: コミットする**

```bash
~/.nix-profile/bin/just validate
git add Cargo.toml Cargo.lock crates/kgd-presentation web/src/api/schema.json docs/superpowers/specs/2026-09-30-location-viewer-design.md
git -c commit.gpgsign=false commit -m "feat: ビューアの API の型と Presenter を追加し、JSON Schema を書き出す

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---
### Task 7: ビューアのルータと履歴の API

**Files:**
- Create: `crates/kgd-presentation/src/viewer/api.rs`
- Create: `crates/kgd-presentation/src/viewer/tests.rs`
- Modify: `crates/kgd-presentation/src/viewer/mod.rs`
- Modify: `crates/kgd-presentation/src/lib.rs`

**Interfaces:**
- Consumes: `guard::guard` (Task 5)、`dto::{HistoryQuery, ErrorResponse}`、`presenter::present_history` (Task 6)、`BrowseLocationHistoryUseCase::browse` (Task 3)
- Produces:
  - `pub fn viewer_router(use_case: Arc<BrowseLocationHistoryUseCase>, settings: ViewerSettings) -> Router` (`kgd_presentation::` から公開)
  - `api::handle_history`、`api::handle_not_found`、`pub(super) fn api::not_found() -> Response` (Task 8 が使う)
  - `const MAX_RANGE_DAYS: i64 = 3660`

設計書は期間の長さに上限を設けないとしている。
ただし URL を手で書き換えて年の桁を誤る (`0026-09-01` など) と、数十万日ぶんの日ごとの集計を作ることになる。
これは点の数とは関係なく応答が数百 MB になるため、10 年 (3660 日) を超える範囲だけは 400 で拒否する。
設計書の「既知の制約」にもこの上限を書き足す (Step 5)。

- [ ] **Step 1: 失敗するテストを書く**

`crates/kgd-presentation/src/viewer/tests.rs` を作る。

```rust
//! ビューアのルータのテスト。送信元は `MockConnectInfo` で差し替える。

use std::{
    net::SocketAddr,
    sync::{Arc, Mutex},
};

use anyhow::Result;
use axum::{
    Router,
    body::{Body, to_bytes},
    extract::connect_info::MockConnectInfo,
    http::{Request, StatusCode, header},
};
use chrono::{DateTime, TimeZone as _, Utc};
use serde_json::Value;
use tower::ServiceExt as _;

use kgd_application::{
    BrowseLocationHistoryUseCase, LocationHistorySettings, RecordLocationUseCase,
    ports::LocationRepository,
};
use kgd_domain::{OwnTracksMessage, TrackPoint};

use crate::{OwnTracksControllerSettings, owntracks_router};

use super::*;

/// 決まった点を返すか、常に失敗するリポジトリのスタブ。
///
/// kgd-application のモックは `#[cfg(test)]` で生成されるためクレート外からは使えない。
struct StubLocationRepository {
    /// `locations_between` が返す点
    points: Vec<TrackPoint>,
    /// `locations_between` を失敗させるかどうか
    should_fail: bool,
    /// `locations_between` に渡された範囲
    ranges: Arc<Mutex<Vec<(DateTime<Utc>, DateTime<Utc>)>>>,
}

impl StubLocationRepository {
    fn with_points(points: Vec<TrackPoint>) -> Self {
        Self {
            points,
            should_fail: false,
            ranges: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn failing() -> Self {
        Self {
            should_fail: true,
            ..Self::with_points(Vec::new())
        }
    }
}

#[async_trait::async_trait]
impl LocationRepository for StubLocationRepository {
    async fn insert_messages(&self, messages: &[OwnTracksMessage]) -> Result<usize> {
        Ok(messages.len())
    }

    async fn locations_between(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<TrackPoint>> {
        self.ranges.lock().unwrap().push((start, end));
        if self.should_fail {
            return Err(anyhow::anyhow!("stub: locations_between failed"));
        }
        Ok(self.points.clone())
    }
}

/// 2026-09-01 10:00 (Asia/Tokyo) の点を作る。
fn point() -> TrackPoint {
    TrackPoint {
        at: chrono_tz::Asia::Tokyo
            .with_ymd_and_hms(2026, 9, 1, 10, 0, 0)
            .unwrap()
            .to_utc(),
        lat: 35.68,
        lon: 139.76,
        accuracy_m: Some(10),
        activity: None,
    }
}

/// ビューアのルータを作る。許可リストは 192.168.0.0/16 だけにする。
fn viewer(repo: StubLocationRepository) -> Router {
    let use_case = Arc::new(BrowseLocationHistoryUseCase::new(
        Arc::new(repo),
        LocationHistorySettings {
            timezone: chrono_tz::Asia::Tokyo,
            max_accuracy_m: 200,
            max_track_points: 100,
        },
    ));
    viewer_router(
        use_case,
        ViewerSettings {
            allowed_cidrs: vec!["192.168.0.0/16".parse().unwrap()],
        },
    )
}

/// 送信元を `peer` にして 1 件のリクエストを送る。
async fn send(router: Router, peer: &str, request: Request<Body>) -> (StatusCode, Vec<u8>, Option<String>) {
    let peer: SocketAddr = SocketAddr::new(peer.parse().unwrap(), 50000);
    let response = router
        .layer(MockConnectInfo(peer))
        .oneshot(request)
        .await
        .unwrap();
    let status = response.status();
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .map(|value| value.to_str().unwrap().to_string());
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (status, body.to_vec(), content_type)
}

fn get(uri: &str) -> Request<Body> {
    Request::builder().uri(uri).body(Body::empty()).unwrap()
}

fn json(body: &[u8]) -> Value {
    serde_json::from_slice(body).expect("body should be JSON")
}

/// LAN の送信元には、期間の集計と軌跡を JSON で返すことを確認する。
#[tokio::test]
async fn history_returns_json_to_lan_peers() {
    let router = viewer(StubLocationRepository::with_points(vec![point()]));

    let (status, body, _) = send(
        router,
        "192.168.1.10",
        get("/viewer/api/history?from=2026-09-01&to=2026-09-02"),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let body = json(&body);
    assert_eq!(body["range"]["from"], "2026-09-01");
    assert_eq!(body["range"]["timezone"], "Asia/Tokyo");
    assert_eq!(body["days"].as_array().unwrap().len(), 2);
    assert_eq!(body["total"]["point_count"], 1);
    assert_eq!(body["track"]["type"], "FeatureCollection");
}

/// loopback の送信元は拒否することを確認する。
///
/// 同じホストの cloudflared はこの送信元で届くため。
#[tokio::test]
async fn history_rejects_loopback_peers() {
    let router = viewer(StubLocationRepository::with_points(Vec::new()));

    let (status, _, _) = send(
        router,
        "127.0.0.1",
        get("/viewer/api/history?from=2026-09-01&to=2026-09-01"),
    )
    .await;

    assert_eq!(status, StatusCode::FORBIDDEN);
}

/// LAN の送信元でも Cloudflare 経由の印があれば拒否し、リポジトリを読まないことを確認する。
#[tokio::test]
async fn history_rejects_requests_through_cloudflare() {
    let repo = StubLocationRepository::with_points(Vec::new());
    let ranges = repo.ranges.clone();
    let request = Request::builder()
        .uri("/viewer/api/history?from=2026-09-01&to=2026-09-01")
        .header("Cf-Connecting-IP", "203.0.113.5")
        .body(Body::empty())
        .unwrap();

    let (status, _, _) = send(viewer(repo), "192.168.1.10", request).await;

    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(ranges.lock().unwrap().is_empty());
}

/// 日付の形式が不正なら、理由を JSON で添えて 400 を返すことを確認する。
#[tokio::test]
async fn history_rejects_malformed_dates() {
    let router = viewer(StubLocationRepository::with_points(Vec::new()));

    let (status, body, _) = send(
        router,
        "192.168.1.10",
        get("/viewer/api/history?from=not-a-date&to=2026-09-01"),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(json(&body)["error"].is_string());
}

/// 開始日が終了日より後なら 400 を返すことを確認する。
#[tokio::test]
async fn history_rejects_reversed_ranges() {
    let router = viewer(StubLocationRepository::with_points(Vec::new()));

    let (status, body, _) = send(
        router,
        "192.168.1.10",
        get("/viewer/api/history?from=2026-09-02&to=2026-09-01"),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(json(&body)["error"].is_string());
}

/// 10 年を超える範囲は、リポジトリを読まずに 400 を返すことを確認する。
///
/// 年の桁を誤った URL で、数十万日ぶんの日ごとの集計を作らないため。
#[tokio::test]
async fn history_rejects_ranges_longer_than_ten_years() {
    let repo = StubLocationRepository::with_points(Vec::new());
    let ranges = repo.ranges.clone();

    let (status, _, _) = send(
        viewer(repo),
        "192.168.1.10",
        get("/viewer/api/history?from=0026-09-01&to=2026-09-01"),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(ranges.lock().unwrap().is_empty());
}

/// リポジトリが失敗したら、中身を出さずに 500 と `internal error` を返すことを確認する。
#[tokio::test]
async fn history_hides_repository_errors() {
    let router = viewer(StubLocationRepository::failing());

    let (status, body, _) = send(
        router,
        "192.168.1.10",
        get("/viewer/api/history?from=2026-09-01&to=2026-09-01"),
    )
    .await;

    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(json(&body)["error"], "internal error");
}

/// `/viewer/api/` 以下の未知のパスには JSON の 404 を返すことを確認する。
///
/// HTML の 200 を返すと、画面側で原因のわかりにくいスキーマ検証のエラーになるため。
#[tokio::test]
async fn unknown_api_paths_return_json_not_found() {
    let router = viewer(StubLocationRepository::with_points(Vec::new()));

    let (status, body, content_type) =
        send(router, "192.168.1.10", get("/viewer/api/histroy")).await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(content_type.as_deref(), Some("application/json"));
    assert!(json(&body)["error"].is_string());
}

/// OwnTracks のルータと合わせても、`/pub` はこれまでどおり loopback から Basic 認証で通ることを確認する。
///
/// cloudflared は loopback から届くため、ガードを `/pub` にかけると位置の受信が止まる。
#[tokio::test]
async fn pub_still_accepts_loopback_requests_when_merged() {
    let owntracks = owntracks_router(
        Arc::new(RecordLocationUseCase::new(Arc::new(
            StubLocationRepository::with_points(Vec::new()),
        ))),
        OwnTracksControllerSettings {
            username: "ekuinox".to_string(),
            password: "secret".to_string(),
        },
    );
    let router = owntracks.merge(viewer(StubLocationRepository::with_points(Vec::new())));
    let request = Request::builder()
        .method("POST")
        .uri("/pub")
        .header("Authorization", "Basic ZWt1aW5veDpzZWNyZXQ=")
        .header("Content-Type", "application/json")
        .header("Cf-Connecting-IP", "203.0.113.5")
        .body(Body::from(r#"{"_type":"location","tst":1,"lat":1.0,"lon":2.0}"#))
        .unwrap();

    let (status, body, _) = send(router, "127.0.0.1", request).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, b"[]");
}
```

`viewer/mod.rs` の末尾に足す。

```rust
#[cfg(test)]
mod tests;
```

- [ ] **Step 2: テストが失敗することを確かめる**

Run: `cargo test -p kgd-presentation viewer::tests`
Expected: FAIL (`cannot find function viewer_router` などのコンパイルエラー)

- [ ] **Step 3: API のハンドラを実装する**

`crates/kgd-presentation/src/viewer/api.rs` を作る。

```rust
//! `/viewer/api/` 以下のハンドラ。

use std::sync::Arc;

use axum::{
    Json,
    extract::{Query, State, rejection::QueryRejection},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use tracing::error;

use kgd_application::BrowseLocationHistoryUseCase;

use super::{
    dto::{ErrorResponse, HistoryQuery},
    presenter::present_history,
};

/// 一度に選べる期間の上限 (日数)。
///
/// 期間の長さは原則として制限しないが、年の桁を誤った URL で数十万日ぶんの
/// 日ごとの集計を作らないよう、10 年を超える範囲だけは拒否する。
const MAX_RANGE_DAYS: i64 = 3660;

/// `GET /viewer/api/history`。期間の集計と地図用の軌跡を返す。
pub(super) async fn handle_history(
    State(use_case): State<Arc<BrowseLocationHistoryUseCase>>,
    query: Result<Query<HistoryQuery>, QueryRejection>,
) -> Response {
    let Query(query) = match query {
        Ok(query) => query,
        Err(rejection) => return error_response(StatusCode::BAD_REQUEST, rejection.body_text()),
    };
    if query.from > query.to {
        return error_response(
            StatusCode::BAD_REQUEST,
            "from must not be after to".to_string(),
        );
    }
    if (query.to - query.from).num_days() >= MAX_RANGE_DAYS {
        return error_response(
            StatusCode::BAD_REQUEST,
            format!("the range must be at most {MAX_RANGE_DAYS} days"),
        );
    }

    match use_case.browse(query.from, query.to).await {
        Ok(history) => Json(present_history(&history)).into_response(),
        Err(error) => {
            error!(?error, from = %query.from, to = %query.to, "Failed to browse location history");
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal error".to_string(),
            )
        }
    }
}

/// `/viewer/api/` 以下の未知のパス。
pub(super) async fn handle_not_found() -> Response {
    not_found()
}

/// JSON の 404 を返す。
pub(super) fn not_found() -> Response {
    error_response(StatusCode::NOT_FOUND, "not found".to_string())
}

/// エラーの応答を作る。
fn error_response(status: StatusCode, message: String) -> Response {
    (status, Json(ErrorResponse { error: message })).into_response()
}
```

`query.to - query.from` は chrono 0.4 の `NaiveDate` 同士の引き算で `TimeDelta` になる。
コンパイルが通らなければ `query.to.signed_duration_since(query.from)` に替える。

- [ ] **Step 4: ルータを実装する**

`viewer/mod.rs` を次の内容にする (Task 5、6 で付けた `#[allow(dead_code)]` は外す)。

```rust
//! ブラウザで位置ログを見るビューアのコントローラ。
//!
//! OwnTracks の受け口と同じ待ち受けで `/viewer/` 以下に置く。ログインは無く、
//! 送信元の許可リストと Cloudflare 経由の印で守る (ADR-0013)。

use std::sync::Arc;

use axum::{
    Router, middleware,
    routing::{any, get},
};
use ipnet::IpNet;

use kgd_application::BrowseLocationHistoryUseCase;

mod api;
mod dto;
mod guard;
mod presenter;

#[cfg(test)]
mod tests;

/// ビューアの設定。
#[derive(Debug, Clone)]
pub struct ViewerSettings {
    /// ビューアに届いてよい送信元
    pub allowed_cidrs: Vec<IpNet>,
}

/// ビューアのルータを組み立てる。
///
/// ガードはこのルータのルートにだけかかる。OwnTracks のルータへ merge しても、
/// `/pub` と `/healthz` には影響しない。送信元を取るため、サーバーは
/// `into_make_service_with_connect_info::<SocketAddr>()` で起動すること。
pub fn viewer_router(
    use_case: Arc<BrowseLocationHistoryUseCase>,
    settings: ViewerSettings,
) -> Router {
    let allowed: Arc<[IpNet]> = settings.allowed_cidrs.into();
    Router::new()
        .route("/viewer/api/history", get(api::handle_history))
        .route("/viewer/api/{*rest}", any(api::handle_not_found))
        .with_state(use_case)
        .layer(middleware::from_fn_with_state(allowed, guard::guard))
}
```

`crates/kgd-presentation/src/lib.rs` の公開を `pub use viewer::{ViewerSettings, viewer_router};` にする。

- [ ] **Step 5: テストが通ることを確かめ、設計書に上限を書き足す**

Run: `cargo test -p kgd-presentation viewer::`
Expected: PASS (tests 9 件、dto 1 件、presenter 4 件、guard 3 件)

`docs/superpowers/specs/2026-09-30-location-viewer-design.md` の「エラー」の表の 1 行目の状況に「、または 10 年 (3660 日) を超える範囲」を足し、「既知の制約」の 1 つ目の項目の後ろに次の文を足す。

```markdown
- ただし年の桁を誤った URL で数十万日ぶんの日ごとの集計を作らないよう、10 年 (3660 日) を超える範囲は 400 で拒否する
```

- [ ] **Step 6: コミットする**

```bash
~/.nix-profile/bin/just validate
git add crates/kgd-presentation docs/superpowers/specs/2026-09-30-location-viewer-design.md
git -c commit.gpgsign=false commit -m "feat: ビューアのルータと期間の位置ログを返す API を追加する

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 8: 埋め込んだ画面の配信

**Files:**
- Modify: `Cargo.toml` (`[workspace.dependencies]` に `rust-embed`)
- Modify: `crates/kgd-presentation/Cargo.toml`
- Create: `crates/kgd-presentation/src/viewer/assets.rs`
- Modify: `crates/kgd-presentation/src/viewer/mod.rs`
- Modify: `crates/kgd-presentation/src/viewer/tests.rs`

**Interfaces:**
- Consumes: `api::not_found()` (Task 7)
- Produces: `assets::redirect_to_index`、`assets::handle_index`、`assets::handle_asset` (ルータから使う)

rust-embed 8.12 は `#[allow_missing = true]` を付けると、`web/dist` が無くてもビルドが通り、中身が空になる。
デバッグビルドでは毎回ディスクから読むため、手元で `web/dist` を作り直せば kgd を再ビルドせずに反映される。
相対パスはどちらのビルドでも `CARGO_MANIFEST_DIR` (`crates/kgd-presentation`) を基準に解決される。

- [ ] **Step 1: 依存を足す**

`Cargo.toml` の `[workspace.dependencies]` の末尾に足す。

```toml
# Embed the built viewer (web/dist) into the binary
rust-embed = { version = "8.12", features = ["mime-guess"] }
```

`crates/kgd-presentation/Cargo.toml` の `[dependencies]` に `rust-embed.workspace = true` を足す。

- [ ] **Step 2: 失敗するテストを書く**

`viewer/tests.rs` の末尾に足す。

```rust
/// `/viewer` は `/viewer/` へ転送することを確認する。
///
/// Vite の `base` が `/viewer/` のため、末尾のスラッシュが無いと相対パスの読み込みがずれる。
#[tokio::test]
async fn viewer_without_trailing_slash_redirects() {
    let router = viewer(StubLocationRepository::with_points(Vec::new()));
    let response = router
        .layer(MockConnectInfo(SocketAddr::new(
            "192.168.1.10".parse().unwrap(),
            50000,
        )))
        .oneshot(get("/viewer"))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::PERMANENT_REDIRECT);
    assert_eq!(response.headers()[header::LOCATION], "/viewer/");
}

/// 画面のパスには、ビルド済みなら index.html を、未ビルドなら案内のページを HTML で返すことを確認する。
///
/// `web/dist` の有無は実行環境によって違うため、どちらでも HTML が返ることだけを確かめる。
#[tokio::test]
async fn viewer_pages_return_html() {
    for uri in ["/viewer/", "/viewer/some/page"] {
        let router = viewer(StubLocationRepository::with_points(Vec::new()));

        let (status, _, content_type) = send(router, "192.168.1.10", get(uri)).await;

        assert!(
            status == StatusCode::OK || status == StatusCode::SERVICE_UNAVAILABLE,
            "{uri}: {status}"
        );
        assert!(
            content_type.is_some_and(|value| value.starts_with("text/html")),
            "{uri}"
        );
    }
}

/// 画面のパスにもガードがかかることを確認する。
#[tokio::test]
async fn viewer_pages_reject_loopback_peers() {
    let router = viewer(StubLocationRepository::with_points(Vec::new()));

    let (status, _, _) = send(router, "127.0.0.1", get("/viewer/")).await;

    assert_eq!(status, StatusCode::FORBIDDEN);
}

/// 画面を配信するルートを足しても、`/viewer/api/` 以下の未知のパスは JSON の 404 のままであることを確認する。
#[tokio::test]
async fn unknown_api_paths_are_not_served_as_pages() {
    let router = viewer(StubLocationRepository::with_points(Vec::new()));

    let (status, _, content_type) =
        send(router, "192.168.1.10", get("/viewer/api/nested/unknown")).await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(content_type.as_deref(), Some("application/json"));
}
```

`crates/kgd-presentation/src/viewer/assets.rs` を作り、純粋な部分のテストを書く。

```rust
//! 埋め込んだ画面 (`web/dist`) の配信。

use axum::{
    extract::Path,
    http::{StatusCode, header},
    response::{Html, IntoResponse, Redirect, Response},
};
use rust_embed::{Embed, EmbeddedFile};

use super::api;

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
```

`viewer/mod.rs` に `mod assets;` を足す。

- [ ] **Step 3: テストが失敗することを確かめる**

Run: `cargo test -p kgd-presentation viewer::`
Expected: FAIL (`cannot find function cache_control` などのコンパイルエラー)

- [ ] **Step 4: 実装する**

`assets.rs` の `#[cfg(test)]` の前に書く。

```rust
/// ビルドした画面。`web/dist` が無くてもビルドは通り、そのときは空になる。
///
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
<p>画面が未ビルドです。リポジトリで <code>just web-build</code> を実行してから kgd を起動し直してください。</p>
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
/// 見つからないパスには index.html を返す。ただし `api/` 以下は JSON の 404、
/// `assets/` 以下は 404 にする (JS や CSS の代わりに HTML を返さないため)。
pub(super) async fn handle_asset(Path(path): Path<String>) -> Response {
    if path.starts_with("api/") {
        return api::not_found();
    }
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
```

`viewer/mod.rs` の `viewer_router` に画面のルートを足す。

```rust
    Router::new()
        .route("/viewer", get(assets::redirect_to_index))
        .route("/viewer/", get(assets::handle_index))
        .route("/viewer/{*path}", get(assets::handle_asset))
        .route("/viewer/api/history", get(api::handle_history))
        .route("/viewer/api/{*rest}", any(api::handle_not_found))
        .with_state(use_case)
        .layer(middleware::from_fn_with_state(allowed, guard::guard))
```

axum 0.8 のルーティング (matchit) は、静的なセグメントをワイルドカードより優先する。
そのため `/viewer/api/history` と `/viewer/api/{*rest}` は `/viewer/{*path}` より先に一致する。
ルータを組み立てるときにルートの衝突で panic した場合は、`/viewer/api/{*rest}` のルートを消す。
`handle_asset` の先頭で `api/` を見て JSON の 404 を返しているため、応答は変わらない。

- [ ] **Step 5: テストが通ることを確かめる**

Run: `cargo test -p kgd-presentation viewer::`
Expected: PASS (tests 13 件、assets 2 件、dto 1 件、presenter 4 件、guard 3 件)

- [ ] **Step 6: コミットする**

```bash
~/.nix-profile/bin/just validate
git add Cargo.toml Cargo.lock crates/kgd-presentation
git -c commit.gpgsign=false commit -m "feat: 埋め込んだビューアの画面を /viewer/ で配信する

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 9: 送信元を渡す起動とビューアの配線

**Files:**
- Modify: `crates/kgd-infrastructure/src/http_server.rs`
- Modify: `crates/kgd/src/bootstrap.rs`

**Interfaces:**
- Consumes: `viewer_router`、`ViewerSettings` (Task 7)、`ViewerConfig` (Task 4)、`BrowseLocationHistoryUseCase`、`LocationHistorySettings` (Task 3)
- Produces: なし (Composition Root)

- [ ] **Step 1: 送信元を渡してサーバーを起動する**

`crates/kgd-infrastructure/src/http_server.rs` の `serve_http` を書き換える。

```rust
/// 待ち受けソケットで HTTP サーバーを起動し、終了するまで待つ。
///
/// bind は [`bind_http`] で済んでいる前提。ここで発生するのは接続を
/// 受け付け始めた後の実行時エラーのみ。ハンドラが `ConnectInfo<SocketAddr>` で
/// 接続元のアドレスを取れるようにして起動する (ビューアのガードが使う)。
pub async fn serve_http(listener: TcpListener, router: Router) -> Result<()> {
    axum::serve(
        listener,
        router.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .context("HTTP server error")
}
```

- [ ] **Step 2: bootstrap でビューアを組み立てる**

`crates/kgd/src/bootstrap.rs` の `use` を直す。

- `kgd_application::{...}` に `BrowseLocationHistoryUseCase` と `LocationHistorySettings` を足す
- `kgd_presentation::{...}` に `ViewerSettings` と `viewer_router` を足す
- `use std::{sync::Arc, time::Duration};` を `use std::{net::{IpAddr, Ipv4Addr, Ipv6Addr}, sync::Arc, time::Duration};` にする
- `use tracing::info;` を `use tracing::{info, warn};` にする

位置情報の受け口を組み立てている `if let Some(location_config) = config.location.clone() { ... }` の中を、次のとおり 3 か所直す。

1. `RecordLocationUseCase::new(location_store)` を `RecordLocationUseCase::new(location_store.clone())` にする
2. `let router = owntracks_router(` を `let mut router = owntracks_router(` にする
3. `owntracks_router(...)` の文の直後 (`let listener = bind_http(...)` の前) に次を足す

```rust
        // ビューアは `[location.viewer]` を書いたときだけ有効にする (ADR-0013)。
        if let Some(viewer_config) = &location_config.viewer {
            let loopbacks = [IpAddr::V4(Ipv4Addr::LOCALHOST), IpAddr::V6(Ipv6Addr::LOCALHOST)];
            if viewer_config
                .allowed_cidrs
                .iter()
                .any(|net| loopbacks.iter().any(|ip| net.contains(ip)))
            {
                warn!(
                    "location.viewer.allowed_cidrs includes loopback. Requests through a local \
                     cloudflared are then blocked only by the Cloudflare header check"
                );
            }
            let browse = Arc::new(BrowseLocationHistoryUseCase::new(
                location_store,
                LocationHistorySettings {
                    timezone: diary_config.timezone,
                    max_accuracy_m: location_config.max_accuracy_m,
                    max_track_points: viewer_config.max_track_points,
                },
            ));
            router = router.merge(viewer_router(
                browse,
                ViewerSettings {
                    allowed_cidrs: viewer_config.allowed_cidrs.clone(),
                },
            ));
            info!("Location viewer enabled at /viewer/");
        }
```

- [ ] **Step 3: ビルドとテストを通す**

Run: `~/.nix-profile/bin/just validate && cargo test --all`
Expected: fmt、check、clippy がエラー無しで通り、すべてのテストが PASS

- [ ] **Step 4: 手元で起動して確かめる (DB と設定がある場合)**

手元に `config.toml` と PostgreSQL がある場合だけ行う。
無ければこの Step は飛ばし、Task 18 で本番相当の環境で確かめる。

`config.toml` の `[location]` に次を足して `just run` で起動する。

```toml
[location.viewer]
allowed_cidrs = ["192.168.0.0/16"]
```

別の端末で確かめる。

```bash
curl -s -o /dev/null -w '%{http_code}\n' http://127.0.0.1:8081/viewer/api/history?from=2026-09-01\&to=2026-09-01
curl -s -o /dev/null -w '%{http_code}\n' http://127.0.0.1:8081/healthz
curl -s "http://$(hostname -I | awk '{print $1}'):8081/viewer/api/history?from=2026-09-01&to=2026-09-01" | head -c 300
```

Expected: 1 行目は `403` (loopback は許可リストに無い)、2 行目は `200`、3 行目は `{"range":...` で始まる JSON (LAN のアドレスからは通る)

- [ ] **Step 5: コミットする**

```bash
git add crates/kgd-infrastructure/src/http_server.rs crates/kgd/src/bootstrap.rs
git -c commit.gpgsign=false commit -m "feat: 位置ログのビューアを OwnTracks の受け口に同居させる

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---
### Task 10: 画面のプロジェクトの足場

**Files:**
- Modify: `mise.toml`
- Modify: `.gitignore`
- Modify: `Justfile`
- Create: `web/package.json`、`web/aube-lock.yaml` (aube が作る)、`web/tsconfig.json`、`web/vite.config.ts`、`web/biome.json`、`web/index.html`、`web/src/main.tsx`、`web/src/App.tsx`、`web/src/index.css`

**Interfaces:**
- Produces:
  - `web/` で `aube run dev|build|typecheck|lint|format|test|gen` が使える (`gen` は Task 11 で中身を作る)
  - Justfile のレシピ `web-install`、`web-dev`、`web-build`、`web-check`
  - `web/dist/index.html` (ビルドの成果物、git 管理外)

依存のバージョンは 2026-09-30 時点の npm の最新である。
aube は npm 公式パッケージ `@endevco/aube` (2.6.1) と mise の `aube` のどちらでも入る。
手元と CI では mise、Docker では npm のパッケージを使う。

- [ ] **Step 1: ツールを入れる**

`mise.toml` を次の内容にする。

```toml
[tools]
"npm:@openai/codex" = "latest"
node = "22"
aube = "2.6"
```

Run: `mise install && mise exec -- aube --version && mise exec -- node --version`
Expected: aube 2.6.x と Node v22.18 以上が表示される (Node 22.18 以上は TypeScript の型注釈を外して `.ts` を直接実行できる)

以降の `aube` と `node` のコマンドは、mise の shims が PATH に無ければ `mise exec -- ` を前に付けて実行する。
Rust のビルドに使う PATH (Global Constraints) には nix を入れないこと。

`.gitignore` に足す。

```gitignore
# Viewer (web/)
web/node_modules/
web/dist/
```

- [ ] **Step 2: プロジェクトの設定ファイルを書く**

`web/package.json`:

```json
{
  "name": "kgd-viewer",
  "private": true,
  "type": "module",
  "devEngines": {
    "runtime": {
      "name": "node",
      "version": ">=22.18.0 <23",
      "onFail": "error"
    }
  },
  "scripts": {
    "dev": "vite",
    "build": "vite build",
    "typecheck": "tsc --noEmit",
    "lint": "biome check .",
    "format": "biome check --write .",
    "test": "vitest run",
    "gen": "node scripts/gen-api.ts"
  },
  "dependencies": {
    "@daypicker/react": "^10.0.2",
    "@vis.gl/react-maplibre": "^8.1.3",
    "maplibre-gl": "^6.11.2",
    "react": "^19.3.0",
    "react-dom": "^19.3.0",
    "recharts": "^3.10.1",
    "valibot": "^1.5.0"
  },
  "devDependencies": {
    "@biomejs/biome": "^2.5.14",
    "@types/geojson": "^7946.0.16",
    "@types/node": "^22",
    "@types/react": "^19",
    "@types/react-dom": "^19",
    "@vitejs/plugin-react": "^6.1.1",
    "typescript": "^7.0.2",
    "vite": "^8.3.1",
    "vitest": "^5.0.3"
  }
}
```

`web/tsconfig.json`:

```json
{
  "compilerOptions": {
    "target": "es2022",
    "lib": ["es2023", "dom", "dom.iterable"],
    "module": "esnext",
    "moduleResolution": "bundler",
    "jsx": "react-jsx",
    "strict": true,
    "noUncheckedIndexedAccess": true,
    "noEmit": true,
    "allowImportingTsExtensions": true,
    "verbatimModuleSyntax": true,
    "isolatedModules": true,
    "skipLibCheck": true,
    "types": ["vite/client", "node"]
  },
  "include": ["src", "scripts", "vite.config.ts"]
}
```

`scripts/` の `.ts` は Node が型注釈を外して直接実行するため、TypeScript 独自の実行時の構文 (`enum`、`namespace`、コンストラクタの引数プロパティ) は使わない。

`web/vite.config.ts`:

```ts
import react from '@vitejs/plugin-react';
import { defineConfig } from 'vitest/config';

export default defineConfig({
  // kgd は画面を /viewer/ 以下で配信する
  base: '/viewer/',
  plugins: [react()],
  server: {
    // 開発中は API を手元の kgd へ流す。kgd の allowed_cidrs に 127.0.0.1/32 を足しておくこと
    proxy: { '/viewer/api': 'http://127.0.0.1:8081' },
  },
  test: {
    environment: 'node',
    include: ['src/**/*.test.ts', 'scripts/**/*.test.ts'],
  },
});
```

`web/biome.json`:

```json
{
  "$schema": "./node_modules/@biomejs/biome/configuration_schema.json",
  "vcs": { "enabled": true, "clientKind": "git", "useIgnoreFile": true },
  "files": { "includes": ["**", "!src/api/schema.json", "!src/api/schema.gen.ts"] },
  "formatter": { "indentStyle": "space", "indentWidth": 2, "lineWidth": 100 },
  "javascript": { "formatter": { "quoteStyle": "single" } },
  "linter": { "enabled": true, "rules": { "recommended": true } }
}
```

`web/index.html`:

```html
<!doctype html>
<html lang="ja">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>位置ログ</title>
  </head>
  <body>
    <div id="root"></div>
    <script type="module" src="/src/main.tsx"></script>
  </body>
</html>
```

`web/src/index.css`:

```css
html,
body {
  margin: 0;
  font-family: system-ui, sans-serif;
  color: #212529;
}
```

`web/src/App.tsx` (Task 15 で置き換える仮の画面):

```tsx
export function App() {
  return <h1>位置ログ</h1>;
}
```

`web/src/main.tsx`:

```tsx
import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { App } from './App.tsx';
import './index.css';

const root = document.getElementById('root');
if (!root) {
  throw new Error('#root is missing in index.html');
}
createRoot(root).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
```

- [ ] **Step 3: 依存を入れてビルドする**

Run: `cd web && aube install && aube run typecheck && aube run lint && aube run build`
Expected: `web/aube-lock.yaml` と `web/dist/index.html` ができ、型チェックと lint がエラー無しで終わる

依存どうしの peer の範囲が合わずに入らないときは、エラーに出ている範囲を満たす最新版へ下げ、下げた理由をコミットメッセージに書く。
`biome check` が `src/api/` の生成物を対象にしてしまう場合は、`files.includes` の否定の書き方を Biome のドキュメントで確かめて直す (生成物はまだ無いので、Task 11 で確かめる)。

- [ ] **Step 4: Justfile にレシピを足す**

`Justfile` の `compose-local-down` の後ろに足す。

```make
# Install viewer dependencies (web/)
web-install:
    cd web && aube install

# Start the viewer dev server (proxies /viewer/api to 127.0.0.1:8081)
web-dev:
    cd web && aube run dev

# Build the viewer into web/dist (embedded into the kgd binary)
web-build:
    cd web && aube run build

# Check the viewer (typecheck, lint, test)
web-check:
    cd web && aube run typecheck && aube run lint && aube run test
```

Run: `~/.nix-profile/bin/just web-build`
Expected: `web/dist/index.html` が作り直される

- [ ] **Step 5: 埋め込まれることを確かめる**

Run: `cargo test -p kgd-presentation viewer::tests::viewer_pages_return_html -- --nocapture`
Expected: PASS (デバッグビルドは `web/dist` をディスクから読むため、今度は 200 の index.html が返る)

- [ ] **Step 6: コミットする**

```bash
git add mise.toml .gitignore Justfile web/package.json web/aube-lock.yaml web/tsconfig.json web/vite.config.ts web/biome.json web/index.html web/src
git -c commit.gpgsign=false commit -m "feat: ビューアの画面のプロジェクト (React, Vite, aube) を用意する

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 11: JSON Schema から valibot のスキーマを生成する

**Files:**
- Create: `web/scripts/json-schema-to-valibot.ts`
- Create: `web/scripts/json-schema-to-valibot.test.ts`
- Create: `web/scripts/gen-api.ts`
- Create: `web/src/api/schema.gen.ts` (生成物)
- Modify: `Justfile`

**Interfaces:**
- Consumes: `web/src/api/schema.json` (Task 6)
- Produces:
  - `export function generateValibotModule(root: Record<string, unknown>): string`
  - `export function convertSchema(schema: unknown, path: string): string`
  - `export class UnsupportedSchemaError extends Error`
  - `web/src/api/schema.gen.ts` は `$defs` の各型とルートについて `export const <Name>Schema` と `export type <Name> = v.InferOutput<typeof <Name>Schema>` を持つ。後続のタスクは `HistoryQuery`、`HistoryQuerySchema`、`HistoryResponse`、`HistoryResponseSchema`、`HistorySummary`、`ErrorResponseSchema` を使う
  - Justfile のレシピ `gen-api`

schemars 1.2 の出力のうち、今回の型が使う形は次のとおりである (Task 6 の Step 4 で確かめた形)。

| JSON Schema | valibot |
|---|---|
| `{"$ref": "#/$defs/X"}` | `XSchema` |
| `{"type": "string"}` | `v.string()` |
| `{"type": "string", "format": "date"}` | `v.pipe(v.string(), v.isoDate())` |
| `{"type": "string", "format": "date-time"}` | `v.pipe(v.string(), v.isoTimestamp())` |
| `{"type": "number", "format": "double"}` | `v.number()` |
| `{"type": "integer", "format": "uint", "minimum": 0}` | `v.pipe(v.number(), v.integer(), v.minValue(0))` |
| `{"type": "boolean"}` | `v.boolean()` |
| `{"type": ["X", "null"], ...}` | `v.nullable(<X の変換>)` |
| `{"anyOf": [<X>, {"type": "null"}]}` | `v.nullable(<X の変換>)` |
| `{"oneOf": [{"const": "a", ...}, ...]}`、`{"enum": ["a", ...]}` | `v.picklist(["a", ...])` |
| `{"const": "a"}` | `v.literal("a")` |
| `{"type": "object", "properties": ..., "required": [...]}` | `v.object({...})`。`required` に無いものは `v.optional(...)`。`additionalProperties: false` なら `v.strictObject` |
| `{"type": "array", "items": X, "minItems": n, "maxItems": m}` | `v.pipe(v.array(X), v.minLength(n), v.maxLength(m))` |
| `{"type": "array", "prefixItems": [...]}` | `v.tuple([...])` |

`$schema`、`title`、`description` は注釈として読み飛ばす。
それ以外のキーワードや未知の `format` に出会ったら `UnsupportedSchemaError` を投げる。

- [ ] **Step 1: 失敗するテストを書く**

`web/scripts/json-schema-to-valibot.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import {
  convertSchema,
  generateValibotModule,
  UnsupportedSchemaError,
} from './json-schema-to-valibot.ts';

describe('convertSchema', () => {
  it('converts primitive types and ignores numeric formats', () => {
    expect(convertSchema({ type: 'string' }, 'x')).toBe('v.string()');
    expect(convertSchema({ type: 'number', format: 'double' }, 'x')).toBe('v.number()');
    expect(convertSchema({ type: 'integer', format: 'uint', minimum: 0 }, 'x')).toBe(
      'v.pipe(v.number(), v.integer(), v.minValue(0))',
    );
    expect(convertSchema({ type: 'boolean', description: 'flag' }, 'x')).toBe('v.boolean()');
  });

  it('converts date and date-time strings', () => {
    expect(convertSchema({ type: 'string', format: 'date' }, 'x')).toBe(
      'v.pipe(v.string(), v.isoDate())',
    );
    expect(convertSchema({ type: 'string', format: 'date-time' }, 'x')).toBe(
      'v.pipe(v.string(), v.isoTimestamp())',
    );
  });

  it('converts nullable types in both forms', () => {
    expect(convertSchema({ type: ['string', 'null'], format: 'date-time' }, 'x')).toBe(
      'v.nullable(v.pipe(v.string(), v.isoTimestamp()))',
    );
    expect(convertSchema({ anyOf: [{ $ref: '#/$defs/Kind' }, { type: 'null' }] }, 'x')).toBe(
      'v.nullable(KindSchema)',
    );
  });

  it('converts string enums in both forms', () => {
    expect(
      convertSchema(
        {
          oneOf: [
            { type: 'string', const: 'walking', description: '徒歩' },
            { type: 'string', const: 'automotive', description: '車' },
          ],
        },
        'x',
      ),
    ).toBe('v.picklist(["walking","automotive"])');
    expect(convertSchema({ type: 'string', enum: ['Feature'] }, 'x')).toBe(
      'v.picklist(["Feature"])',
    );
    expect(convertSchema({ const: 'Feature' }, 'x')).toBe('v.literal("Feature")');
  });

  it('converts objects with required and optional properties', () => {
    const schema = {
      type: 'object',
      properties: { from: { type: 'string' }, note: { type: ['string', 'null'] } },
      required: ['from'],
    };
    expect(convertSchema(schema, 'x')).toBe(
      'v.object({ "from": v.string(), "note": v.optional(v.nullable(v.string())) })',
    );
    expect(convertSchema({ ...schema, additionalProperties: false }, 'x')).toBe(
      'v.strictObject({ "from": v.string(), "note": v.optional(v.nullable(v.string())) })',
    );
  });

  it('converts arrays and tuples', () => {
    expect(
      convertSchema(
        { type: 'array', items: { type: 'number', format: 'double' }, minItems: 2, maxItems: 2 },
        'x',
      ),
    ).toBe('v.pipe(v.array(v.number()), v.minLength(2), v.maxLength(2))');
    expect(
      convertSchema(
        {
          type: 'array',
          prefixItems: [{ type: 'number' }, { type: 'number' }],
          items: false,
          minItems: 2,
          maxItems: 2,
        },
        'x',
      ),
    ).toBe('v.tuple([v.number(), v.number()])');
  });

  it('rejects unsupported keywords and formats', () => {
    expect(() => convertSchema({ type: 'string', pattern: '^a' }, 'x.name')).toThrow(
      UnsupportedSchemaError,
    );
    expect(() => convertSchema({ type: 'string', format: 'email' }, 'x')).toThrow(
      UnsupportedSchemaError,
    );
    expect(() => convertSchema({ allOf: [] }, 'x')).toThrow(UnsupportedSchemaError);
    expect(() => convertSchema({ $ref: 'other.json#/X' }, 'x')).toThrow(UnsupportedSchemaError);
  });
});

describe('generateValibotModule', () => {
  it('emits definitions after the definitions they reference', () => {
    const code = generateValibotModule({
      $schema: 'https://json-schema.org/draft/2020-12/schema',
      title: 'Root',
      type: 'object',
      properties: { a: { $ref: '#/$defs/A' } },
      required: ['a'],
      $defs: {
        A: {
          type: 'object',
          properties: { b: { $ref: '#/$defs/B' } },
          required: ['b'],
        },
        B: { type: 'string' },
      },
    });

    const b = code.indexOf('export const BSchema');
    const a = code.indexOf('export const ASchema');
    const root = code.indexOf('export const RootSchema');
    expect(b).toBeGreaterThan(-1);
    expect(b).toBeLessThan(a);
    expect(a).toBeLessThan(root);
    expect(code).toContain("import * as v from 'valibot';");
    expect(code).toContain('export type A = v.InferOutput<typeof ASchema>;');
  });

  it('rejects circular references and unknown definitions', () => {
    expect(() =>
      generateValibotModule({
        title: 'Root',
        $ref: '#/$defs/A',
        $defs: { A: { $ref: '#/$defs/A' } },
      }),
    ).toThrow(UnsupportedSchemaError);
    expect(() => generateValibotModule({ title: 'Root', $ref: '#/$defs/Missing' })).toThrow(
      UnsupportedSchemaError,
    );
  });
});
```

- [ ] **Step 2: テストが失敗することを確かめる**

Run: `cd web && aube run test`
Expected: FAIL (`Failed to resolve import "./json-schema-to-valibot.ts"`)

- [ ] **Step 3: 変換スクリプトを実装する**

`web/scripts/json-schema-to-valibot.ts`:

```ts
/**
 * schemars が出す JSON Schema (draft 2020-12) を、valibot のスキーマの TypeScript コードに変換する。
 *
 * 対応するのは kgd のビューアの API が使う範囲に限る。知らないキーワードに出会ったら、
 * 黙って無視せずに UnsupportedSchemaError を投げる。
 */

type JsonObject = Record<string, unknown>;

/** 対応していない JSON Schema に出会ったことを表す。 */
export class UnsupportedSchemaError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'UnsupportedSchemaError';
  }
}

/** 検証に影響しない注釈のキーワード。 */
const ANNOTATIONS = ['$schema', 'title', 'description'];

/** 変換できるキーワード。 */
const SUPPORTED_KEYS = new Set([
  ...ANNOTATIONS,
  '$ref',
  'anyOf',
  'oneOf',
  'enum',
  'const',
  'type',
  'properties',
  'required',
  'additionalProperties',
  'items',
  'prefixItems',
  'minItems',
  'maxItems',
  'format',
  'minimum',
]);

/** schemars が Rust の数値型から付ける format。値の検証には影響しない。 */
const NUMERIC_FORMATS = new Set([
  'double',
  'float',
  'int',
  'int8',
  'int16',
  'int32',
  'int64',
  'uint',
  'uint8',
  'uint16',
  'uint32',
  'uint64',
]);

/** `$defs` の名前として受け付ける形。TypeScript の識別子になる。 */
const DEFINITION_NAME = /^[A-Za-z_][A-Za-z0-9_]*$/;

/**
 * ルートのスキーマから、`$defs` の各型とルートの valibot スキーマを並べたモジュールを作る。
 *
 * ルートの名前は `title` から取る。参照される型が先に来るように並べる。
 */
export function generateValibotModule(root: JsonObject): string {
  const { $defs, ...rootSchema } = root;
  const rootName = root.title;
  if (typeof rootName !== 'string' || !DEFINITION_NAME.test(rootName)) {
    throw new UnsupportedSchemaError('root: "title" must be a valid identifier');
  }
  const definitions: Record<string, JsonObject> = {
    ...asObject($defs ?? {}, '#/$defs'),
    [rootName]: rootSchema,
  };

  const lines = [
    '// scripts/gen-api.ts が src/api/schema.json から生成する。手で編集しないこと。',
    "import * as v from 'valibot';",
    '',
  ];
  for (const name of dependencyOrder(definitions)) {
    lines.push(`export const ${name}Schema = ${convertSchema(definitions[name], name)};`);
    lines.push(`export type ${name} = v.InferOutput<typeof ${name}Schema>;`);
    lines.push('');
  }
  return lines.join('\n');
}

/** 1 つのスキーマを valibot の式に変換する。`path` はエラーの場所の表示に使う。 */
export function convertSchema(schema: unknown, path: string): string {
  const s = asObject(schema, path);
  for (const key of Object.keys(s)) {
    if (!SUPPORTED_KEYS.has(key)) {
      throw new UnsupportedSchemaError(`${path}: unsupported keyword "${key}"`);
    }
  }

  if (s.$ref !== undefined) {
    return `${refName(s.$ref, path)}Schema`;
  }
  if (s.anyOf !== undefined) {
    return convertNullableUnion(s.anyOf, path);
  }
  if (s.oneOf !== undefined) {
    return convertConstUnion(s.oneOf, path);
  }
  if (s.enum !== undefined) {
    return picklist(s.enum, path);
  }
  if (s.const !== undefined) {
    return literal(s.const, path);
  }
  if (Array.isArray(s.type)) {
    const others = s.type.filter((type) => type !== 'null');
    if (others.length !== 1 || others.length === s.type.length) {
      throw new UnsupportedSchemaError(`${path}: only "[X, null]" type unions are supported`);
    }
    return `v.nullable(${convertSchema({ ...s, type: others[0] }, path)})`;
  }

  switch (s.type) {
    case 'object':
      return convertObject(s, path);
    case 'array':
      return convertArray(s, path);
    case 'string':
      return convertString(s, path);
    case 'number':
      return convertNumber(s, path, false);
    case 'integer':
      return convertNumber(s, path, true);
    case 'boolean':
      return 'v.boolean()';
    case 'null':
      return 'v.null()';
    default:
      throw new UnsupportedSchemaError(`${path}: unsupported type ${JSON.stringify(s.type)}`);
  }
}

function convertObject(s: JsonObject, path: string): string {
  const properties = asObject(s.properties ?? {}, `${path}.properties`);
  const required = new Set(stringArray(s.required ?? [], `${path}.required`));
  if (s.additionalProperties !== undefined && typeof s.additionalProperties !== 'boolean') {
    throw new UnsupportedSchemaError(`${path}: only boolean "additionalProperties" is supported`);
  }
  const entries = Object.entries(properties).map(([key, value]) => {
    const inner = convertSchema(value, `${path}.${key}`);
    return `${JSON.stringify(key)}: ${required.has(key) ? inner : `v.optional(${inner})`}`;
  });
  const factory = s.additionalProperties === false ? 'v.strictObject' : 'v.object';
  return `${factory}({ ${entries.join(', ')} })`;
}

function convertArray(s: JsonObject, path: string): string {
  if (s.prefixItems !== undefined) {
    if (s.items !== undefined && s.items !== false) {
      throw new UnsupportedSchemaError(`${path}: "prefixItems" with extra "items" is unsupported`);
    }
    const items = arrayOf(s.prefixItems, `${path}.prefixItems`).map((item, index) =>
      convertSchema(item, `${path}[${index}]`),
    );
    return `v.tuple([${items.join(', ')}])`;
  }
  const actions: string[] = [];
  if (s.minItems !== undefined) {
    actions.push(`v.minLength(${nonNegativeInteger(s.minItems, `${path}.minItems`)})`);
  }
  if (s.maxItems !== undefined) {
    actions.push(`v.maxLength(${nonNegativeInteger(s.maxItems, `${path}.maxItems`)})`);
  }
  return pipe(`v.array(${convertSchema(s.items, `${path}[]`)})`, actions);
}

function convertString(s: JsonObject, path: string): string {
  switch (s.format) {
    case undefined:
      return 'v.string()';
    case 'date':
      return 'v.pipe(v.string(), v.isoDate())';
    case 'date-time':
      return 'v.pipe(v.string(), v.isoTimestamp())';
    default:
      throw new UnsupportedSchemaError(`${path}: unsupported string format ${JSON.stringify(s.format)}`);
  }
}

function convertNumber(s: JsonObject, path: string, integer: boolean): string {
  if (s.format !== undefined && !NUMERIC_FORMATS.has(String(s.format))) {
    throw new UnsupportedSchemaError(`${path}: unsupported number format ${JSON.stringify(s.format)}`);
  }
  const actions: string[] = [];
  if (integer) {
    actions.push('v.integer()');
  }
  if (s.minimum !== undefined) {
    if (typeof s.minimum !== 'number') {
      throw new UnsupportedSchemaError(`${path}: "minimum" must be a number`);
    }
    actions.push(`v.minValue(${s.minimum})`);
  }
  return pipe('v.number()', actions);
}

function convertNullableUnion(members: unknown, path: string): string {
  const list = arrayOf(members, `${path}.anyOf`);
  const isNull = (member: unknown) => {
    const object = asObject(member, `${path}.anyOf`);
    return object.type === 'null' && Object.keys(object).every((key) => key === 'type' || ANNOTATIONS.includes(key));
  };
  const others = list.filter((member) => !isNull(member));
  if (list.length !== 2 || others.length !== 1) {
    throw new UnsupportedSchemaError(`${path}: only "anyOf: [X, null]" is supported`);
  }
  return `v.nullable(${convertSchema(others[0], `${path}.anyOf`)})`;
}

function convertConstUnion(members: unknown, path: string): string {
  const values = arrayOf(members, `${path}.oneOf`).map((member, index) => {
    const object = asObject(member, `${path}.oneOf[${index}]`);
    const extra = Object.keys(object).filter(
      (key) => key !== 'const' && key !== 'type' && !ANNOTATIONS.includes(key),
    );
    if (typeof object.const !== 'string' || extra.length > 0) {
      throw new UnsupportedSchemaError(`${path}: only "oneOf" of string constants is supported`);
    }
    return object.const;
  });
  return `v.picklist(${JSON.stringify(values)})`;
}

function picklist(values: unknown, path: string): string {
  return `v.picklist(${JSON.stringify(stringArray(values, `${path}.enum`))})`;
}

function literal(value: unknown, path: string): string {
  if (typeof value !== 'string' && typeof value !== 'number' && typeof value !== 'boolean') {
    throw new UnsupportedSchemaError(`${path}: "const" must be a string, number or boolean`);
  }
  return `v.literal(${JSON.stringify(value)})`;
}

/** `$ref` から `$defs` の名前を取り出す。 */
function refName(ref: unknown, path: string): string {
  const match = typeof ref === 'string' ? /^#\/\$defs\/(.+)$/.exec(ref) : null;
  const name = match?.[1];
  if (name === undefined || !DEFINITION_NAME.test(name)) {
    throw new UnsupportedSchemaError(`${path}: unsupported $ref ${JSON.stringify(ref)}`);
  }
  return name;
}

/** 参照される定義が先に来る順序で名前を返す。循環と未知の参照は拒否する。 */
function dependencyOrder(definitions: Record<string, JsonObject>): string[] {
  const order: string[] = [];
  const state = new Map<string, 'visiting' | 'done'>();
  const visit = (name: string, from: string) => {
    const definition = definitions[name];
    if (definition === undefined) {
      throw new UnsupportedSchemaError(`${from}: unknown definition "${name}"`);
    }
    if (state.get(name) === 'done') {
      return;
    }
    if (state.get(name) === 'visiting') {
      throw new UnsupportedSchemaError(`${from}: circular reference to "${name}"`);
    }
    state.set(name, 'visiting');
    for (const ref of collectRefs(definition, name)) {
      visit(ref, name);
    }
    state.set(name, 'done');
    order.push(name);
  };
  for (const name of Object.keys(definitions)) {
    visit(name, name);
  }
  return order;
}

/** スキーマの中の `$ref` をすべて集める。 */
function collectRefs(value: unknown, path: string): string[] {
  if (Array.isArray(value)) {
    return value.flatMap((item) => collectRefs(item, path));
  }
  if (typeof value !== 'object' || value === null) {
    return [];
  }
  return Object.entries(value).flatMap(([key, inner]) =>
    key === '$ref' ? [refName(inner, path)] : collectRefs(inner, path),
  );
}

function pipe(base: string, actions: string[]): string {
  return actions.length === 0 ? base : `v.pipe(${[base, ...actions].join(', ')})`;
}

function asObject(value: unknown, path: string): JsonObject {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) {
    throw new UnsupportedSchemaError(`${path}: expected an object`);
  }
  return value as JsonObject;
}

function arrayOf(value: unknown, path: string): unknown[] {
  if (!Array.isArray(value)) {
    throw new UnsupportedSchemaError(`${path}: expected an array`);
  }
  return value;
}

function stringArray(value: unknown, path: string): string[] {
  const list = arrayOf(value, path);
  if (!list.every((item) => typeof item === 'string')) {
    throw new UnsupportedSchemaError(`${path}: expected an array of strings`);
  }
  return list as string[];
}

function nonNegativeInteger(value: unknown, path: string): number {
  if (typeof value !== 'number' || !Number.isInteger(value) || value < 0) {
    throw new UnsupportedSchemaError(`${path}: expected a non-negative integer`);
  }
  return value;
}
```

`generateValibotModule` の中の `definitions[name]` は、`noUncheckedIndexedAccess` で `JsonObject | undefined` になる。
`dependencyOrder` が返す名前は必ず `definitions` にあるため、`convertSchema` は `unknown` を受けて自分で検査する作りにしてある (型エラーにならない)。

`web/scripts/gen-api.ts`:

```ts
/** src/api/schema.json から src/api/schema.gen.ts を作る。`aube run gen` で実行する。 */

import { readFileSync, writeFileSync } from 'node:fs';
import { generateValibotModule } from './json-schema-to-valibot.ts';

const apiDir = new URL('../src/api/', import.meta.url);
const schema: unknown = JSON.parse(readFileSync(new URL('schema.json', apiDir), 'utf8'));
if (typeof schema !== 'object' || schema === null || Array.isArray(schema)) {
  throw new Error('src/api/schema.json must contain a JSON object');
}
writeFileSync(
  new URL('schema.gen.ts', apiDir),
  generateValibotModule(schema as Record<string, unknown>),
);
```

- [ ] **Step 4: テストを通し、生成する**

Run: `cd web && aube run test`
Expected: PASS (convertSchema 7 件、generateValibotModule 2 件)

Run: `cd web && aube run gen && aube run typecheck && aube run lint`
Expected: `web/src/api/schema.gen.ts` ができ、型チェックと lint が通る。lint が `schema.gen.ts` や `schema.json` を報告するなら、`biome.json` の `files.includes` の否定の書き方を Biome のドキュメントで確かめて直す

`schema.gen.ts` を開き、`HistoryQuerySchema`、`HistoryResponseSchema`、`HistorySummarySchema`、`ErrorResponseSchema` と、それぞれの `type` があることを確かめる。
`UnsupportedSchemaError` が出た場合は、Task 6 の Step 4 で確かめた schemars の出力とこのタスクの対応表が食い違っている。
その形の変換とテストを足してから生成し直す。

- [ ] **Step 5: `gen-api` のレシピを足す**

`Justfile` の `web-check` の後ろに足す。

```make
# Regenerate the viewer API schemas (Rust DTO -> web/src/api/schema.json -> schema.gen.ts)
gen-api:
    UPDATE_API_SCHEMA=1 cargo test -p kgd-presentation viewer::dto
    cd web && aube run gen
```

Run: `~/.nix-profile/bin/just gen-api && git status --short web/src/api`
Expected: 差分が無い (いま作ったものと同じになる)

- [ ] **Step 6: コミットする**

```bash
git add Justfile web/scripts web/src/api/schema.gen.ts
git -c commit.gpgsign=false commit -m "feat: API の JSON Schema から valibot のスキーマを生成する

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 12: 期間の扱いと API の取得

**Files:**
- Create: `web/src/range.ts`、`web/src/range.test.ts`
- Create: `web/src/format.ts`、`web/src/format.test.ts`
- Create: `web/src/api/client.ts`、`web/src/api/client.test.ts`、`web/src/api/fixture.ts`
- Create: `web/src/api/useHistory.ts`

**Interfaces:**
- Consumes: `HistoryQuery`、`HistoryQuerySchema`、`HistoryResponse`、`HistoryResponseSchema`、`ErrorResponseSchema` (Task 11)
- Produces:
  - `range.ts`: `type DateRange = HistoryQuery`、`toIsoDate(date: Date): string`、`fromIsoDate(value: string): Date`、`todayRange(now: Date): DateRange`、`presetRanges(now: Date): RangePreset[]`、`rangeFromSearch(search: string, now: Date): DateRange`、`rangeToSearch(range: DateRange): string`
  - `format.ts`: `formatKm(meters: number): string`、`formatDuration(seconds: number): string`、`formatCount(value: number): string`、`formatDateTime(iso: string | null | undefined, timeZone: string): string`、`formatShortDate(isoDate: string): string`
  - `api/client.ts`: `class ApiError extends Error { status: number }`、`historyUrl(range: DateRange): string`、`fetchHistory(range: DateRange, signal?: AbortSignal): Promise<HistoryResponse>`
  - `api/useHistory.ts`: `useHistory(range: DateRange): { data: HistoryResponse | null; loading: boolean; error: string | null }`
  - `api/fixture.ts`: `export const historyFixture` (テスト用の応答の例)

期間はブラウザの現地の日付で扱う (設計書の「既知の制約」のとおり)。
文言の数字の形 (`12.3 km`、`3 時間 5 分`、`1,234 点`) は日次レポートに揃える。

- [ ] **Step 1: 失敗するテストを書く**

`web/src/range.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import {
  fromIsoDate,
  presetRanges,
  rangeFromSearch,
  rangeToSearch,
  todayRange,
  toIsoDate,
} from './range.ts';

const now = new Date(2026, 8, 30, 10, 0);

describe('toIsoDate / fromIsoDate', () => {
  it('formats the local date with zero padding and parses it back', () => {
    expect(toIsoDate(new Date(2026, 0, 5, 23, 59))).toBe('2026-01-05');
    expect(toIsoDate(fromIsoDate('2026-09-01'))).toBe('2026-09-01');
  });
});

describe('presetRanges', () => {
  it('offers today, yesterday, the last 7 days and this month', () => {
    expect(presetRanges(now)).toEqual([
      { label: '今日', range: { from: '2026-09-30', to: '2026-09-30' } },
      { label: '昨日', range: { from: '2026-09-29', to: '2026-09-29' } },
      { label: '直近 7 日', range: { from: '2026-09-24', to: '2026-09-30' } },
      { label: '今月', range: { from: '2026-09-01', to: '2026-09-30' } },
    ]);
  });
});

describe('rangeFromSearch', () => {
  it('reads from and to from the query string', () => {
    expect(rangeFromSearch('?from=2026-09-01&to=2026-09-10', now)).toEqual({
      from: '2026-09-01',
      to: '2026-09-10',
    });
  });

  it('falls back to today for missing, malformed or reversed ranges', () => {
    const today = todayRange(now);
    expect(rangeFromSearch('', now)).toEqual(today);
    expect(rangeFromSearch('?from=2026-9-1&to=2026-09-10', now)).toEqual(today);
    expect(rangeFromSearch('?from=2026-09-10&to=2026-09-01', now)).toEqual(today);
  });

  it('round-trips through rangeToSearch', () => {
    const range = { from: '2026-09-01', to: '2026-09-10' };
    expect(rangeFromSearch(rangeToSearch(range), now)).toEqual(range);
  });
});
```

`web/src/format.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import { formatCount, formatDateTime, formatDuration, formatKm, formatShortDate } from './format.ts';

describe('format', () => {
  it('formats distances like the daily report', () => {
    expect(formatKm(12345)).toBe('12.3 km');
    expect(formatKm(0)).toBe('0.0 km');
  });

  it('formats durations like the daily report', () => {
    expect(formatDuration(59)).toBe('0 分');
    expect(formatDuration(5 * 60)).toBe('5 分');
    expect(formatDuration(3 * 3600 + 5 * 60 + 30)).toBe('3 時間 5 分');
  });

  it('groups thousands', () => {
    expect(formatCount(1234567)).toBe('1,234,567');
  });

  it('shows date and time in the given time zone', () => {
    expect(formatDateTime('2026-09-01T00:03:12Z', 'Asia/Tokyo')).toBe('9/1 09:03');
    expect(formatDateTime(null, 'Asia/Tokyo')).toBe('-');
  });

  it('shortens ISO dates for chart labels', () => {
    expect(formatShortDate('2026-09-05')).toBe('9/5');
  });
});
```

`web/src/api/fixture.ts`:

```ts
/** API の応答の例。Rust の DTO が出す JSON と同じ形にする (テストで使う)。 */
export const historyFixture = {
  range: { from: '2026-09-01', to: '2026-09-01', timezone: 'Asia/Tokyo' },
  total: {
    distance_m: 1500,
    distance_by_activity: { walking: 500, cycling: 0, automotive: 1000, unknown: 0 },
    moving_s: 1200,
    stationary_s: 90,
    point_count: 3,
    excluded_count: 1,
    first_at: '2026-09-01T00:03:12Z',
    last_at: '2026-09-01T09:30:00Z',
  },
  days: [
    {
      date: '2026-09-01',
      summary: {
        distance_m: 1500,
        distance_by_activity: { walking: 500, cycling: 0, automotive: 1000, unknown: 0 },
        moving_s: 1200,
        stationary_s: 90,
        point_count: 3,
        excluded_count: 1,
        first_at: '2026-09-01T00:03:12Z',
        last_at: '2026-09-01T09:30:00Z',
      },
    },
  ],
  track: {
    type: 'FeatureCollection',
    features: [
      {
        type: 'Feature',
        properties: { activity: 'walking' },
        geometry: {
          type: 'LineString',
          coordinates: [
            [139.7, 35.6],
            [139.71, 35.61],
          ],
        },
      },
    ],
  },
  track_meta: { original_points: 3, returned_points: 3, simplified: false },
};
```

`web/src/api/client.test.ts`:

```ts
import { afterEach, describe, expect, it, vi } from 'vitest';
import { ApiError, fetchHistory, historyUrl } from './client.ts';
import { historyFixture } from './fixture.ts';

const range = { from: '2026-09-01', to: '2026-09-01' };

function respondWith(body: unknown, status = 200) {
  vi.stubGlobal(
    'fetch',
    vi.fn(async () => new Response(JSON.stringify(body), { status })),
  );
}

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('fetchHistory', () => {
  it('requests the history API under the viewer base path', () => {
    expect(historyUrl(range)).toBe('/viewer/api/history?from=2026-09-01&to=2026-09-01');
  });

  it('returns a response that matches the generated schema', async () => {
    respondWith(historyFixture);

    const history = await fetchHistory(range);

    expect(history.total.point_count).toBe(3);
    expect(history.track.features[0]?.properties.activity).toBe('walking');
  });

  it('throws ApiError with the server message on error responses', async () => {
    respondWith({ error: 'from must not be after to' }, 400);

    await expect(fetchHistory(range)).rejects.toEqual(
      expect.objectContaining({ name: 'ApiError', status: 400, message: 'from must not be after to' }),
    );
  });

  it('rejects responses that do not match the schema', async () => {
    const { track_meta: _, ...broken } = historyFixture;
    respondWith(broken);

    await expect(fetchHistory(range)).rejects.not.toBeInstanceOf(ApiError);
    await expect(fetchHistory(range)).rejects.toThrow();
  });

  it('rejects when the request is aborted', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(
        (_url: string, init?: RequestInit) =>
          new Promise((_resolve, reject) => {
            init?.signal?.addEventListener('abort', () =>
              reject(new DOMException('aborted', 'AbortError')),
            );
          }),
      ),
    );
    const controller = new AbortController();

    const pending = fetchHistory(range, controller.signal);
    controller.abort();

    await expect(pending).rejects.toMatchObject({ name: 'AbortError' });
  });
});
```

- [ ] **Step 2: テストが失敗することを確かめる**

Run: `cd web && aube run test`
Expected: FAIL (`Failed to resolve import "./range.ts"` など)

- [ ] **Step 3: 実装する**

`web/src/range.ts`:

```ts
import * as v from 'valibot';
import { type HistoryQuery, HistoryQuerySchema } from './api/schema.gen.ts';

/** 選んだ期間。開始日と終了日 (どちらも含む) を `YYYY-MM-DD` で持つ。 */
export type DateRange = HistoryQuery;

/** よく使う範囲のボタン。 */
export type RangePreset = { label: string; range: DateRange };

/** ブラウザの現地の日付を `YYYY-MM-DD` にする。 */
export function toIsoDate(date: Date): string {
  const year = String(date.getFullYear()).padStart(4, '0');
  const month = String(date.getMonth() + 1).padStart(2, '0');
  const day = String(date.getDate()).padStart(2, '0');
  return `${year}-${month}-${day}`;
}

/** `YYYY-MM-DD` を、ブラウザの現地の 0 時の Date にする。 */
export function fromIsoDate(value: string): Date {
  const [year = 1970, month = 1, day = 1] = value.split('-').map(Number);
  return new Date(year, month - 1, day);
}

function addDays(date: Date, days: number): Date {
  return new Date(date.getFullYear(), date.getMonth(), date.getDate() + days);
}

/** 今日だけの期間。 */
export function todayRange(now: Date): DateRange {
  const today = toIsoDate(now);
  return { from: today, to: today };
}

/** よく使う範囲 (今日、昨日、直近 7 日、今月)。 */
export function presetRanges(now: Date): RangePreset[] {
  const today = toIsoDate(now);
  const yesterday = toIsoDate(addDays(now, -1));
  return [
    { label: '今日', range: { from: today, to: today } },
    { label: '昨日', range: { from: yesterday, to: yesterday } },
    { label: '直近 7 日', range: { from: toIsoDate(addDays(now, -6)), to: today } },
    {
      label: '今月',
      range: { from: toIsoDate(new Date(now.getFullYear(), now.getMonth(), 1)), to: today },
    },
  ];
}

/** URL のクエリから期間を読む。無い、形式が違う、前後が逆のときは今日にする。 */
export function rangeFromSearch(search: string, now: Date): DateRange {
  const params = new URLSearchParams(search);
  const parsed = v.safeParse(HistoryQuerySchema, {
    from: params.get('from'),
    to: params.get('to'),
  });
  if (!parsed.success || parsed.output.from > parsed.output.to) {
    return todayRange(now);
  }
  return parsed.output;
}

/** 期間を URL のクエリにする。 */
export function rangeToSearch(range: DateRange): string {
  return `?${new URLSearchParams({ from: range.from, to: range.to })}`;
}
```

`web/src/format.ts`:

```ts
/** メートルを小数 1 桁のキロメートル表記にする (日次レポートと同じ形)。 */
export function formatKm(meters: number): string {
  return `${(meters / 1000).toFixed(1)} km`;
}

/** 秒を「N 時間 M 分」または「M 分」にする (日次レポートと同じ形)。 */
export function formatDuration(seconds: number): string {
  const minutes = Math.floor(seconds / 60);
  const hours = Math.floor(minutes / 60);
  return hours > 0 ? `${hours} 時間 ${minutes % 60} 分` : `${minutes} 分`;
}

/** 3 桁ごとにカンマで区切る。 */
export function formatCount(value: number): string {
  return value.toLocaleString('en-US');
}

/** ISO 8601 の時刻を、指定したタイムゾーンの「M/D HH:MM」にする。無ければ「-」。 */
export function formatDateTime(iso: string | null | undefined, timeZone: string): string {
  if (!iso) {
    return '-';
  }
  const parts = new Intl.DateTimeFormat('en-US', {
    timeZone,
    month: 'numeric',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
    hourCycle: 'h23',
  }).formatToParts(new Date(iso));
  const part = (type: Intl.DateTimeFormatPartTypes) =>
    parts.find((item) => item.type === type)?.value ?? '';
  return `${part('month')}/${part('day')} ${part('hour')}:${part('minute')}`;
}

/** `YYYY-MM-DD` をグラフの目盛り用の「M/D」にする。 */
export function formatShortDate(isoDate: string): string {
  const [, month = '0', day = '0'] = isoDate.split('-');
  return `${Number(month)}/${Number(day)}`;
}
```

`web/src/api/client.ts`:

```ts
import * as v from 'valibot';
import type { DateRange } from '../range.ts';
import { ErrorResponseSchema, type HistoryResponse, HistoryResponseSchema } from './schema.gen.ts';

/** API がエラーの応答を返したことを表す。 */
export class ApiError extends Error {
  readonly status: number;

  constructor(message: string, status: number) {
    super(message);
    this.name = 'ApiError';
    this.status = status;
  }
}

/** 期間の位置ログを返す API の URL。 */
export function historyUrl(range: DateRange): string {
  const query = new URLSearchParams({ from: range.from, to: range.to });
  return `${import.meta.env.BASE_URL}api/history?${query}`;
}

/**
 * 期間の位置ログを取得する。応答は生成したスキーマで検証してから返す。
 *
 * エラーの応答なら ApiError を、スキーマに合わない応答なら valibot の ValiError を投げる。
 */
export async function fetchHistory(
  range: DateRange,
  signal?: AbortSignal,
): Promise<HistoryResponse> {
  const response = await fetch(historyUrl(range), { signal });
  const body: unknown = await response.json().catch(() => null);
  if (!response.ok) {
    const error = v.safeParse(ErrorResponseSchema, body);
    throw new ApiError(error.success ? error.output.error : `HTTP ${response.status}`, response.status);
  }
  return v.parse(HistoryResponseSchema, body);
}
```

`web/src/api/useHistory.ts`:

```ts
import { useEffect, useState } from 'react';
import type { DateRange } from '../range.ts';
import { fetchHistory } from './client.ts';
import type { HistoryResponse } from './schema.gen.ts';

/** 期間の位置ログの取得状態。読み込み中も直前の結果を残し、地図のちらつきを抑える。 */
export type HistoryState = {
  data: HistoryResponse | null;
  loading: boolean;
  error: string | null;
};

/**
 * 期間が変わるたびに位置ログを取得する。
 *
 * 期間を変えたら前のリクエストを取り消し、古い応答が新しい期間の表示を上書きしないようにする。
 */
export function useHistory(range: DateRange): HistoryState {
  const [state, setState] = useState<HistoryState>({ data: null, loading: true, error: null });
  const { from, to } = range;

  useEffect(() => {
    const controller = new AbortController();
    setState((previous) => ({ ...previous, loading: true, error: null }));
    fetchHistory({ from, to }, controller.signal).then(
      (data) => setState({ data, loading: false, error: null }),
      (error: unknown) => {
        if (controller.signal.aborted) {
          return;
        }
        console.error('Failed to load location history', error);
        const message = error instanceof Error ? error.message : String(error);
        setState((previous) => ({ ...previous, loading: false, error: message }));
      },
    );
    return () => controller.abort();
  }, [from, to]);

  return state;
}
```

- [ ] **Step 4: テストを通す**

Run: `cd web && aube run test && aube run typecheck && aube run lint`
Expected: PASS (range 5 件、format 5 件、client 5 件、Task 11 の 9 件)

`client.test.ts` の「returns a response that matches the generated schema」が失敗したら、`fixture.ts` と Rust の DTO の JSON の形が食い違っている。
Task 7 の `history_returns_json_to_lan_peers` を `--nocapture` で動かして実際の JSON を出し、どちらが正しいかを確かめてから直す。

- [ ] **Step 5: コミットする**

```bash
git add web/src
git -c commit.gpgsign=false commit -m "feat: ビューアの期間の扱いと API の取得を追加する

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 13: 軌跡の地図

**Files:**
- Create: `web/src/activity.ts`、`web/src/activity.test.ts`
- Create: `web/src/geo.ts`、`web/src/geo.test.ts`
- Create: `web/src/components/TrackMap.tsx`、`web/src/components/TrackMap.module.css`

**Interfaces:**
- Consumes: `HistoryResponse` (Task 11)、`formatCount` (Task 12)
- Produces:
  - `activity.ts`: `type ActivityKind`、`ACTIVITY_COLORS: Record<ActivityKind, string>`、`ACTIVITY_LABELS: Record<ActivityKind, string>`、`DISTANCE_ACTIVITIES: readonly ['walking', 'cycling', 'automotive', 'unknown']`、`lineColorExpression(): ExpressionSpecification`
  - `geo.ts`: `type Bounds = [[number, number], [number, number]]`、`trackBounds(track: HistoryResponse['track']): Bounds | null`
  - `components/TrackMap.tsx`: `TrackMap(props: { track: HistoryResponse['track']; meta: HistoryResponse['track_meta'] })`

色と表示名は日次レポート (`segment_rgb` と `activity_label`) に揃える。
MapLibre GL JS 6 を Vite で使うには、Web Worker の URL を先に設定する必要がある (`@vis.gl/react-maplibre` 8.1 の例のとおり)。
OpenFreeMap の liberty スタイルは、日本の地名をローマ字と日本語の併記 (例: 「Tōkyō 東京」) で表示する。
これは既定のままにする。

- [ ] **Step 1: 失敗するテストを書く**

`web/src/activity.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import { ACTIVITY_COLORS, ACTIVITY_LABELS, lineColorExpression } from './activity.ts';

describe('activity', () => {
  it('uses the same colors and labels as the daily report', () => {
    expect(ACTIVITY_COLORS).toEqual({
      walking: '#2e9e44',
      cycling: '#f08c1a',
      automotive: '#1f6fd1',
      stationary: '#c0392b',
      unknown: '#808080',
    });
    expect(ACTIVITY_LABELS).toEqual({
      walking: '徒歩',
      cycling: '自転車',
      automotive: '車',
      stationary: '静止',
      unknown: '不明',
    });
  });

  it('builds a match expression over every activity with gray as the fallback', () => {
    expect(lineColorExpression()).toEqual([
      'match',
      ['get', 'activity'],
      'walking',
      '#2e9e44',
      'cycling',
      '#f08c1a',
      'automotive',
      '#1f6fd1',
      'stationary',
      '#c0392b',
      'unknown',
      '#808080',
      '#808080',
    ]);
  });
});
```

`web/src/geo.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import { trackBounds } from './geo.ts';

function line(coordinates: [number, number][]) {
  return {
    type: 'Feature' as const,
    properties: { activity: 'walking' as const },
    geometry: { type: 'LineString' as const, coordinates },
  };
}

describe('trackBounds', () => {
  it('returns the south-west and north-east corners of every line', () => {
    const track = {
      type: 'FeatureCollection' as const,
      features: [
        line([
          [139.7, 35.6],
          [139.8, 35.7],
        ]),
        line([[139.5, 35.9]]),
      ],
    };

    expect(trackBounds(track)).toEqual([
      [139.5, 35.6],
      [139.8, 35.9],
    ]);
  });

  it('returns null for an empty track', () => {
    expect(trackBounds({ type: 'FeatureCollection', features: [] })).toBeNull();
  });
});
```

`geo.test.ts` の `track` の型が `HistoryResponse['track']` と合わず型チェックで落ちる場合 (`coordinates` が `v.tuple` の型になった場合など) は、`line` の引数の型を `HistoryResponse['track']['features'][number]['geometry']['coordinates']` に替える。

- [ ] **Step 2: テストが失敗することを確かめる**

Run: `cd web && aube run test`
Expected: FAIL (`Failed to resolve import "./activity.ts"` など)

- [ ] **Step 3: 実装する**

`web/src/activity.ts`:

```ts
import type { ExpressionSpecification } from 'maplibre-gl';
import type { HistoryResponse } from './api/schema.gen.ts';

/** 移動種別。 */
export type ActivityKind = HistoryResponse['track']['features'][number]['properties']['activity'];

/** 線の色。日次レポートの地図画像と揃える。 */
export const ACTIVITY_COLORS: Record<ActivityKind, string> = {
  walking: '#2e9e44',
  cycling: '#f08c1a',
  automotive: '#1f6fd1',
  stationary: '#c0392b',
  unknown: '#808080',
};

/** 表示名。日次レポートの文言と揃える。 */
export const ACTIVITY_LABELS: Record<ActivityKind, string> = {
  walking: '徒歩',
  cycling: '自転車',
  automotive: '車',
  stationary: '静止',
  unknown: '不明',
};

/** 距離を持つ移動種別 (静止は距離を積算しない)。グラフの積み上げの順序にも使う。 */
export const DISTANCE_ACTIVITIES = ['walking', 'cycling', 'automotive', 'unknown'] as const;

/** 区間の `activity` から線の色を選ぶ MapLibre の式。 */
export function lineColorExpression(): ExpressionSpecification {
  const pairs = (Object.keys(ACTIVITY_COLORS) as ActivityKind[]).flatMap((activity) => [
    activity,
    ACTIVITY_COLORS[activity],
  ]);
  return ['match', ['get', 'activity'], ...pairs, ACTIVITY_COLORS.unknown] as ExpressionSpecification;
}
```

`maplibre-gl` から `ExpressionSpecification` を import できない場合は、`@maplibre/maplibre-gl-style-spec` から import する (maplibre-gl の依存に含まれる)。
それも解決できなければ `aube add @maplibre/maplibre-gl-style-spec` で直接の依存にする。

`web/src/geo.ts`:

```ts
import type { HistoryResponse } from './api/schema.gen.ts';

/** 南西の角と北東の角 (`[経度, 緯度]`)。MapLibre の fitBounds に渡す形。 */
export type Bounds = [[number, number], [number, number]];

/** 軌跡の全体が収まる範囲を返す。点が無ければ null。 */
export function trackBounds(track: HistoryResponse['track']): Bounds | null {
  let west = Number.POSITIVE_INFINITY;
  let south = Number.POSITIVE_INFINITY;
  let east = Number.NEGATIVE_INFINITY;
  let north = Number.NEGATIVE_INFINITY;
  for (const feature of track.features) {
    for (const position of feature.geometry.coordinates) {
      const lon = position[0];
      const lat = position[1];
      if (lon === undefined || lat === undefined) {
        continue;
      }
      west = Math.min(west, lon);
      east = Math.max(east, lon);
      south = Math.min(south, lat);
      north = Math.max(north, lat);
    }
  }
  if (!Number.isFinite(west)) {
    return null;
  }
  return [
    [west, south],
    [east, north],
  ];
}
```

`web/src/components/TrackMap.module.css`:

```css
.container {
  position: relative;
  width: 100%;
  height: 100%;
}

.badge,
.empty {
  position: absolute;
  left: 50%;
  transform: translateX(-50%);
  padding: 0.25rem 0.75rem;
  border-radius: 999px;
  background: rgb(255 255 255 / 90%);
  box-shadow: 0 1px 4px rgb(0 0 0 / 20%);
  font-size: 0.85rem;
}

.badge {
  top: 0.75rem;
}

.empty {
  top: 50%;
}
```

`web/src/components/TrackMap.tsx`:

```tsx
import { Layer, Map as MapView, type MapRef, Source } from '@vis.gl/react-maplibre';
import type { FeatureCollection } from 'geojson';
import { setWorkerUrl } from 'maplibre-gl';
import 'maplibre-gl/dist/maplibre-gl.css';
import workerUrl from 'maplibre-gl/dist/maplibre-gl-worker.mjs?worker&url';
import { useCallback, useEffect, useMemo, useRef } from 'react';
import { lineColorExpression } from '../activity.ts';
import type { HistoryResponse } from '../api/schema.gen.ts';
import { formatCount } from '../format.ts';
import { trackBounds } from '../geo.ts';
import styles from './TrackMap.module.css';

// MapLibre GL JS 6 は Vite でビルドするとき、Worker の URL を明示する必要がある
setWorkerUrl(workerUrl);

/** OpenFreeMap のベクタータイルのスタイル。登録も API キーも要らない。 */
const STYLE_URL = 'https://tiles.openfreemap.org/styles/liberty';

/** 軌跡が無いときの初期表示 (東京駅)。 */
const INITIAL_VIEW = { longitude: 139.767, latitude: 35.681, zoom: 10 };

type Props = {
  /** 地図に描く軌跡 */
  track: HistoryResponse['track'];
  /** 間引きの情報 */
  meta: HistoryResponse['track_meta'];
};

/** 期間の軌跡を移動種別で色分けして描く地図。 */
export function TrackMap({ track, meta }: Props) {
  const mapRef = useRef<MapRef>(null);
  const bounds = useMemo(() => trackBounds(track), [track]);
  const colors = useMemo(() => lineColorExpression(), []);

  const fit = useCallback(() => {
    if (bounds) {
      mapRef.current?.fitBounds(bounds, { padding: 40, duration: 0, maxZoom: 16 });
    }
  }, [bounds]);

  useEffect(fit, [fit]);

  return (
    <div className={styles.container}>
      <MapView
        ref={mapRef}
        initialViewState={INITIAL_VIEW}
        mapStyle={STYLE_URL}
        onLoad={fit}
        style={{ width: '100%', height: '100%' }}
      >
        <Source id="track" type="geojson" data={track as FeatureCollection}>
          <Layer
            id="track-line"
            type="line"
            layout={{ 'line-join': 'round', 'line-cap': 'round' }}
            paint={{ 'line-color': colors, 'line-width': 3 }}
          />
        </Source>
      </MapView>
      {meta.simplified && (
        <div className={styles.badge}>
          {formatCount(meta.returned_points)} / {formatCount(meta.original_points)}{' '}
          点に間引いて表示中
        </div>
      )}
      {track.features.length === 0 && <div className={styles.empty}>この期間の記録はありません</div>}
    </div>
  );
}
```

`track as FeatureCollection` は、生成した型 (リテラルの `type` と `[number, number]` の座標) と `geojson` の型が構造上一致しない場合のために置いている。
`as unknown as FeatureCollection` でないと型チェックが通らない場合は、そう書き換える。

- [ ] **Step 4: テストと型チェックを通す**

Run: `cd web && aube run test && aube run typecheck && aube run lint && aube run build`
Expected: PASS (activity 2 件、geo 2 件を含む)。ビルドも通る

- [ ] **Step 5: コミットする**

```bash
git add web/src
git -c commit.gpgsign=false commit -m "feat: ビューアに移動種別で色分けした軌跡の地図を追加する

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 14: 期間の合計と日ごとのグラフ

**Files:**
- Create: `web/src/charts.ts`、`web/src/charts.test.ts`
- Create: `web/src/components/SummaryPanel.tsx`、`web/src/components/SummaryPanel.module.css`
- Create: `web/src/components/DailyCharts.tsx`、`web/src/components/DailyCharts.module.css`

**Interfaces:**
- Consumes: `HistoryResponse`、`HistorySummary` (Task 11)、`formatKm`、`formatDuration`、`formatCount`、`formatDateTime`、`formatShortDate` (Task 12)、`ACTIVITY_COLORS`、`ACTIVITY_LABELS`、`DISTANCE_ACTIVITIES` (Task 13)
- Produces:
  - `charts.ts`: `type ChartRow`、`toChartRows(days: HistoryResponse['days']): ChartRow[]`
  - `components/SummaryPanel.tsx`: `SummaryPanel(props: { summary: HistorySummary; timezone: string })`
  - `components/DailyCharts.tsx`: `DailyCharts(props: { days: HistoryResponse['days']; onSelectDay: (date: string) => void })`

- [ ] **Step 1: 失敗するテストを書く**

`web/src/charts.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import { historyFixture } from './api/fixture.ts';
import { toChartRows } from './charts.ts';

describe('toChartRows', () => {
  it('converts daily summaries into chart rows in km, hours and counts', () => {
    expect(toChartRows(historyFixture.days)).toEqual([
      {
        date: '2026-09-01',
        label: '9/1',
        walking: 0.5,
        cycling: 0,
        automotive: 1,
        unknown: 0,
        movingHours: 0.3,
        stationaryHours: 0,
        points: 3,
        excluded: 1,
      },
    ]);
  });
});
```

`historyFixture.days` を `HistoryResponse['days']` として渡せず型チェックで落ちる場合は、`v.parse(HistoryResponseSchema, historyFixture).days` を渡す。

- [ ] **Step 2: テストが失敗することを確かめる**

Run: `cd web && aube run test`
Expected: FAIL (`Failed to resolve import "./charts.ts"`)

- [ ] **Step 3: 実装する**

`web/src/charts.ts`:

```ts
import type { HistoryResponse } from './api/schema.gen.ts';
import { formatShortDate } from './format.ts';

/** 日ごとのグラフの 1 行。距離は km、時間は時間で持つ (どちらも小数 1 桁)。 */
export type ChartRow = {
  date: string;
  label: string;
  walking: number;
  cycling: number;
  automotive: number;
  unknown: number;
  movingHours: number;
  stationaryHours: number;
  points: number;
  excluded: number;
};

const toKm = (meters: number) => Math.round(meters / 100) / 10;
const toHours = (seconds: number) => Math.round(seconds / 360) / 10;

/** 日ごとの集計をグラフの行にする。 */
export function toChartRows(days: HistoryResponse['days']): ChartRow[] {
  return days.map(({ date, summary }) => ({
    date,
    label: formatShortDate(date),
    walking: toKm(summary.distance_by_activity.walking),
    cycling: toKm(summary.distance_by_activity.cycling),
    automotive: toKm(summary.distance_by_activity.automotive),
    unknown: toKm(summary.distance_by_activity.unknown),
    movingHours: toHours(summary.moving_s),
    stationaryHours: toHours(summary.stationary_s),
    points: summary.point_count,
    excluded: summary.excluded_count,
  }));
}
```

`web/src/components/SummaryPanel.module.css`:

```css
.summary {
  display: grid;
  grid-template-columns: max-content 1fr;
  gap: 0.25rem 1rem;
  margin: 0 0 1rem;
}

.summary dt {
  color: #6c757d;
}

.summary dd {
  margin: 0;
}
```

`web/src/components/SummaryPanel.tsx`:

```tsx
import { ACTIVITY_LABELS, DISTANCE_ACTIVITIES } from '../activity.ts';
import type { HistorySummary } from '../api/schema.gen.ts';
import { formatCount, formatDateTime, formatDuration, formatKm } from '../format.ts';
import styles from './SummaryPanel.module.css';

type Props = {
  /** 期間全体の集計 */
  summary: HistorySummary;
  /** 時刻を表示するタイムゾーン */
  timezone: string;
};

/** 期間全体の合計。文言は日次レポートに揃える。 */
export function SummaryPanel({ summary, timezone }: Props) {
  const breakdown = DISTANCE_ACTIVITIES.filter(
    (activity) => summary.distance_by_activity[activity] > 0,
  )
    .map(
      (activity) =>
        `${ACTIVITY_LABELS[activity]} ${formatKm(summary.distance_by_activity[activity])}`,
    )
    .join(' / ');

  return (
    <dl className={styles.summary}>
      <dt>移動距離</dt>
      <dd>
        {formatKm(summary.distance_m)}
        {breakdown && ` (${breakdown})`}
      </dd>
      <dt>移動 / 静止</dt>
      <dd>
        {formatDuration(summary.moving_s)} / {formatDuration(summary.stationary_s)}
      </dd>
      <dt>記録</dt>
      <dd>
        {formatCount(summary.point_count)} 点
        {summary.excluded_count > 0 &&
          ` (精度不足で ${formatCount(summary.excluded_count)} 点を除外)`}
      </dd>
      <dt>最初 / 最後</dt>
      <dd>
        {formatDateTime(summary.first_at, timezone)} / {formatDateTime(summary.last_at, timezone)}
      </dd>
    </dl>
  );
}
```

`web/src/components/DailyCharts.module.css`:

```css
.charts {
  display: grid;
  gap: 1rem;
}

.charts h3 {
  margin: 0 0 0.25rem;
  font-size: 0.9rem;
}

.hint {
  margin: 0;
  color: #6c757d;
  font-size: 0.8rem;
}
```

`web/src/components/DailyCharts.tsx`:

```tsx
import type { ReactNode } from 'react';
import {
  Bar,
  BarChart,
  CartesianGrid,
  Legend,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from 'recharts';
import { ACTIVITY_COLORS, ACTIVITY_LABELS, DISTANCE_ACTIVITIES } from '../activity.ts';
import type { HistoryResponse } from '../api/schema.gen.ts';
import { type ChartRow, toChartRows } from '../charts.ts';
import styles from './DailyCharts.module.css';

type Props = {
  /** 日ごとの集計 */
  days: HistoryResponse['days'];
  /** 棒をクリックした日 (`YYYY-MM-DD`) を受け取る */
  onSelectDay: (date: string) => void;
};

/** 日ごとの移動距離、移動と静止の時間、記録点数のグラフ。 */
export function DailyCharts({ days, onSelectDay }: Props) {
  const rows = toChartRows(days);
  const select = (item: { payload?: ChartRow }) => {
    if (item.payload) {
      onSelectDay(item.payload.date);
    }
  };

  return (
    <div className={styles.charts}>
      <p className={styles.hint}>棒をクリックすると、その日だけを表示します</p>
      <Chart title="移動距離 (km)" rows={rows}>
        {DISTANCE_ACTIVITIES.map((activity) => (
          <Bar
            key={activity}
            dataKey={activity}
            name={ACTIVITY_LABELS[activity]}
            stackId="distance"
            fill={ACTIVITY_COLORS[activity]}
            cursor="pointer"
            onClick={select}
          />
        ))}
      </Chart>
      <Chart title="移動と静止 (時間)" rows={rows}>
        <Bar
          dataKey="movingHours"
          name="移動"
          stackId="time"
          fill="#4c6ef5"
          cursor="pointer"
          onClick={select}
        />
        <Bar
          dataKey="stationaryHours"
          name="静止"
          stackId="time"
          fill={ACTIVITY_COLORS.stationary}
          cursor="pointer"
          onClick={select}
        />
      </Chart>
      <Chart title="記録点数" rows={rows}>
        <Bar
          dataKey="points"
          name="記録"
          stackId="points"
          fill="#495057"
          cursor="pointer"
          onClick={select}
        />
        <Bar
          dataKey="excluded"
          name="除外"
          stackId="points"
          fill="#adb5bd"
          cursor="pointer"
          onClick={select}
        />
      </Chart>
    </div>
  );
}

/** 1 つの棒グラフ。 */
function Chart({ title, rows, children }: { title: string; rows: ChartRow[]; children: ReactNode }) {
  return (
    <section>
      <h3>{title}</h3>
      <ResponsiveContainer width="100%" height={180}>
        <BarChart data={rows}>
          <CartesianGrid strokeDasharray="3 3" />
          <XAxis dataKey="label" />
          <YAxis />
          <Tooltip />
          <Legend />
          {children}
        </BarChart>
      </ResponsiveContainer>
    </section>
  );
}
```

Recharts 3 の `Bar` の `onClick` の型に `select` が合わず型チェックで落ちる場合は、引数を `(item: unknown)` にし、`(item as { payload?: ChartRow }).payload` で取り出す。

- [ ] **Step 4: テストと型チェックを通す**

Run: `cd web && aube run test && aube run typecheck && aube run lint`
Expected: PASS (charts 1 件を含む)

- [ ] **Step 5: コミットする**

```bash
git add web/src
git -c commit.gpgsign=false commit -m "feat: ビューアに期間の合計と日ごとのグラフを追加する

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 15: 期間の選択と画面の組み立て

**Files:**
- Create: `web/src/components/RangePicker.tsx`、`web/src/components/RangePicker.module.css`
- Modify: `web/src/App.tsx`
- Create: `web/src/App.module.css`

**Interfaces:**
- Consumes: Task 12 から 14 のすべて
- Produces: `App()`、`RangePicker(props: { range: DateRange; today: Date; onChange: (range: DateRange) => void })`

カレンダーでの選択は、「この期間を表示」ボタンで確定する。
react-day-picker の範囲選択は 1 回目のクリックで開始日と終了日を同じ日にすることがあり、選択の完了を自動で判定すると 2 日以上を選べなくなるおそれがあるためである。

- [ ] **Step 1: 期間の選択を実装する**

`web/src/components/RangePicker.module.css`:

```css
.picker {
  position: relative;
  display: flex;
  flex-wrap: wrap;
  gap: 0.5rem;
  align-items: center;
}

.picker button {
  padding: 0.25rem 0.75rem;
  border: 1px solid #ced4da;
  border-radius: 4px;
  background: #fff;
  cursor: pointer;
}

.picker button[aria-pressed='true'] {
  border-color: #1f6fd1;
  background: #e7f0fb;
}

.popover {
  position: absolute;
  top: calc(100% + 0.25rem);
  left: 0;
  z-index: 10;
  padding: 0.5rem;
  border: 1px solid #ced4da;
  border-radius: 4px;
  background: #fff;
  box-shadow: 0 2px 8px rgb(0 0 0 / 15%);
}
```

`web/src/components/RangePicker.tsx`:

```tsx
import { DayPicker, type DateRange as PickerRange } from '@daypicker/react';
import { ja } from '@daypicker/react/locale';
import '@daypicker/react/style.css';
import { useState } from 'react';
import { type DateRange, fromIsoDate, presetRanges, toIsoDate } from '../range.ts';
import styles from './RangePicker.module.css';

type Props = {
  /** 選んでいる期間 */
  range: DateRange;
  /** 今日 (これより後の日は選べない) */
  today: Date;
  /** 期間を変えたときに呼ぶ */
  onChange: (range: DateRange) => void;
};

/** よく使う範囲のボタンと、カレンダーでの範囲選択。 */
export function RangePicker({ range, today, onChange }: Props) {
  const [draft, setDraft] = useState<PickerRange | undefined>(undefined);
  const open = draft !== undefined;

  const toggle = () =>
    setDraft(open ? undefined : { from: fromIsoDate(range.from), to: fromIsoDate(range.to) });

  const apply = () => {
    if (!draft?.from) {
      return;
    }
    onChange({ from: toIsoDate(draft.from), to: toIsoDate(draft.to ?? draft.from) });
    setDraft(undefined);
  };

  return (
    <div className={styles.picker}>
      {presetRanges(today).map((preset) => (
        <button
          key={preset.label}
          type="button"
          aria-pressed={preset.range.from === range.from && preset.range.to === range.to}
          onClick={() => onChange(preset.range)}
        >
          {preset.label}
        </button>
      ))}
      <button type="button" aria-expanded={open} onClick={toggle}>
        {range.from === range.to ? range.from : `${range.from} 〜 ${range.to}`}
      </button>
      {open && (
        <div className={styles.popover}>
          <DayPicker
            mode="range"
            locale={ja}
            selected={draft}
            onSelect={setDraft}
            defaultMonth={draft.from}
            disabled={{ after: today }}
          />
          <button type="button" disabled={!draft.from} onClick={apply}>
            この期間を表示
          </button>
        </div>
      )}
    </div>
  );
}
```

`@daypicker/react/locale` が解決できない場合は、`web/node_modules/@daypicker/react/package.json` の `exports` を見て日本語ロケールの import パスを確かめる (`@daypicker/react/locale/ja` の可能性がある)。
`onSelect={setDraft}` の型が合わない場合は `onSelect={(next) => setDraft(next ?? { from: undefined })}` にする (選択を解除してもカレンダーを閉じないため)。

- [ ] **Step 2: 画面を組み立てる**

`web/src/App.module.css`:

```css
.app {
  display: flex;
  flex-direction: column;
  height: 100dvh;
}

.header {
  display: flex;
  flex-wrap: wrap;
  gap: 0.5rem 1rem;
  align-items: center;
  padding: 0.5rem 1rem;
  border-bottom: 1px solid #dee2e6;
}

.header h1 {
  margin: 0;
  font-size: 1.1rem;
}

.error {
  margin: 0;
  padding: 0.5rem 1rem;
  background: #fdecea;
  color: #c0392b;
}

.main {
  flex: 1;
  display: grid;
  grid-template-columns: minmax(0, 1fr) 380px;
  min-height: 0;
}

.map {
  position: relative;
  min-height: 0;
}

.status {
  display: grid;
  place-items: center;
  height: 100%;
  color: #6c757d;
}

.side {
  overflow-y: auto;
  padding: 1rem;
  border-left: 1px solid #dee2e6;
}

@media (max-width: 900px) {
  .app {
    height: auto;
  }

  .main {
    grid-template-columns: 1fr;
  }

  .map {
    height: 60vh;
  }

  .side {
    border-left: none;
    border-top: 1px solid #dee2e6;
  }
}
```

`web/src/App.tsx` を置き換える。

```tsx
import { useEffect, useState } from 'react';
import styles from './App.module.css';
import { useHistory } from './api/useHistory.ts';
import { DailyCharts } from './components/DailyCharts.tsx';
import { RangePicker } from './components/RangePicker.tsx';
import { SummaryPanel } from './components/SummaryPanel.tsx';
import { TrackMap } from './components/TrackMap.tsx';
import { type DateRange, rangeFromSearch, rangeToSearch } from './range.ts';

/** 位置ログのビューア。期間は URL のクエリに持ち、再読み込みやブックマークでも同じ期間を開く。 */
export function App() {
  const [today] = useState(() => new Date());
  const [range, setRange] = useState<DateRange>(() =>
    rangeFromSearch(window.location.search, today),
  );
  const { data, loading, error } = useHistory(range);

  useEffect(() => {
    const search = rangeToSearch(range);
    if (search !== window.location.search) {
      window.history.replaceState(null, '', search);
    }
  }, [range]);

  return (
    <div className={styles.app}>
      <header className={styles.header}>
        <h1>位置ログ</h1>
        <RangePicker range={range} today={today} onChange={setRange} />
        {loading && <span>読み込み中…</span>}
      </header>
      {error && <p className={styles.error}>読み込めませんでした: {error}</p>}
      <main className={styles.main}>
        <div className={styles.map}>
          {data ? (
            <TrackMap track={data.track} meta={data.track_meta} />
          ) : (
            <div className={styles.status}>{loading ? '読み込み中…' : '表示できるデータがありません'}</div>
          )}
        </div>
        <aside className={styles.side}>
          {data && (
            <>
              <SummaryPanel summary={data.total} timezone={data.range.timezone} />
              <DailyCharts days={data.days} onSelectDay={(date) => setRange({ from: date, to: date })} />
            </>
          )}
        </aside>
      </main>
    </div>
  );
}
```

- [ ] **Step 3: 型チェック、lint、テスト、ビルドを通す**

Run: `~/.nix-profile/bin/just web-check && ~/.nix-profile/bin/just web-build`
Expected: すべて通り、`web/dist/index.html` と `web/dist/assets/` ができる

- [ ] **Step 4: 開発サーバーで見た目を確かめる**

手元に kgd を動かせる `config.toml` と DB がある場合に行う。
無ければこの Step は飛ばし、Task 18 で確かめる。

1. `config.toml` の `[location.viewer]` の `allowed_cidrs` に `"127.0.0.1/32"` を足し、`just run` で kgd を起動する
2. 別の端末で `just web-dev` を実行し、表示された URL の `/viewer/` をブラウザで開く
3. 次を確かめる
   - 今日の軌跡が色分けされて表示され、地図が軌跡に合わせてズームする
   - 「直近 7 日」を押すと URL が `?from=...&to=...` に変わり、グラフに 7 本の棒が出る
   - グラフの棒をクリックすると、その日だけの表示になる
   - カレンダーで 2 日以上の範囲を選んで「この期間を表示」を押すと、その範囲になる
   - 記録の無い日を選ぶと「この期間の記録はありません」と出る
   - ブラウザの幅を狭めると、地図の下に集計が並ぶ

- [ ] **Step 5: コミットする**

```bash
git add web/src
git -c commit.gpgsign=false commit -m "feat: ビューアの期間の選択と画面の組み立てを追加する

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---
### Task 16: Docker のビルドと CI

**Files:**
- Modify: `Dockerfile`
- Modify: `.dockerignore`
- Modify: `.github/workflows/build.yml`

**Interfaces:**
- Consumes: `web/` の `aube-lock.yaml` と `build` スクリプト (Task 10)、`gen` スクリプト (Task 11)
- Produces: 画面を埋め込んだイメージ。画面のビルドを忘れたイメージはビルドの段で失敗する

aube の公式 npm パッケージ `@endevco/aube` は、ネイティブバイナリを install スクリプトで取得する。
そのため `--ignore-scripts=false` を付けて入れる (aube のインストール手順のとおり)。
CI での手順は aube のドキュメントのとおり `aube install --frozen-lockfile` と `aube run --no-install <script>` を使う。

- [ ] **Step 1: Dockerfile に画面のビルドの段を足す**

`Dockerfile` の先頭の `# Stage 1: Chef base` の前に足す。

```dockerfile
# ========================================
# Stage 0: Web (build the location viewer)
# ========================================
# 成果物は静的ファイルでアーキテクチャに依存しないため、ビルドするマシンのネイティブで動かす
FROM --platform=$BUILDPLATFORM node:22-bookworm-slim AS web

# aube の公式 npm パッケージ。ネイティブバイナリを install スクリプトで取得する
RUN npm install -g --ignore-scripts=false @endevco/aube@2.6.1

WORKDIR /web

# 依存だけを先に入れて、ソースの変更でキャッシュが切れないようにする
COPY web/package.json web/aube-lock.yaml ./
RUN aube install --frozen-lockfile

COPY web/ ./
RUN aube run --no-install build
```

builder の段の `COPY . .` と `RUN cargo build --release --bin kgd` の間に足す。

```dockerfile
# 画面を埋め込む。rust-embed はフォルダが無くてもビルドを通すため、ここで有無を確かめる
COPY --from=web /web/dist web/dist
RUN test -f web/dist/index.html
```

`.dockerignore` の末尾に足す。

```
web/node_modules/
web/dist/
```

- [ ] **Step 2: イメージをビルドして確かめる**

Run: `/usr/bin/docker build --target ci -t kgd:viewer-test .`
Expected: web の段で `aube run --no-install build` が通り、builder の段の `test -f web/dist/index.html` が通ってビルドが終わる (Pi では数十分かかることがある)

Run: `/usr/bin/docker run --rm kgd:viewer-test --version`
Expected: バージョンが表示される

画面が埋め込まれていることを確かめる。

Run: `/usr/bin/docker run --rm --entrypoint sh kgd:viewer-test -c 'grep -c "tiles.openfreemap.org" /app/kgd'`
Expected: 1 以上 (画面の JS がバイナリに入っている)

`grep` がイメージに無い場合は、`docker create` と `docker cp` で `/app/kgd` を取り出し、ホストの `grep -c` で確かめる。

- [ ] **Step 3: CI に画面のジョブを足す**

`.github/workflows/build.yml` の `coverage:` ジョブの前に足す。

```yaml
  web:
    runs-on: ubuntu-latest
    defaults:
      run:
        working-directory: web
    steps:
      - uses: actions/checkout@v4

      - name: Set up Node.js
        uses: actions/setup-node@v4
        with:
          node-version: 22

      - name: Install aube
        run: npm install -g --ignore-scripts=false @endevco/aube@2.6.1

      - name: Install dependencies
        run: aube install --frozen-lockfile

      - name: Typecheck
        run: aube run --no-install typecheck

      - name: Lint
        run: aube run --no-install lint

      - name: Test
        run: aube run --no-install test

      # Rust の型から作った schema.json と、そこから作る valibot のスキーマがずれていないか確かめる。
      # schema.json 自体のずれは check ジョブの cargo test (api_schema_matches_committed_file) が見つける。
      - name: Check generated API schema is up to date
        run: |
          aube run --no-install gen
          git diff --exit-code -- src/api/schema.gen.ts
```

- [ ] **Step 4: ワークフローの書式を確かめる**

Run: `python3 -c "import yaml,sys; yaml.safe_load(open('.github/workflows/build.yml'))" && echo ok`
Expected: `ok`

PyYAML が無ければ、`/usr/bin/docker run --rm -v "$PWD":/repo -w /repo rhysd/actionlint:latest` で確かめる。

- [ ] **Step 5: コミットする**

```bash
git add Dockerfile .dockerignore .github/workflows/build.yml
git -c commit.gpgsign=false commit -m "build: ビューアの画面をビルドしてイメージに埋め込み、CI で検査する

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 17: 公開範囲の絞り込みと確認のコマンド

**Files:**
- Modify: `cloudflared.example/config.yml`
- Modify: `Justfile`
- Modify: `README.md`

**Interfaces:**
- Produces: Justfile のレシピ `check-exposure url`

cloudflared の ingress の `path` は Go の正規表現で、クエリを除いたパスに対して照合する。
照合は前後に固定されないため、`^` と `$` を自分で付ける。

- [ ] **Step 1: cloudflared の雛形を直す**

`cloudflared.example/config.yml` の `ingress:` を次にする。

```yaml
ingress:
  # `cloudflared tunnel route dns <名前> <ホスト名>` で割り当てたホスト名を、
  # kgd の [location] listen が待ち受けるポートへ向ける。
  # 通すのは OwnTracks の受け口だけにする。同じポートの /viewer/ (位置ログのビューア) には
  # ログインが無いため、トンネルに出さない (ADR-0013)。
  - hostname: owntracks.example.com
    path: ^/(pub|healthz)$
    service: http://127.0.0.1:8081
  - service: http_status:404
```

- [ ] **Step 2: 公開状態を確かめるレシピを足す**

`Justfile` の `gen-api` の後ろに足す。

```make
# Check that only the OwnTracks endpoints are reachable at the given base URL
# (e.g. just check-exposure https://owntracks.example.com)
check-exposure url:
    #!/usr/bin/env bash
    set -euo pipefail
    fail=0
    check() {
      local path="$1" expected="$2" status
      status=$(curl -s -o /dev/null -w '%{http_code}' "{{url}}$path")
      if [[ " $expected " == *" $status "* ]]; then
        echo "ok   $path -> $status"
      else
        echo "FAIL $path -> $status (expected: $expected)"
        fail=1
      fi
    }
    check /viewer/ "403 404"
    check "/viewer/api/history?from=2026-01-01&to=2026-01-01" "403 404"
    check /healthz "200"
    exit "$fail"
```

- [ ] **Step 3: レシピを手元で確かめる**

kgd を手元で動かせる場合だけ行う (Task 9 の Step 4 と同じ設定。`allowed_cidrs` に loopback を入れない)。

Run: `~/.nix-profile/bin/just check-exposure http://127.0.0.1:8081`
Expected: 3 行とも `ok` で、終了コード 0 (loopback からは `/viewer/` が 403)

Run: `~/.nix-profile/bin/just check-exposure "http://$(hostname -I | awk '{print $1}'):8081"`
Expected: `/viewer/` の 2 行が `FAIL ... -> 200` になり、終了コードが 1 (LAN からは見えるため、レシピが公開を検出できることの確認)

- [ ] **Step 4: README を直す**

`README.md` の「Cloudflare Access は使わない。」で始まる段落を、次の 2 段落に置き換える。

```markdown
Cloudflare Access は使わない。OwnTracks はブラウザではないため Access のログイン画面を通過できない。公開 URL を守るのは `[location]` の Basic 認証のみになる。

同じポートには位置ログのビューア (`/viewer/`) も載る。ビューアにはログインが無いため、ingress の `path: ^/(pub|healthz)$` で OwnTracks の受け口だけをトンネルに通す。雛形 (`cloudflared.example/config.yml`) より前に作った `cloudflared/config.yml` にはこの行が無いので、足してから cloudflared を再起動すること。
```

同じファイルの `import-owntracks` の段落の前に、ビューアの節を足す。

````markdown
### 位置ログのビューア

記録した軌跡と集計を、LAN の中からブラウザで見られる。`[location]` に `[location.viewer]` を足すと有効になり、`http://<host>:8081/viewer/` で開ける。

```toml
[location.viewer]
# ビューアに届いてよい送信元 (省略時: LAN のプライベート帯。loopback は含まない)
allowed_cidrs = ["192.168.0.0/16"]
# 地図に返す軌跡の点数の上限 (省略時: 20000)
max_track_points = 20000
```

ビューアにはログインが無い。代わりに、送信元が `allowed_cidrs` に無いリクエストと、Cloudflare を経由したリクエスト (`Cf-Connecting-IP` などのヘッダを持つもの) を 403 で拒否する。同じホストの cloudflared は 127.0.0.1 から接続してくるため、本番では `allowed_cidrs` に loopback を入れないこと。

有効にする手順は次のとおり。順番を守ると、途中でビューアがインターネットに出ることが無い。

1. 本番の `cloudflared/config.yml` の OwnTracks のホスト名に `path: ^/(pub|healthz)$` を足し、`docker compose --profile tunnel restart tunnel` で反映する
2. `config.toml` に `[location.viewer]` を足し、kgd を新しいイメージで起動し直す
3. `just check-exposure https://<OwnTracks のホスト名>` を実行し、すべて `ok` になることを確かめる
4. LAN の端末のブラウザで `http://<host>:8081/viewer/` を開く

画面は `web/` にある React のアプリで、Docker のイメージをビルドするときにビルドしてバイナリに埋め込む。手元で開発するときは次のようにする。

```bash
just web-install   # 依存を入れる (mise で node と aube を入れておく)
just web-dev       # Vite の開発サーバー。/viewer/api は 127.0.0.1:8081 の kgd へ流す
just web-build     # web/dist にビルドする (デバッグビルドの kgd はここを直接読む)
just web-check     # 型チェック、lint、テスト
just gen-api       # Rust の API の型を変えたら、画面側のスキーマを作り直す
```

開発サーバーからのプロキシは 127.0.0.1 から届くため、手元の `config.toml` でだけ `allowed_cidrs` に `"127.0.0.1/32"` を足す。
````

- [ ] **Step 5: コミットする**

```bash
git add cloudflared.example/config.yml Justfile README.md
git -c commit.gpgsign=false commit -m "docs: トンネルに OwnTracks の受け口だけを通し、公開状態を確かめるレシピを足す

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 18: アーキテクチャの文書と ADR

**Files:**
- Modify: `docs/architecture.md`
- Create: `docs/adr/0013-guard-viewer-sharing-the-owntracks-listener.md`
- Create: `docs/adr/0014-build-viewer-with-react-and-embed-it.md`
- Modify: `docs/adr/README.md`

- [ ] **Step 1: アーキテクチャの文書を直す**

`docs/architecture.md` を次のとおり直す。

- 層構成の図の `PRES` のラベルを `Controller (DiscordController / owntracks_router / viewer_router) / Presenter` にする
- 「各層の責務」の表の kgd-presentation の行の「置くもの」を「Discord イベントを受ける Controller (DiscordController)、OwnTracks の HTTP 受け口 (owntracks_router)、位置ログのビューア (viewer_router、埋め込んだ画面の配信)、結果を文言・embed・API の応答に変換する Presenter」にする
- 「ユースケース一覧」の表の末尾に `| BrowseLocationHistoryUseCase | 暦日で選んだ期間の位置ログを、日ごとの集計と地図用の軌跡 (上限を超えたら間引く) にまとめる |` を足す
- 「テスト戦略」の前に、次の節を足す

````markdown
## 位置ログのビューア (web/)

ブラウザで位置ログを見る画面は、リポジトリ直下の `web/` にある React、Vite、TypeScript のアプリである。
Docker のビルドの段でビルドし、kgd-presentation が rust-embed で `web/dist` をバイナリに埋め込んで `/viewer/` 以下で配信する ([ADR-0014](adr/0014-build-viewer-with-react-and-embed-it.md))。

API の入出力の型は kgd-presentation の DTO を正とする。
DTO から schemars で `web/src/api/schema.json` を書き出し、自前の変換スクリプトで `web/src/api/schema.gen.ts` (valibot のスキーマ) を作る。
画面は応答を必ずこのスキーマで検証してから使う。
型を変えたら `just gen-api` で両方を作り直す。
生成し忘れは、Rust のテストと CI の web ジョブがそれぞれ検出する。

ビューアは OwnTracks の受け口と同じ待ち受けに置き、ログインの代わりに送信元の許可リストと Cloudflare 経由の印で守る ([ADR-0013](adr/0013-guard-viewer-sharing-the-owntracks-listener.md))。

```mermaid
sequenceDiagram
    participant B as ブラウザ (LAN)
    participant G as guard (presentation)
    participant A as handle_history (presentation)
    participant U as BrowseLocationHistoryUseCase
    participant R as LocationRepository

    B->>G: GET /viewer/api/history?from&to
    G->>G: 送信元の許可リスト / Cloudflare の印
    G->>A: 通す
    A->>U: browse(from, to)
    U->>R: locations_between (1 回)
    U->>U: 暦日ごとに集計、軌跡を間引く (domain 純粋関数)
    A->>B: JSON (Presenter が DTO に変換)
```
````

- [ ] **Step 2: ADR を書く**

`docs/adr/0013-guard-viewer-sharing-the-owntracks-listener.md`:

```markdown
# 0013: ビューアを OwnTracks の受け口と同じ待ち受けに置き、多重のガードで守る

## ステータス

受理 (2026-09-30)

## 文脈

位置ログをブラウザで見るビューアを作るにあたり、今は LAN の中から見られれば足りる。
ビューアにはログインを設けない。

OwnTracks の受け口 (`[location] listen`、既定 8081) は cloudflared を通して
インターネットに公開している。cloudflared は WoL のために host ネットワークで動く kgd と
同じ名前空間から 127.0.0.1 で接続するため、kgd から見るとトンネル経由のリクエストは
ローカルからのリクエストと区別がつかない。

待ち受けを分ければトンネルから切り離せるが、設定と待ち受けを 1 つにまとめたいという要望があった。
同じ待ち受けに置くと、何もしなければ位置の履歴が認証なしでインターネットから見える。

## 決定

**ビューアを OwnTracks の受け口と同じ待ち受けの `/viewer/` 以下に置き、
どれか 1 つの設定を誤っても他で止まるよう、独立したガードを重ねる。**

1. `[location.viewer]` を書いたときだけビューアのルートを登録する
2. ソケットの相手アドレスが `allowed_cidrs` に含まれるときだけ応答する。既定値は LAN の
   プライベート帯で loopback を含めないため、同じホストの cloudflared から届くリクエストは
   これだけで拒否される。IPv4 射影アドレスは IPv4 に戻してから照合する。
   `X-Forwarded-For` などのヘッダは使わない
3. `Cf-Connecting-IP`、`Cf-Ray`、`Cdn-Loop: cloudflare` のいずれかがあれば拒否する。
   Cloudflare のエッジが付与するヘッダで、インターネット側の利用者には取り除けないため、
   cloudflared が別のホストへ移ってもトンネル経由のリクエストを止められる
4. cloudflared の ingress で、OwnTracks のホスト名に `path: ^/(pub|healthz)$` を付ける
5. `just check-exposure` で、デプロイ後に外から `/viewer/` が見えないことを確かめる

ガード 2 と 3 はビューアのルートだけにかけ、`/pub` と `/healthz` には影響させない。

## 結果

ガード 2 と 3 は、前に立つのが cloudflared であることを前提にしている。
Cloudflare 以外のリバースプロキシ (aoi の Web サーバーを一元管理する入口や、Docker の
ブリッジネットワーク上のコンテナなど) を kgd の前に置くと、送信元はプライベートなアドレスになり
Cloudflare の印も付かないため、ビューアに認証なしで届く。既定の `172.16.0.0/12` は Docker の
ブリッジの範囲も含む。そのような入口を置くときは、この決定を見直す。

将来 Cloudflare Access を前に立ててトンネルから公開したくなったときも、ガード 2 と 3 が公開を止める。
これは意図した挙動であり、公開するときはオリジン側での Access の JWT 検証と合わせて設定を変える。

手元で Vite の開発サーバーからプロキシするときは、手元の設定でだけ `allowed_cidrs` に
`127.0.0.1/32` を足す。loopback を含む許可リストで起動すると、kgd は警告のログを出す。
```

`docs/adr/0014-build-viewer-with-react-and-embed-it.md`:

```markdown
# 0014: ビューアの画面は React と Vite で作り、バイナリに埋め込み、API の型は Rust から生成する

## ステータス

受理 (2026-09-30)

## 文脈

ビューアの画面は、期間を選び、GL の地図に軌跡を描き、日ごとの集計をグラフにする。
kgd はこれまで Rust だけでビルドしており、Dockerfile にも Node は無い。

## 決定

**画面は React、Vite、TypeScript で作り、パッケージマネージャーには aube を使う。**
地図は MapLibre GL JS (`@vis.gl/react-maplibre`) で、OpenFreeMap のベクタータイルをブラウザから
直接取得する。タイルの中継はしない。

**Docker のビルド専用の Node の段で画面をビルドし、rust-embed でバイナリに埋め込む。**
実行用のイメージには Node を入れない。rust-embed は `web/dist` が無くてもビルドを通すため、
Node が無い環境でも `cargo build` と `cargo test` は動く。その代わり、Docker の builder の段で
`web/dist/index.html` の有無を確かめ、画面を含まないイメージができないようにする。

**API の入出力の型は Rust の DTO を正とし、schemars の JSON Schema から valibot のスキーマを生成する。**
JSON Schema から valibot への変換は自前の小さなスクリプトで行う。既存の変換ツールは、最も
知られていた `liam-hq/json-schema-to-valibot` が 2026 年 6 月にアーカイブされており、他の実装も
利用者が少なかった。スクリプトは今回の DTO が使うキーワードだけに対応し、知らないキーワードに
出会ったらエラーで止まる。

Rust の型から valibot を直接出す specta-valibot は採らなかった。specta 2 は RC の段階にあり、
specta-valibot は部分的な実装で crates.io に公開されておらず、`deny.toml` の `allow-git` に
例外が要るためである。

生成物 (`web/src/api/schema.json` と `schema.gen.ts`) はコミットする。Docker の Node の段が
Rust 無しでビルドでき、API の変更がレビューの差分に現れる。

## 結果

画面を変えるには Node と aube が要る。mise で入れる。

Rust の型を変えて生成し忘れると、kgd-presentation のテストが `schema.json` のずれを、
CI の web ジョブが `schema.gen.ts` のずれを検出する。

OpenFreeMap の提供が止まると地図の背景が表示されなくなる。軌跡と集計は表示される。
```

`docs/adr/README.md` の表の末尾に足す。

```markdown
| [0013](0013-guard-viewer-sharing-the-owntracks-listener.md) | ビューアを OwnTracks の受け口と同じ待ち受けに置き、多重のガードで守る | 受理 |
| [0014](0014-build-viewer-with-react-and-embed-it.md) | ビューアの画面は React と Vite で作り、バイナリに埋め込み、API の型は Rust から生成する | 受理 |
```

- [ ] **Step 3: コミットする**

```bash
git add docs/architecture.md docs/adr
git -c commit.gpgsign=false commit -m "docs: 位置ログのビューアのアーキテクチャと ADR-0013, 0014 を追加する

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 19: 全体の確認

**Files:** なし (確認だけ。直したら該当するタスクのコミットに足さず、新しいコミットにする)

- [ ] **Step 1: Rust の CI と同じ検査を通す**

Run: `~/.nix-profile/bin/just ci`
Expected: fmt-check、check、clippy、deny (ipnet、schemars、rust-embed とその依存のライセンスを含む)、machete、test がすべて通る

`cargo deny` がライセンスで落ちた場合は、落ちたクレートとライセンスを利用者に報告して判断を仰ぐ。
`deny.toml` の許可リストを勝手に広げない。

- [ ] **Step 2: 画面の検査と生成物のずれを確かめる**

Run: `~/.nix-profile/bin/just web-check && ~/.nix-profile/bin/just gen-api && git status --short`
Expected: 検査がすべて通り、`gen-api` の後に差分が無い

- [ ] **Step 3: 設計書との対応を確かめる**

`docs/superpowers/specs/2026-09-30-location-viewer-design.md` の各節について、実装した場所を確かめる。

- 設定、アクセスの保護 (5 つのガード)、各層に置くもの、API、エラー、型の生成、画面、ビルドと配布、デプロイ、テスト、ドキュメント

食い違いがあれば、実装か設計書のどちらを直すかを利用者に確かめる。

- [ ] **Step 4: 本番相当の環境での確認を利用者に依頼する**

ここから先は本番の設定と cloudflared を触るため、実行者は手順を示すだけにし、実施は利用者に任せる。
README の「位置ログのビューア」の手順 1 から 4 を案内し、次を確かめてもらう。

- `just check-exposure https://<OwnTracks のホスト名>` がすべて `ok`
- OwnTracks アプリからの送信がこれまでどおり届く (kgd のログに `Recorded OwnTracks messages` が出る)
- LAN の端末で `/viewer/` が開き、Task 15 の Step 4 の項目がすべて期待どおり
