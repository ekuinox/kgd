# OwnTracks 受信と保存 実装計画

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** OwnTracks (iPhone) が送る位置情報を kgd が HTTP で受信し、PostgreSQL に冪等に保存する。既存の JSONL も同じ経路で取り込めるようにする。

**Architecture:** axum のルータを kgd-presentation に置き、`RecordLocationUseCase` (kgd-application) 経由で `LocationStore` (kgd-infrastructure, sqlx) に書く。JSON から型への変換は kgd-domain の純粋関数に寄せる。冪等性はテーブルの UNIQUE 制約と `ON CONFLICT DO NOTHING` に委ねる。

**Tech Stack:** Rust 2024 / axum 0.8 / sqlx 0.8 (PostgreSQL) / serde_json / base64 / tokio / mockall

**Spec:** `docs/superpowers/specs/2026-09-21-owntracks-location-tracking-design.md`

## Global Constraints

- 依存方向は kgd-domain ← kgd-application ← kgd-infrastructure / kgd-presentation ← kgd (binary)。kgd-domain に IO ライブラリ (serenity / sqlx / reqwest / tokio / axum) を入れてはならない
- 新規依存は workspace の `[workspace.dependencies]` に定義し、各 crate では `axum.workspace = true` の形で参照する。値が 1 つのときはインラインテーブルを使わない
- 追加してよいライセンスは MIT / Apache-2.0 / Apache-2.0 WITH LLVM-exception / BSD-2-Clause / BSD-3-Clause / ISC / Zlib / Unicode-3.0 / CDLA-Permissive-2.0 のみ (`deny.toml`)
- コミット前に `just validate` (fmt / check / clippy -D warnings)、プッシュ前に `just ci` (fmt-check / check / clippy / deny / machete / test) を通す
- 判断・変換のロジックは kgd-domain の純粋関数として書き、同一ファイル内の `#[cfg(test)] mod tests` でテストする
- 非同期テストは `#[tokio::test]`
- テスト関数名は `<対象>_<動詞>_<条件>` のスネークケース英文。doc コメントは日本語で「何を確認するか」を 1 文、必要なら空行を挟んで「なぜそうあるべきか」
- 構造体とフィールドには doc コメントを付ける
- `use` は std / 外部クレート / `crate` / `super` のブロックに分けて書く
- コミットメッセージは日本語。末尾に `Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>` を付ける
- yomogi では署名鍵が無いため `git -c commit.gpgsign=false commit ...` でコミットする

---

## ファイル構成

| ファイル | 責務 |
|---|---|
| `crates/kgd-domain/src/owntracks.rs` (新規) | OwnTracks メッセージのドメイン型と、JSON からの変換 (純粋関数) |
| `crates/kgd-application/src/ports/location.rs` (新規) | `LocationRepository` ポート |
| `crates/kgd-application/src/record_location.rs` (新規) | `RecordLocationUseCase` |
| `crates/kgd-infrastructure/migrations/20260921_000001_create_owntracks_messages.sql` (新規) | テーブル定義 |
| `crates/kgd-infrastructure/src/location_store/mod.rs` (新規) | `LocationStore` (sqlx 実装) |
| `crates/kgd-infrastructure/src/store/mod.rs` (変更) | プール生成とマイグレーションを `connect_pool` に分離 |
| `crates/kgd-presentation/src/owntracks/mod.rs` (新規) | axum のルータとハンドラ |
| `crates/kgd-presentation/src/owntracks/auth.rs` (新規) | Basic 認証の検証 (純粋関数) |
| `crates/kgd-infrastructure/src/http_server.rs` (新規) | ルータを bind して serve する常駐タスク |
| `crates/kgd/src/config/mod.rs` (変更) | `[location]` セクション |
| `crates/kgd/src/bootstrap.rs` (変更) | プール共有と HTTP サーバーの起動 |
| `crates/kgd/src/main.rs` (変更) | `import-owntracks` サブコマンド |
| `crates/kgd/src/import.rs` (新規) | JSONL 取り込みの実行部 |

---

### Task 1: ドメイン型と JSON からの変換

**Files:**
- Create: `crates/kgd-domain/src/owntracks.rs`
- Modify: `crates/kgd-domain/src/lib.rs`
- Test: `crates/kgd-domain/src/owntracks.rs` (同一ファイル内 `mod tests`)

**Interfaces:**
- Consumes: なし (最初のタスク)
- Produces:
  - `pub struct OwnTracksMessage { pub user_id: String, pub device_id: String, pub msg_type: String, pub tst: Option<DateTime<Utc>>, pub lat: Option<f64>, pub lon: Option<f64>, pub acc: Option<i32>, pub alt: Option<i32>, pub vel: Option<i32>, pub batt: Option<i16>, pub trigger_type: Option<String>, pub motion: Option<String>, pub payload: serde_json::Value }`
  - `pub fn parse_owntracks_message(user_id: &str, device_id: &str, payload: Value) -> Option<OwnTracksMessage>`
  - `pub fn sanitize_identifier(raw: Option<&str>, fallback: &str) -> String`

- [ ] **Step 1: 失敗するテストを書く**

`crates/kgd-domain/src/owntracks.rs` を新規作成し、テストだけを書く。

```rust
//! OwnTracks から受け取るメッセージのドメイン型と変換。
//!
//! IO を伴わない純粋関数として実装し、JSON は引数で受ける。

#[cfg(test)]
mod tests {
    use chrono::TimeZone as _;
    use serde_json::json;

    use super::*;

    /// location メッセージの主要な項目が型付きのフィールドへ取り出され、
    /// payload には元の JSON がそのまま残ることを確認する。
    #[test]
    fn parse_owntracks_message_extracts_location_fields() {
        let payload = json!({
            "_type": "location",
            "tst": 1789266896,
            "lat": 34.710452,
            "lon": 135.471914,
            "acc": 8,
            "alt": 2,
            "vel": 4,
            "batt": 89,
            "t": "t",
            "motionactivities": ["walking"]
        });

        let message = parse_owntracks_message("ekuinox", "ohtori", payload.clone())
            .expect("should parse");

        assert_eq!(message.user_id, "ekuinox");
        assert_eq!(message.device_id, "ohtori");
        assert_eq!(message.msg_type, "location");
        assert_eq!(message.tst, Some(Utc.timestamp_opt(1789266896, 0).unwrap()));
        assert_eq!(message.lat, Some(34.710452));
        assert_eq!(message.lon, Some(135.471914));
        assert_eq!(message.acc, Some(8));
        assert_eq!(message.alt, Some(2));
        assert_eq!(message.vel, Some(4));
        assert_eq!(message.batt, Some(89));
        assert_eq!(message.trigger_type, Some("t".to_string()));
        assert_eq!(message.motion, Some("walking".to_string()));
        assert_eq!(message.payload, payload);
    }

    /// tst や位置を持たないメッセージ (waypoints など) でも、
    /// _type さえあれば取りこぼさずに保持できることを確認する。
    #[test]
    fn parse_owntracks_message_accepts_message_without_position() {
        let payload = json!({ "_type": "waypoints", "waypoints": [] });

        let message =
            parse_owntracks_message("ekuinox", "ohtori", payload).expect("should parse");

        assert_eq!(message.msg_type, "waypoints");
        assert_eq!(message.tst, None);
        assert_eq!(message.lat, None);
    }

    /// _type を持たない JSON は取り込まないことを確認する。
    ///
    /// 種別が分からないとレポートの集計対象を決められないため。
    #[test]
    fn parse_owntracks_message_rejects_payload_without_type() {
        assert!(parse_owntracks_message("u", "d", json!({ "lat": 1.0 })).is_none());
    }

    /// 配列やスカラなどオブジェクトでない JSON を弾くことを確認する。
    #[test]
    fn parse_owntracks_message_rejects_non_object() {
        assert!(parse_owntracks_message("u", "d", json!([1, 2, 3])).is_none());
    }

    /// acc が小数で届いても整数へ丸めて取り込むことを確認する。
    ///
    /// OwnTracks は端末やバージョンによって精度を小数で送ることがあるため。
    #[test]
    fn parse_owntracks_message_rounds_fractional_accuracy() {
        let payload = json!({ "_type": "location", "acc": 12.6 });

        let message = parse_owntracks_message("u", "d", payload).expect("should parse");

        assert_eq!(message.acc, Some(13));
    }

    /// 識別子に使えない文字を落とし、空になった場合は代替値を使うことを確認する。
    ///
    /// ヘッダの値がそのままテーブルへ入るため、制御文字や記号を持ち込まない。
    #[test]
    fn sanitize_identifier_strips_unsafe_characters() {
        assert_eq!(sanitize_identifier(Some("ekuinox"), "unknown"), "ekuinox");
        assert_eq!(sanitize_identifier(Some("oh/tori\n"), "unknown"), "ohtori");
        assert_eq!(sanitize_identifier(Some("///"), "unknown"), "unknown");
        assert_eq!(sanitize_identifier(None, "unknown"), "unknown");
    }
}
```

- [ ] **Step 2: テストが失敗することを確認する**

```
cargo test -p kgd-domain owntracks
```

期待: コンパイルエラー (`parse_owntracks_message` が見つからない)。

- [ ] **Step 3: 実装を書く**

`crates/kgd-domain/src/owntracks.rs` のテストより上に追記する。

```rust
use chrono::{DateTime, TimeZone as _, Utc};
use serde_json::Value;

/// OwnTracks から受け取った 1 件のメッセージ。
///
/// レポートで使う項目は型付きで持ち、元の JSON は `payload` に丸ごと残す。
/// 後から別の項目が必要になったときに再取り込みを不要にするため。
#[derive(Debug, Clone, PartialEq)]
pub struct OwnTracksMessage {
    /// 端末が名乗るユーザー識別子 (X-Limit-U)
    pub user_id: String,
    /// 端末が名乗るデバイス識別子 (X-Limit-D)
    pub device_id: String,
    /// メッセージ種別 (_type)
    pub msg_type: String,
    /// 端末が位置を取得した時刻 (tst)
    pub tst: Option<DateTime<Utc>>,
    /// 緯度
    pub lat: Option<f64>,
    /// 経度
    pub lon: Option<f64>,
    /// 水平精度 (メートル)
    pub acc: Option<i32>,
    /// 高度 (メートル)
    pub alt: Option<i32>,
    /// 速度 (km/h)
    pub vel: Option<i32>,
    /// バッテリー残量 (%)
    pub batt: Option<i16>,
    /// 送信のきっかけ (t)
    pub trigger_type: Option<String>,
    /// モーション判定の先頭要素 (motionactivities[0])
    pub motion: Option<String>,
    /// 受け取った JSON 全体
    pub payload: Value,
}

/// 識別子として安全な文字だけを残す。空になった場合は `fallback` を返す。
pub fn sanitize_identifier(raw: Option<&str>, fallback: &str) -> String {
    let kept: String = raw
        .unwrap_or_default()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        .collect();

    if kept.is_empty() {
        fallback.to_string()
    } else {
        kept
    }
}

/// JSON から [`OwnTracksMessage`] を組み立てる。
///
/// オブジェクトでない、または `_type` を持たない JSON は `None` を返す。
pub fn parse_owntracks_message(
    user_id: &str,
    device_id: &str,
    payload: Value,
) -> Option<OwnTracksMessage> {
    let object = payload.as_object()?;
    let msg_type = object.get("_type")?.as_str()?.to_string();

    let tst = object
        .get("tst")
        .and_then(Value::as_i64)
        .and_then(|seconds| Utc.timestamp_opt(seconds, 0).single());

    let motion = object
        .get("motionactivities")
        .and_then(Value::as_array)
        .and_then(|values| values.first())
        .and_then(Value::as_str)
        .map(str::to_string);

    Some(OwnTracksMessage {
        user_id: user_id.to_string(),
        device_id: device_id.to_string(),
        msg_type,
        tst,
        lat: object.get("lat").and_then(Value::as_f64),
        lon: object.get("lon").and_then(Value::as_f64),
        acc: rounded_i32(object.get("acc")),
        alt: rounded_i32(object.get("alt")),
        vel: rounded_i32(object.get("vel")),
        batt: rounded_i32(object.get("batt")).and_then(|v| i16::try_from(v).ok()),
        trigger_type: object.get("t").and_then(Value::as_str).map(str::to_string),
        motion,
        payload: payload.clone(),
    })
}

/// 数値を i32 に丸める。整数でも小数でも受け取れるようにする。
fn rounded_i32(value: Option<&Value>) -> Option<i32> {
    let number = value?.as_f64()?;
    if number.is_finite() {
        Some(number.round() as i32)
    } else {
        None
    }
}
```

`crates/kgd-domain/src/lib.rs` の `mod` 宣言に `mod owntracks;` をアルファベット順の位置 (`mod ogp;` の次) に追加し、再エクスポートを追加する。

```rust
pub use owntracks::{OwnTracksMessage, parse_owntracks_message, sanitize_identifier};
```

- [ ] **Step 4: テストが通ることを確認する**

```
cargo test -p kgd-domain owntracks
```

期待: 6 件すべて PASS。

- [ ] **Step 5: コミット**

```bash
just validate
git add crates/kgd-domain/src/owntracks.rs crates/kgd-domain/src/lib.rs
git -c commit.gpgsign=false commit -m "feat(domain): OwnTracks メッセージのドメイン型と変換を追加する

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
```

---

### Task 2: ポートとユースケース

**Files:**
- Create: `crates/kgd-application/src/ports/location.rs`
- Create: `crates/kgd-application/src/record_location.rs`
- Modify: `crates/kgd-application/src/ports/mod.rs`
- Modify: `crates/kgd-application/src/lib.rs`
- Test: `crates/kgd-application/src/record_location.rs` (同一ファイル内 `mod tests`)

**Interfaces:**
- Consumes: `kgd_domain::{OwnTracksMessage, parse_owntracks_message}` (Task 1)
- Produces:
  - `pub trait LocationRepository { async fn insert_messages(&self, messages: &[OwnTracksMessage]) -> Result<usize>; }` — 戻り値は実際に挿入された件数 (重複で無視されたぶんは含まない)
  - `pub struct RecordLocationUseCase`、`RecordLocationUseCase::new(repo: Arc<dyn LocationRepository>) -> Self`
  - `pub async fn record(&self, user_id: &str, device_id: &str, payloads: Vec<Value>) -> Result<RecordOutcome>`
  - `pub struct RecordOutcome { pub parsed: usize, pub stored: usize, pub skipped: usize }`
  - テスト用に `MockLocationRepository`

- [ ] **Step 1: 失敗するテストを書く**

`crates/kgd-application/src/record_location.rs` を新規作成し、テストだけを書く。

```rust
//! OwnTracks のメッセージを受け取って永続化するユースケース。

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::ports::MockLocationRepository;

    use super::*;

    /// 複数のメッセージをまとめて渡すと 1 回の呼び出しで保存され、
    /// 保存件数が結果に反映されることを確認する。
    #[tokio::test]
    async fn record_stores_all_parsed_messages_in_one_call() {
        let mut repo = MockLocationRepository::new();
        repo.expect_insert_messages()
            .withf(|messages| messages.len() == 2)
            .times(1)
            .returning(|messages| Ok(messages.len()));

        let use_case = RecordLocationUseCase::new(Arc::new(repo));
        let payloads = vec![
            json!({ "_type": "location", "tst": 1, "lat": 1.0, "lon": 2.0 }),
            json!({ "_type": "location", "tst": 2, "lat": 1.5, "lon": 2.5 }),
        ];

        let outcome = use_case.record("ekuinox", "ohtori", payloads).await.unwrap();

        assert_eq!(outcome.parsed, 2);
        assert_eq!(outcome.stored, 2);
        assert_eq!(outcome.skipped, 0);
    }

    /// 解釈できない JSON が混ざっていても、解釈できたぶんだけ保存し、
    /// スキップ件数を返すことを確認する。
    ///
    /// 1 件の異常で端末のバッチ送信全体を落とさないため。
    #[tokio::test]
    async fn record_skips_unparsable_payloads() {
        let mut repo = MockLocationRepository::new();
        repo.expect_insert_messages()
            .withf(|messages| messages.len() == 1)
            .times(1)
            .returning(|messages| Ok(messages.len()));

        let use_case = RecordLocationUseCase::new(Arc::new(repo));
        let payloads = vec![
            json!({ "_type": "location", "tst": 1 }),
            json!({ "lat": 1.0 }),
            json!("plain string"),
        ];

        let outcome = use_case.record("ekuinox", "ohtori", payloads).await.unwrap();

        assert_eq!(outcome.parsed, 1);
        assert_eq!(outcome.stored, 1);
        assert_eq!(outcome.skipped, 2);
    }

    /// すべて解釈できなかった場合はリポジトリを呼ばないことを確認する。
    #[tokio::test]
    async fn record_does_not_touch_repository_when_nothing_parsed() {
        let mut repo = MockLocationRepository::new();
        repo.expect_insert_messages().times(0);

        let use_case = RecordLocationUseCase::new(Arc::new(repo));

        let outcome = use_case
            .record("ekuinox", "ohtori", vec![json!({ "lat": 1.0 })])
            .await
            .unwrap();

        assert_eq!(outcome.parsed, 0);
        assert_eq!(outcome.stored, 0);
        assert_eq!(outcome.skipped, 1);
    }

    /// 重複で挿入されなかった件数が stored に含まれないことを確認する。
    ///
    /// 端末は圏外復帰時に同じ点を送り直すため、再送を「保存した」と数えない。
    #[tokio::test]
    async fn record_reports_stored_count_from_repository() {
        let mut repo = MockLocationRepository::new();
        repo.expect_insert_messages()
            .withf(|messages| messages.len() == 2)
            .times(1)
            .returning(|_| Ok(1));

        let use_case = RecordLocationUseCase::new(Arc::new(repo));
        let payloads = vec![
            json!({ "_type": "location", "tst": 1 }),
            json!({ "_type": "location", "tst": 1 }),
        ];

        let outcome = use_case.record("ekuinox", "ohtori", payloads).await.unwrap();

        assert_eq!(outcome.parsed, 2);
        assert_eq!(outcome.stored, 1);
    }
}
```

引数の内容で一致を見るときは `with` (`Predicate` を要求する) ではなく `withf` (クロージャを取る) を使う。スライスに対する `Predicate` 実装は用意されていないため。

- [ ] **Step 2: テストが失敗することを確認する**

```
cargo test -p kgd-application record_location
```

期待: コンパイルエラー (`RecordLocationUseCase` と `MockLocationRepository` が見つからない)。

- [ ] **Step 3: ポートを書く**

`crates/kgd-application/src/ports/location.rs` を新規作成する。

```rust
//! OwnTracks メッセージの永続化を抽象化するポート。

use anyhow::Result;

use kgd_domain::OwnTracksMessage;

/// OwnTracks メッセージの永続化を抽象化するポート。
#[cfg_attr(test, mockall::automock)]
#[async_trait::async_trait]
pub trait LocationRepository: Send + Sync {
    /// メッセージをまとめて保存し、実際に挿入された件数を返す。
    ///
    /// 既に同じメッセージが保存されている場合は挿入せず、件数にも含めない。
    async fn insert_messages(&self, messages: &[OwnTracksMessage]) -> Result<usize>;
}
```

`crates/kgd-application/src/ports/mod.rs` に 3 行を追加する。

```rust
mod location;
```

```rust
pub use location::LocationRepository;
```

```rust
#[cfg(test)]
pub use location::MockLocationRepository;
```

- [ ] **Step 4: ユースケースを書く**

`crates/kgd-application/src/record_location.rs` のテストより上に追記する。

```rust
use std::sync::Arc;

use anyhow::Result;
use serde_json::Value;
use tracing::warn;

use kgd_domain::{OwnTracksMessage, parse_owntracks_message};

use super::ports::LocationRepository;

/// 受信 1 回ぶんの処理結果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecordOutcome {
    /// 解釈できたメッセージ数
    pub parsed: usize,
    /// 実際に保存された件数 (重複を除く)
    pub stored: usize,
    /// 解釈できず捨てた件数
    pub skipped: usize,
}

/// OwnTracks のメッセージを受け取って永続化するユースケース。
pub struct RecordLocationUseCase {
    /// 位置情報リポジトリポート
    repo: Arc<dyn LocationRepository>,
}

impl RecordLocationUseCase {
    /// 新しい RecordLocationUseCase を作成する。
    pub fn new(repo: Arc<dyn LocationRepository>) -> Self {
        Self { repo }
    }

    /// 受け取った JSON 群を解釈して保存する。
    ///
    /// 解釈できない要素は捨てて処理を続ける。端末は複数メッセージを 1 度に
    /// 送ることがあり、1 件の異常でバッチ全体を失わないようにするため。
    pub async fn record(
        &self,
        user_id: &str,
        device_id: &str,
        payloads: Vec<Value>,
    ) -> Result<RecordOutcome> {
        let total = payloads.len();
        let messages: Vec<OwnTracksMessage> = payloads
            .into_iter()
            .filter_map(|payload| parse_owntracks_message(user_id, device_id, payload))
            .collect();

        let parsed = messages.len();
        let skipped = total - parsed;
        if skipped > 0 {
            warn!(skipped, user_id, device_id, "Skipped unparsable OwnTracks payloads");
        }

        if messages.is_empty() {
            return Ok(RecordOutcome {
                parsed: 0,
                stored: 0,
                skipped,
            });
        }

        let stored = self.repo.insert_messages(&messages).await?;

        Ok(RecordOutcome {
            parsed,
            stored,
            skipped,
        })
    }
}
```

`crates/kgd-application/src/lib.rs` に `mod record_location;` と `pub use record_location::{RecordLocationUseCase, RecordOutcome};` を既存の並びに合わせて追加する。

- [ ] **Step 5: テストが通ることを確認する**

```
cargo test -p kgd-application record_location
```

期待: 4 件すべて PASS。

- [ ] **Step 6: コミット**

```bash
just validate
git add crates/kgd-application/src/ports/location.rs crates/kgd-application/src/ports/mod.rs crates/kgd-application/src/record_location.rs crates/kgd-application/src/lib.rs
git -c commit.gpgsign=false commit -m "feat(application): 位置情報の保存ポートとユースケースを追加する

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
```

---

### Task 3: テーブルとストア

**Files:**
- Create: `crates/kgd-infrastructure/migrations/20260921_000001_create_owntracks_messages.sql`
- Create: `crates/kgd-infrastructure/src/location_store/mod.rs`
- Modify: `crates/kgd-infrastructure/src/store/mod.rs`
- Modify: `crates/kgd-infrastructure/src/lib.rs`
- Modify: `Cargo.toml` (sqlx に `json` feature)

**Interfaces:**
- Consumes: `LocationRepository` (Task 2)、`OwnTracksMessage` (Task 1)
- Produces:
  - `pub async fn connect_pool(database_url: &str) -> Result<PgPool>` (kgd-infrastructure) — 接続してマイグレーションを実行する
  - `pub fn DiaryStore::new(pool: PgPool) -> Self`
  - `pub fn LocationStore::new(pool: PgPool) -> Self`

- [ ] **Step 1: マイグレーションを書く**

`crates/kgd-infrastructure/migrations/20260921_000001_create_owntracks_messages.sql` を新規作成する。

```sql
-- OwnTracks から受信した位置情報メッセージを保存するテーブル
CREATE TABLE IF NOT EXISTS owntracks_messages (
    id BIGSERIAL PRIMARY KEY,
    -- 端末が名乗るユーザー識別子 (X-Limit-U)
    user_id TEXT NOT NULL,
    -- 端末が名乗るデバイス識別子 (X-Limit-D)
    device_id TEXT NOT NULL,
    -- メッセージ種別 (_type)
    msg_type TEXT NOT NULL,
    -- 端末が位置を取得した時刻 (tst)
    tst TIMESTAMPTZ,
    -- 受信した時刻
    received_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- 緯度
    lat DOUBLE PRECISION,
    -- 経度
    lon DOUBLE PRECISION,
    -- 水平精度 (メートル)
    acc INTEGER,
    -- 高度 (メートル)
    alt INTEGER,
    -- 速度 (km/h)
    vel INTEGER,
    -- バッテリー残量 (%)
    batt SMALLINT,
    -- 送信のきっかけ (t)
    trigger_type TEXT,
    -- モーション判定の先頭要素 (motionactivities[0])
    motion TEXT,
    -- 受け取った JSON 全体
    payload JSONB NOT NULL
);

-- 同じメッセージの再送を弾く（端末は圏外復帰時に同じ点を送り直す）
CREATE UNIQUE INDEX IF NOT EXISTS idx_owntracks_messages_identity
    ON owntracks_messages(user_id, device_id, msg_type, tst);

-- 日次レポートは端末と日付で範囲を絞って取り出す
CREATE INDEX IF NOT EXISTS idx_owntracks_messages_device_tst
    ON owntracks_messages(user_id, device_id, tst);
```

- [ ] **Step 2: sqlx に json feature を足す**

`Cargo.toml` の該当行を書き換える。

```toml
sqlx = { version = "0.8", features = ["runtime-tokio", "postgres", "chrono", "json", "migrate"] }
```

- [ ] **Step 3: プール生成を分離する**

`crates/kgd-infrastructure/src/store/mod.rs` の `impl DiaryStore` を次の形に書き換える。`connect` は呼び出し元が無くなるため削除する。

```rust
/// データベースへ接続し、マイグレーションを実行したプールを返す。
///
/// プールは日報と位置情報のストアで共有する。マイグレーションのパスは
/// このクレートの manifest 基準で解決されるため、実行はここに置く。
pub async fn connect_pool(database_url: &str) -> Result<PgPool> {
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(database_url)
        .await
        .context("Failed to connect to database")?;

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .context("Failed to run migrations")?;

    Ok(pool)
}

impl DiaryStore {
    /// 既存のプールからストアを作る。
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}
```

`crates/kgd-infrastructure/src/lib.rs` の再エクスポートに `connect_pool` を追加する。

- [ ] **Step 4: LocationStore を書く**

`crates/kgd-infrastructure/src/location_store/mod.rs` を新規作成する。

```rust
//! OwnTracks の受信メッセージを永続化するストア。

use anyhow::{Context as _, Result};
use sqlx::{PgPool, QueryBuilder, Postgres};

use kgd_application::ports::LocationRepository;
use kgd_domain::OwnTracksMessage;

/// OwnTracks の受信メッセージを管理するストア。
#[derive(Clone)]
pub struct LocationStore {
    pool: PgPool,
}

impl LocationStore {
    /// 既存のプールからストアを作る。
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl LocationRepository for LocationStore {
    async fn insert_messages(&self, messages: &[OwnTracksMessage]) -> Result<usize> {
        if messages.is_empty() {
            return Ok(0);
        }

        // 端末はバッチで送ってくるため、1 文へまとめて往復を減らす。
        let mut builder: QueryBuilder<Postgres> = QueryBuilder::new(
            "INSERT INTO owntracks_messages \
             (user_id, device_id, msg_type, tst, lat, lon, acc, alt, vel, batt, trigger_type, motion, payload) ",
        );

        builder.push_values(messages, |mut row, message| {
            row.push_bind(&message.user_id)
                .push_bind(&message.device_id)
                .push_bind(&message.msg_type)
                .push_bind(message.tst)
                .push_bind(message.lat)
                .push_bind(message.lon)
                .push_bind(message.acc)
                .push_bind(message.alt)
                .push_bind(message.vel)
                .push_bind(message.batt)
                .push_bind(&message.trigger_type)
                .push_bind(&message.motion)
                .push_bind(&message.payload);
        });

        builder.push(" ON CONFLICT (user_id, device_id, msg_type, tst) DO NOTHING");

        let result = builder
            .build()
            .execute(&self.pool)
            .await
            .context("Failed to insert owntracks messages")?;

        Ok(result.rows_affected() as usize)
    }
}
```

`crates/kgd-infrastructure/src/lib.rs` に `mod location_store;` と `pub use location_store::LocationStore;` を追加する。

- [ ] **Step 5: ビルドと既存テストが通ることを確認する**

```
cargo check --all-targets
cargo test --all
```

期待: どちらも成功。`DiaryStore::connect` を参照している箇所が残っていればここで落ちるので、Task 6 を待たず `bootstrap.rs` を `connect_pool` + `DiaryStore::new` に書き換えて通す。

- [ ] **Step 6: コミット**

```bash
just validate
git add Cargo.toml crates/kgd-infrastructure/migrations crates/kgd-infrastructure/src/location_store crates/kgd-infrastructure/src/store/mod.rs crates/kgd-infrastructure/src/lib.rs crates/kgd/src/bootstrap.rs
git -c commit.gpgsign=false commit -m "feat(infrastructure): OwnTracks メッセージのテーブルとストアを追加する

接続プールは日報と共有し、マイグレーションの実行を connect_pool に集約する。

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
```

---

### Task 4: 設定

**Files:**
- Modify: `crates/kgd/src/config/mod.rs`
- Modify: `crates/kgd/src/config/defaults.rs`
- Modify: `crates/kgd/src/config/tests.rs`
- Modify: `config.example.toml`

**Interfaces:**
- Consumes: なし
- Produces: `pub struct LocationConfig { pub listen: SocketAddr, pub username: String, pub password: String }`、`Config` のフィールド `pub location: Option<LocationConfig>`

- [ ] **Step 1: 失敗するテストを書く**

`crates/kgd/src/config/tests.rs` の末尾に追加する。

```rust
/// [location] を書かなければ None になり、既存の設定ファイルがそのまま読めることを確認する。
#[test]
fn location_is_optional() {
    let config: Config = toml::from_str(&minimal_config_toml("")).expect("should parse");
    assert_eq!(config.location, None);
}

/// [location] を書くと待ち受けアドレスと資格情報が読めることを確認する。
#[test]
fn location_parses_listen_and_credentials() {
    let toml_text = format!(
        "{}\n[location]\nlisten = \"0.0.0.0:8081\"\nusername = \"ekuinox\"\npassword = \"secret\"\n",
        minimal_config_toml("")
    );

    let config: Config = toml::from_str(&toml_text).expect("should parse");
    let location = config.location.expect("should be present");

    assert_eq!(location.listen, "0.0.0.0:8081".parse::<SocketAddr>().unwrap());
    assert_eq!(location.username, "ekuinox");
    assert_eq!(location.password, "secret");
}

/// listen を省略すると既定の 0.0.0.0:8081 になることを確認する。
#[test]
fn location_listen_defaults_to_8081() {
    let toml_text = format!(
        "{}\n[location]\nusername = \"ekuinox\"\npassword = \"secret\"\n",
        minimal_config_toml("")
    );

    let config: Config = toml::from_str(&toml_text).expect("should parse");

    assert_eq!(
        config.location.unwrap().listen,
        "0.0.0.0:8081".parse::<SocketAddr>().unwrap()
    );
}
```

冒頭の `use super::*;` に加えて `use std::net::SocketAddr;` を追加する。

- [ ] **Step 2: テストが失敗することを確認する**

```
cargo test -p kgd config::tests::location
```

期待: コンパイルエラー (`Config` に `location` フィールドが無い)。

- [ ] **Step 3: 実装を書く**

`crates/kgd/src/config/mod.rs` の `Config` にフィールドを追加する。

```rust
    /// 日報機能の設定
    pub diary: DiaryConfig,
    /// 位置情報の受信設定 (省略時は機能を無効にする)
    #[serde(default)]
    pub location: Option<LocationConfig>,
}
```

`DiaryConfig` の定義の後ろに追加する。

```rust
/// 位置情報 (OwnTracks) の受信設定。
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct LocationConfig {
    /// HTTP 受け口の待ち受けアドレス（デフォルト: 0.0.0.0:8081）
    ///
    /// 移行期間中は 8080 の既存受け口と併存させるため 8081 を既定とする。
    #[serde(default = "default_location_listen")]
    pub listen: SocketAddr,
    /// Basic 認証のユーザー名
    pub username: String,
    /// Basic 認証のパスワード
    pub password: String,
}
```

`crates/kgd/src/config/mod.rs` の冒頭の `use std::{fs, path::Path, time::Duration};` を `use std::{fs, net::SocketAddr, path::Path, time::Duration};` に変える。

`crates/kgd/src/config/defaults.rs` に追加する。

```rust
pub(super) fn default_location_listen() -> SocketAddr {
    SocketAddr::from(([0, 0, 0, 0], 8081))
}
```

こちらも冒頭に `use std::net::SocketAddr;` を追加する。

- [ ] **Step 4: config.example.toml を更新する**

末尾に追加する。

```toml
# Location Tracking Configuration (OwnTracks)
# Receives location messages over HTTP and stores them in the diary database.
# Omit this section entirely to disable the feature.
# [location]
# Listen address for the HTTP receiver (default: 0.0.0.0:8081)
# listen = "0.0.0.0:8081"
# Basic auth credentials. Must match UserID / Password in the OwnTracks app.
# username = "ekuinox"
# password = "CHANGE_ME"
```

すべてコメントアウトしておくこと。`parse_example_config` は `Config` 全体を比較しており、`location` は `None` のままでなければ落ちる。テスト側の `expected` に `location: None` を追加する。

- [ ] **Step 5: テストが通ることを確認する**

```
cargo test -p kgd config
```

期待: 新規 3 件を含め、すべて PASS。

- [ ] **Step 6: コミット**

```bash
just validate
git add crates/kgd/src/config config.example.toml
git -c commit.gpgsign=false commit -m "feat(config): 位置情報受信の [location] セクションを追加する

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
```

---

### Task 5: HTTP のルータ

**Files:**
- Create: `crates/kgd-presentation/src/owntracks/mod.rs`
- Create: `crates/kgd-presentation/src/owntracks/auth.rs`
- Create: `crates/kgd-presentation/src/owntracks/tests.rs`
- Modify: `crates/kgd-presentation/src/lib.rs`
- Modify: `crates/kgd-presentation/Cargo.toml`
- Modify: `Cargo.toml`

**Interfaces:**
- Consumes: `RecordLocationUseCase` (Task 2)、`sanitize_identifier` (Task 1)
- Produces:
  - `pub struct OwnTracksControllerSettings { pub username: String, pub password: String }`
  - `pub fn owntracks_router(use_case: Arc<RecordLocationUseCase>, settings: OwnTracksControllerSettings) -> axum::Router`
  - `pub fn is_valid_basic_auth(header: Option<&str>, username: &str, password: &str) -> bool` (auth.rs、純粋関数)

- [ ] **Step 1: 依存を追加する**

`Cargo.toml` の `[workspace.dependencies]` に追加する。

```toml
# HTTP server (OwnTracks receiver)
axum = "0.8"

# Basic auth decoding
base64 = "0.23"

# HTTP service testing
tower = "0.5"
```

`crates/kgd-presentation/Cargo.toml` の `[dependencies]` に `axum.workspace = true`、`base64.workspace = true`、`serde_json.workspace = true` を追加し、`[dev-dependencies]` に `tower.workspace = true` と `tokio = { workspace = true, features = ["macros", "rt-multi-thread"] }` を追加する。

- [ ] **Step 2: 認証の失敗するテストを書く**

`crates/kgd-presentation/src/owntracks/auth.rs` を新規作成する。

```rust
//! Basic 認証ヘッダの検証。

#[cfg(test)]
mod tests {
    use super::*;

    /// 正しい資格情報の Basic ヘッダを受理することを確認する。
    #[test]
    fn is_valid_basic_auth_accepts_matching_credentials() {
        // "ekuinox:secret" の base64
        let header = "Basic ZWt1aW5veDpzZWNyZXQ=";
        assert!(is_valid_basic_auth(Some(header), "ekuinox", "secret"));
    }

    /// パスワードが異なる場合に拒否することを確認する。
    #[test]
    fn is_valid_basic_auth_rejects_wrong_password() {
        let header = "Basic ZWt1aW5veDpzZWNyZXQ=";
        assert!(!is_valid_basic_auth(Some(header), "ekuinox", "other"));
    }

    /// ヘッダが無い場合に拒否することを確認する。
    #[test]
    fn is_valid_basic_auth_rejects_missing_header() {
        assert!(!is_valid_basic_auth(None, "ekuinox", "secret"));
    }

    /// Basic 以外のスキームや壊れた base64 を拒否することを確認する。
    ///
    /// 外部に公開する口であり、解釈に失敗したものは通さない。
    #[test]
    fn is_valid_basic_auth_rejects_malformed_header() {
        assert!(!is_valid_basic_auth(Some("Bearer token"), "ekuinox", "secret"));
        assert!(!is_valid_basic_auth(Some("Basic !!!!"), "ekuinox", "secret"));
        assert!(!is_valid_basic_auth(Some("Basic"), "ekuinox", "secret"));
    }
}
```

- [ ] **Step 3: テストが失敗することを確認する**

```
cargo test -p kgd-presentation owntracks::auth
```

期待: コンパイルエラー (`is_valid_basic_auth` が見つからない)。

- [ ] **Step 4: 認証を実装する**

`auth.rs` のテストより上に追記する。

```rust
use base64::{Engine as _, engine::general_purpose::STANDARD};

/// Basic 認証ヘッダが設定の資格情報と一致するか判定する。
///
/// 解釈に失敗したヘッダはすべて不一致として扱う。
pub fn is_valid_basic_auth(header: Option<&str>, username: &str, password: &str) -> bool {
    let Some(encoded) = header.and_then(|value| value.strip_prefix("Basic ")) else {
        return false;
    };
    let Ok(decoded) = STANDARD.decode(encoded.trim()) else {
        return false;
    };
    let Ok(text) = String::from_utf8(decoded) else {
        return false;
    };
    let Some((user, pass)) = text.split_once(':') else {
        return false;
    };

    user == username && pass == password
}
```

- [ ] **Step 5: ルータの失敗するテストを書く**

`crates/kgd-presentation/src/owntracks/tests.rs` を新規作成する。

```rust
use std::sync::Arc;

use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use tower::ServiceExt as _;

use kgd_application::{RecordLocationUseCase, ports::MockLocationRepository};

use super::*;

/// テスト用のルータを作る。保存は常に成功する。
fn test_router() -> axum::Router {
    let mut repo = MockLocationRepository::new();
    repo.expect_insert_messages()
        .returning(|messages| Ok(messages.len()));

    owntracks_router(
        Arc::new(RecordLocationUseCase::new(Arc::new(repo))),
        OwnTracksControllerSettings {
            username: "ekuinox".to_string(),
            password: "secret".to_string(),
        },
    )
}

/// 認証付きの POST に 200 と空の JSON 配列を返すことを確認する。
///
/// 応答が空ボディや非配列だと OwnTracks が送信失敗と解釈するため。
#[tokio::test]
async fn post_pub_returns_empty_json_array() {
    let request = Request::builder()
        .method("POST")
        .uri("/pub")
        .header("Authorization", "Basic ZWt1aW5veDpzZWNyZXQ=")
        .header("Content-Type", "application/json")
        .header("X-Limit-U", "ekuinox")
        .header("X-Limit-D", "ohtori")
        .body(Body::from(r#"{"_type":"location","tst":1,"lat":1.0,"lon":2.0}"#))
        .unwrap();

    let response = test_router().oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    assert_eq!(&body[..], b"[]");
}

/// 認証ヘッダが無い POST を 401 で弾き、WWW-Authenticate を返すことを確認する。
#[tokio::test]
async fn post_pub_rejects_unauthenticated_request() {
    let request = Request::builder()
        .method("POST")
        .uri("/pub")
        .body(Body::from("{}"))
        .unwrap();

    let response = test_router().oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert!(response.headers().contains_key("WWW-Authenticate"));
}

/// 配列で送られた複数メッセージを受理することを確認する。
#[tokio::test]
async fn post_pub_accepts_array_body() {
    let request = Request::builder()
        .method("POST")
        .uri("/pub")
        .header("Authorization", "Basic ZWt1aW5veDpzZWNyZXQ=")
        .header("Content-Type", "application/json")
        .body(Body::from(
            r#"[{"_type":"location","tst":1},{"_type":"location","tst":2}]"#,
        ))
        .unwrap();

    let response = test_router().oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

/// 1 MiB を超えるボディを 413 で弾くことを確認する。
#[tokio::test]
async fn post_pub_rejects_oversized_body() {
    let oversized = "a".repeat(1024 * 1024 + 1);
    let request = Request::builder()
        .method("POST")
        .uri("/pub")
        .header("Authorization", "Basic ZWt1aW5veDpzZWNyZXQ=")
        .header("Content-Type", "application/json")
        .body(Body::from(oversized))
        .unwrap();

    let response = test_router().oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
}

/// 壊れた JSON を 400 で弾くことを確認する。
#[tokio::test]
async fn post_pub_rejects_invalid_json() {
    let request = Request::builder()
        .method("POST")
        .uri("/pub")
        .header("Authorization", "Basic ZWt1aW5veDpzZWNyZXQ=")
        .header("Content-Type", "application/json")
        .body(Body::from("{not json"))
        .unwrap();

    let response = test_router().oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

/// 死活確認の GET が 200 を返すことを確認する。
///
/// トンネルと死活監視がこの口を使う。
#[tokio::test]
async fn get_healthz_returns_ok() {
    let request = Request::builder()
        .method("GET")
        .uri("/healthz")
        .body(Body::empty())
        .unwrap();

    let response = test_router().oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}
```

- [ ] **Step 6: テストが失敗することを確認する**

```
cargo test -p kgd-presentation owntracks
```

期待: コンパイルエラー (`owntracks_router` が見つからない)。

- [ ] **Step 7: ルータを実装する**

`crates/kgd-presentation/src/owntracks/mod.rs` を新規作成する。

```rust
//! OwnTracks からの HTTP リクエストを受けてユースケースを呼び出すコントローラ。

use std::sync::Arc;

use axum::{
    Json, Router,
    body::Bytes,
    extract::{DefaultBodyLimit, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::{Value, json};
use tracing::{info, warn};

use kgd_application::RecordLocationUseCase;
use kgd_domain::sanitize_identifier;

mod auth;

#[cfg(test)]
mod tests;

use auth::is_valid_basic_auth;

/// ボディの上限 (1 MiB)。
const MAX_BODY_BYTES: usize = 1024 * 1024;

/// OwnTracks コントローラの設定。
#[derive(Debug, Clone)]
pub struct OwnTracksControllerSettings {
    /// Basic 認証のユーザー名
    pub username: String,
    /// Basic 認証のパスワード
    pub password: String,
}

/// ハンドラ間で共有する状態。
#[derive(Clone)]
struct OwnTracksState {
    /// 受信ユースケース
    use_case: Arc<RecordLocationUseCase>,
    /// 認証設定
    settings: OwnTracksControllerSettings,
}

/// URL クエリで渡される端末識別子。
///
/// OwnTracks はヘッダではなく `?u=&d=` で名乗ることがある。
#[derive(Debug, Deserialize)]
struct DeviceQuery {
    /// ユーザー識別子
    u: Option<String>,
    /// デバイス識別子
    d: Option<String>,
}

/// OwnTracks 受信用のルータを組み立てる。
pub fn owntracks_router(
    use_case: Arc<RecordLocationUseCase>,
    settings: OwnTracksControllerSettings,
) -> Router {
    Router::new()
        .route("/pub", post(handle_pub))
        .route("/healthz", get(handle_healthz))
        .layer(DefaultBodyLimit::max(MAX_BODY_BYTES))
        .with_state(OwnTracksState { use_case, settings })
}

/// 死活確認。
async fn handle_healthz() -> Json<Value> {
    Json(json!({ "ok": true }))
}

/// メッセージを受け取って保存する。
///
/// 応答は常に JSON 配列。OwnTracks は配列以外を失敗として扱うため。
async fn handle_pub(
    State(state): State<OwnTracksState>,
    Query(query): Query<DeviceQuery>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let authorization = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok());

    if !is_valid_basic_auth(
        authorization,
        &state.settings.username,
        &state.settings.password,
    ) {
        warn!("Rejected unauthenticated OwnTracks request");
        return (
            StatusCode::UNAUTHORIZED,
            [(header::WWW_AUTHENTICATE, "Basic realm=\"owntracks\"")],
        )
            .into_response();
    }

    let Ok(value) = serde_json::from_slice::<Value>(&body) else {
        warn!("Rejected malformed OwnTracks payload");
        return StatusCode::BAD_REQUEST.into_response();
    };

    let payloads = match value {
        Value::Array(values) => values,
        other => vec![other],
    };

    let user_id = sanitize_identifier(
        header_or_query(&headers, "X-Limit-U", query.u.as_deref()),
        "unknown",
    );
    let device_id = sanitize_identifier(
        header_or_query(&headers, "X-Limit-D", query.d.as_deref()),
        "device",
    );

    match state.use_case.record(&user_id, &device_id, payloads).await {
        Ok(outcome) => {
            info!(
                user_id,
                device_id,
                parsed = outcome.parsed,
                stored = outcome.stored,
                skipped = outcome.skipped,
                "Recorded OwnTracks messages"
            );
        }
        Err(error) => {
            // 端末側に再送させても直らないため、記録に失敗しても 200 を返す。
            // 失われるのは 1 バッチぶんで、端末のキューは次の送信で流れる。
            warn!(?error, "Failed to record OwnTracks messages");
        }
    }

    Json(Vec::<Value>::new()).into_response()
}

/// ヘッダを優先し、無ければクエリの値を使う。
fn header_or_query<'a>(
    headers: &'a HeaderMap,
    name: &str,
    fallback: Option<&'a str>,
) -> Option<&'a str> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .or(fallback)
}
```

`crates/kgd-presentation/src/lib.rs` に `mod owntracks;` と `pub use owntracks::{OwnTracksControllerSettings, owntracks_router};` を追加する。

- [ ] **Step 8: テストが通ることを確認する**

```
cargo test -p kgd-presentation owntracks
```

期待: 認証 4 件 + ルータ 6 件が PASS。

`post_pub_rejects_oversized_body` が 413 ではなく他のステータスで落ちる場合は、`DefaultBodyLimit::max` の位置が `with_state` より後ろになっていないか確認する。

- [ ] **Step 9: コミット**

```bash
just validate
git add Cargo.toml crates/kgd-presentation
git -c commit.gpgsign=false commit -m "feat(presentation): OwnTracks の HTTP 受け口を追加する

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
```

---

### Task 6: サーバーの起動と配線

**Files:**
- Create: `crates/kgd-infrastructure/src/http_server.rs`
- Modify: `crates/kgd-infrastructure/src/lib.rs`
- Modify: `crates/kgd-infrastructure/Cargo.toml`
- Modify: `crates/kgd/src/bootstrap.rs`

**Interfaces:**
- Consumes: `owntracks_router` (Task 5)、`connect_pool` / `LocationStore` (Task 3)、`LocationConfig` (Task 4)
- Produces: `pub async fn serve_http(listen: SocketAddr, router: axum::Router) -> anyhow::Result<()>`

- [ ] **Step 1: サーバーのランナーを書く**

`crates/kgd-infrastructure/Cargo.toml` の `[dependencies]` に `axum.workspace = true` を追加し、`tokio` に `net` feature を足す (`tokio = { workspace = true, features = ["net"] }`)。

`crates/kgd-infrastructure/src/http_server.rs` を新規作成する。

```rust
//! HTTP サーバーの常駐タスク。

use std::net::SocketAddr;

use anyhow::{Context as _, Result};
use axum::Router;
use tokio::net::TcpListener;
use tracing::info;

/// 指定アドレスで HTTP サーバーを起動し、終了するまで待つ。
pub async fn serve_http(listen: SocketAddr, router: Router) -> Result<()> {
    let listener = TcpListener::bind(listen)
        .await
        .with_context(|| format!("Failed to bind {listen}"))?;

    info!(%listen, "HTTP server started");

    axum::serve(listener, router)
        .await
        .context("HTTP server error")
}
```

`crates/kgd-infrastructure/src/lib.rs` に `mod http_server;` と `pub use http_server::serve_http;` を追加する。

- [ ] **Step 2: bootstrap を書き換える**

`crates/kgd/src/bootstrap.rs` のストア生成部を次の形にする (Task 3 で `connect_pool` へ切り替え済みであれば差分は `LocationStore` と HTTP サーバーの追加のみ)。

```rust
    let pool = connect_pool(&diary_config.database_url)
        .await
        .context("Failed to connect to database")?;
    let diary_store: Arc<dyn DiaryRepository> = Arc::new(DiaryStore::new(pool.clone()));
```

`client.start()` の直前に追加する。

```rust
    // 位置情報の受け口。設定が無ければ起動しない。
    if let Some(location_config) = config.location.clone() {
        let location_store: Arc<dyn LocationRepository> =
            Arc::new(LocationStore::new(pool.clone()));
        let record_location = Arc::new(RecordLocationUseCase::new(location_store));
        let router = owntracks_router(
            record_location,
            OwnTracksControllerSettings {
                username: location_config.username.clone(),
                password: location_config.password.clone(),
            },
        );
        let listen = location_config.listen;
        tokio::spawn(async move {
            if let Err(error) = serve_http(listen, router).await {
                tracing::error!(?error, "OwnTracks HTTP server stopped");
            }
        });
        info!(%listen, "OwnTracks receiver started");
    }
```

`use` の追加:

```rust
use kgd_application::{
    // 既存に加えて
    RecordLocationUseCase,
    ports::LocationRepository,
};
use kgd_infrastructure::{
    // 既存に加えて
    LocationStore, connect_pool, serve_http,
};
use kgd_presentation::{
    // 既存に加えて
    OwnTracksControllerSettings, owntracks_router,
};
```

- [ ] **Step 3: ビルドと全テストが通ることを確認する**

```
cargo check --all-targets
cargo test --all
```

期待: どちらも成功。

- [ ] **Step 4: 手で疎通を確認する**

PostgreSQL を起動し、`[location]` を書いた `config.toml` で kgd を起動する。

```bash
just compose-local
```

別のシェルから確認する。

```bash
curl -s localhost:8081/healthz
curl -s -o /dev/null -w '%{http_code}\n' -X POST localhost:8081/pub -d '{}'
curl -s -u ekuinox:CHANGE_ME -X POST localhost:8081/pub \
  -H 'X-Limit-U: ekuinox' -H 'X-Limit-D: test' \
  -d '{"_type":"location","tst":'$(date +%s)',"lat":35.0,"lon":135.0,"batt":50}'
```

期待: 順に `{"ok":true}`、`401`、`[]`。3 つ目の後にテーブルへ 1 行入っていること。

```bash
docker compose exec db psql -U kgd -c "SELECT user_id, device_id, msg_type, tst, lat, lon FROM owntracks_messages;"
```

- [ ] **Step 5: コミット**

```bash
just validate
git add crates/kgd-infrastructure crates/kgd/src/bootstrap.rs
git -c commit.gpgsign=false commit -m "feat: OwnTracks の受け口を起動時に配線する

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
```

---

### Task 7: JSONL の取り込み

**Files:**
- Create: `crates/kgd/src/import.rs`
- Modify: `crates/kgd/src/main.rs`
- Test: `crates/kgd/src/import.rs` (同一ファイル内 `mod tests`)

**Interfaces:**
- Consumes: `RecordLocationUseCase` (Task 2)、`connect_pool` / `LocationStore` (Task 3)
- Produces:
  - `pub fn collect_jsonl_files(paths: &[PathBuf]) -> Result<Vec<PathBuf>>`
  - `pub fn parse_jsonl_line(line: &str) -> Option<Value>`
  - `pub async fn run_import(config: &Config, paths: &[PathBuf]) -> Result<()>`

- [ ] **Step 1: 失敗するテストを書く**

`crates/kgd/src/import.rs` を新規作成する。

```rust
//! JSONL に記録された OwnTracks メッセージの取り込み。

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    /// ディレクトリを渡すと配下の .jsonl を再帰的に集め、
    /// それ以外の拡張子を無視することを確認する。
    #[test]
    fn collect_jsonl_files_walks_directories() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("ekuinox-ohtori");
        fs::create_dir_all(&nested).unwrap();
        fs::write(nested.join("2026-09-13.jsonl"), "").unwrap();
        fs::write(nested.join("README.md"), "").unwrap();

        let files = collect_jsonl_files(&[dir.path().to_path_buf()]).unwrap();

        assert_eq!(files.len(), 1);
        assert!(files[0].ends_with("2026-09-13.jsonl"));
    }

    /// ファイルを直接渡した場合はそのまま対象になることを確認する。
    #[test]
    fn collect_jsonl_files_accepts_direct_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("a.jsonl");
        fs::write(&file, "").unwrap();

        let files = collect_jsonl_files(&[file.clone()]).unwrap();

        assert_eq!(files, vec![file]);
    }

    /// 空行と壊れた行を読み飛ばし、正しい行だけを返すことを確認する。
    ///
    /// 途中で壊れた行があっても取り込みを止めないため。
    #[test]
    fn parse_jsonl_line_skips_blank_and_broken_lines() {
        assert!(parse_jsonl_line("").is_none());
        assert!(parse_jsonl_line("   ").is_none());
        assert!(parse_jsonl_line("{not json").is_none());
        assert!(parse_jsonl_line(r#"{"_type":"location"}"#).is_some());
    }
}
```

`crates/kgd/Cargo.toml` の `[dev-dependencies]` に `tempfile.workspace = true` を追加する (無ければセクションごと作る)。

- [ ] **Step 2: テストが失敗することを確認する**

```
cargo test -p kgd import
```

期待: コンパイルエラー (`collect_jsonl_files` が見つからない)。

- [ ] **Step 3: 実装を書く**

`crates/kgd/src/import.rs` のテストより上に追記する。

```rust
use std::{
    fs::{self, File},
    io::{BufRead as _, BufReader},
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::{Context as _, Result};
use serde_json::Value;
use tracing::{info, warn};

use kgd_application::{RecordLocationUseCase, ports::LocationRepository};
use kgd_domain::sanitize_identifier;
use kgd_infrastructure::{LocationStore, connect_pool};

use crate::config::Config;

/// 取り込み対象の JSONL ファイルを集める。
///
/// ディレクトリは再帰的に辿り、拡張子が `jsonl` のものだけを対象にする。
pub fn collect_jsonl_files(paths: &[PathBuf]) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for path in paths {
        collect_into(path, &mut files)
            .with_context(|| format!("Failed to scan {}", path.display()))?;
    }
    files.sort();
    Ok(files)
}

fn collect_into(path: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    if path.is_dir() {
        for entry in fs::read_dir(path)? {
            collect_into(&entry?.path(), files)?;
        }
    } else if path.extension().is_some_and(|ext| ext == "jsonl") {
        files.push(path.to_path_buf());
    }
    Ok(())
}

/// JSONL の 1 行を JSON として解釈する。空行や壊れた行は `None` を返す。
pub fn parse_jsonl_line(line: &str) -> Option<Value> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }
    serde_json::from_str(trimmed).ok()
}

/// JSONL を読み込んでデータベースへ取り込む。
pub async fn run_import(config: &Config, paths: &[PathBuf]) -> Result<()> {
    let files = collect_jsonl_files(paths)?;
    info!(files = files.len(), "Importing OwnTracks JSONL");

    let pool = connect_pool(&config.diary.database_url)
        .await
        .context("Failed to connect to database")?;
    let store: Arc<dyn LocationRepository> = Arc::new(LocationStore::new(pool));
    let use_case = RecordLocationUseCase::new(store);

    let mut parsed_total = 0usize;
    let mut stored_total = 0usize;
    let mut skipped_total = 0usize;

    for file in files {
        // ディレクトリ名 <user>-<device> から端末を復元する。
        // 受信時は X-Limit-U / X-Limit-D から取るが、JSONL には残っていない。
        let (user_id, device_id) = device_from_path(&file);

        let reader = BufReader::new(
            File::open(&file).with_context(|| format!("Failed to open {}", file.display()))?,
        );

        let mut payloads = Vec::new();
        for line in reader.lines() {
            let line = line.with_context(|| format!("Failed to read {}", file.display()))?;
            match parse_jsonl_line(&line) {
                Some(value) => payloads.push(value),
                None => skipped_total += 1,
            }
        }

        let outcome = use_case.record(&user_id, &device_id, payloads).await?;
        parsed_total += outcome.parsed;
        stored_total += outcome.stored;
        skipped_total += outcome.skipped;

        info!(
            file = %file.display(),
            user_id,
            device_id,
            parsed = outcome.parsed,
            stored = outcome.stored,
            "Imported file"
        );
    }

    info!(
        parsed = parsed_total,
        stored = stored_total,
        skipped = skipped_total,
        "Import finished"
    );

    Ok(())
}

/// `<user>-<device>/<date>.jsonl` の親ディレクトリ名から端末を復元する。
fn device_from_path(path: &Path) -> (String, String) {
    let directory = path
        .parent()
        .and_then(Path::file_name)
        .and_then(|name| name.to_str())
        .unwrap_or_default();

    match directory.split_once('-') {
        Some((user, device)) => (
            sanitize_identifier(Some(user), "unknown"),
            sanitize_identifier(Some(device), "device"),
        ),
        None => ("unknown".to_string(), "device".to_string()),
    }
}
```

`device_from_path` のテストを `mod tests` に追加する。

```rust
    /// ディレクトリ名 <user>-<device> から端末を復元することを確認する。
    ///
    /// JSONL には X-Limit-U / X-Limit-D が残っていないため、
    /// 受け口が付けたディレクトリ名が唯一の手がかりになる。
    #[test]
    fn device_from_path_splits_directory_name() {
        let path = PathBuf::from("data/ekuinox-ohtori/2026-09-13.jsonl");
        assert_eq!(
            device_from_path(&path),
            ("ekuinox".to_string(), "ohtori".to_string())
        );
    }

    /// 区切りが無いディレクトリ名では既定値を返すことを確認する。
    #[test]
    fn device_from_path_falls_back_without_separator() {
        let path = PathBuf::from("data/plain/2026-09-13.jsonl");
        assert_eq!(
            device_from_path(&path),
            ("unknown".to_string(), "device".to_string())
        );
    }
```

- [ ] **Step 4: サブコマンドを追加する**

`crates/kgd/src/main.rs` を書き換える。

```rust
mod bootstrap;
mod config;
mod import;
mod version;
```

```rust
use clap::{Parser, Subcommand};
```

```rust
#[derive(Parser)]
#[command(version = short_version())]
struct Args {
    #[arg(long, default_value = "config.toml")]
    config: PathBuf,

    #[arg(long)]
    init: bool,

    #[command(subcommand)]
    command: Option<Command>,
}

/// 常駐以外の実行モード。
#[derive(Subcommand)]
enum Command {
    /// OwnTracks の JSONL をデータベースへ取り込む
    ImportOwntracks {
        /// 取り込む JSONL ファイルまたはディレクトリ
        #[arg(required = true)]
        paths: Vec<PathBuf>,
    },
}
```

`main` の `args.init` の分岐の後、`tracing::info!(version = ...)` の後ろに追加する。

```rust
    let config = open_config(&args.config).context("Failed to load configuration")?;
    info!(servers = config.servers.len(), "Configuration loaded");

    if let Some(Command::ImportOwntracks { paths }) = &args.command {
        return import::run_import(&config, paths).await;
    }
```

既存の `open_config` 呼び出しは 1 つに統合し、重複させないこと。

- [ ] **Step 5: テストが通ることを確認する**

```
cargo test -p kgd import
cargo check --all-targets
```

期待: 5 件 PASS、ビルド成功。

- [ ] **Step 6: 実データで取り込みを確認する**

aoi 上の JSONL を読み取り専用でマウントして実行する。

```bash
docker compose run --rm -v ~/services/owntracks/data:/import:ro kgd import-owntracks /import
```

期待: 取り込み件数がログに出る。**同じコマンドを 2 回実行し、2 回目の `stored` が 0 になることを確認する** (冪等性の確認)。

- [ ] **Step 7: コミット**

```bash
just validate
git add crates/kgd/src/import.rs crates/kgd/src/main.rs crates/kgd/Cargo.toml
git -c commit.gpgsign=false commit -m "feat: OwnTracks の JSONL を取り込むサブコマンドを追加する

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
```

---

### Task 8: ドキュメントの更新

**Files:**
- Modify: `docs/architecture.md`
- Modify: `README.md`

**Interfaces:**
- Consumes: Task 1〜7 の成果
- Produces: なし

- [ ] **Step 1: architecture.md の表を更新する**

「ポートと実装の対応」の表に 1 行追加する。

```markdown
| LocationRepository | LocationStore (sqlx / PostgreSQL) | MockLocationRepository |
```

「ユースケース一覧」の表に 1 行追加する。

```markdown
| RecordLocationUseCase | OwnTracks から受信したメッセージを解釈して保存する |
```

層構成の mermaid 図の `PRES` ノードの説明を `Controller (DiscordController / OwnTracksController) / Presenter` に変える。

- [ ] **Step 2: README.md に機能の説明を追加する**

位置情報の受信について、有効化の方法 (`[location]` セクション) と取り込みコマンドを数行で書く。

- [ ] **Step 3: CI と同じ検査を通す**

```
just ci
```

期待: fmt-check / check / clippy / deny / machete / test がすべて成功。

`cargo deny` が axum / base64 / tower / tiny-skia の推移的依存で落ちた場合は、落ちたクレートとライセンスを報告して停止する。許可リストを勝手に広げない。

`cargo machete` が `base64` や `tower` を未使用と誤検知した場合は、該当 crate の `Cargo.toml` に `[package.metadata.cargo-machete] ignored = [...]` を足す。

- [ ] **Step 4: コミット**

```bash
git add docs/architecture.md README.md
git -c commit.gpgsign=false commit -m "docs: 位置情報の受信機能をアーキテクチャ文書に反映する

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
```

---

## 完了の定義

- `just ci` が通る
- `[location]` を書いた設定で kgd を起動すると 8081 で待ち受け、認証付き POST に `[]` を返し、行がテーブルに入る
- 同じ点を 2 回送っても行が増えない
- `[location]` を書かない既存の設定ファイルがそのまま動く
- `kgd import-owntracks` を 2 回実行しても 2 回目は 0 件

## この計画に含まれないもの

次は後続の計画 (日次レポートと死活監視) で扱う。

- 地図画像の生成と Discord への投稿 (tiny-skia、OSM タイル)
- 位置情報が途絶えたときの通知
- `owntracks_report_posts` テーブル
- `DiscordGateway` への画像送信メソッドの追加
- 名前付きトンネルへの切り替え
