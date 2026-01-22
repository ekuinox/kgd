# Development Rules

## コミット前のチェック

コミットを作成する前に、必ず以下のコマンドを実行してください。

### Just を使用する場合 (推奨)

```bash
just validate
```

このコマンドは自動的に以下を実行します:

- `cargo fmt` - コードフォーマット
- `cargo check` - ビルドチェック
- `cargo clippy` - Lintチェック

### 個別に実行する場合

```bash
cargo fmt
cargo check
cargo clippy
```

すべてのチェックが通過することを確認してからコミットしてください。

### その他の便利なコマンド

```bash
just test          # テストを実行
just ci            # CI環境と同じチェック (fmt check, check, clippy, test)
just build         # リリースビルド
just run           # Discord Botを起動 (開発モード)
just run-release   # Discord Botを起動 (リリースモード)
```

利用可能なすべてのコマンドを確認するには `just` または `just --list` を実行してください。

## 依存の管理

### バージョン選択

- 新しい依存を追加する際は、極力最新のバージョンを調べて使用すること
- crates.io で最新の安定版を確認する
- メジャーバージョンの変更には注意し、CHANGELOG を確認する

### Workspace での共有

- 依存は原則として workspace 間で共有すること
- 共通の依存は `Cargo.toml` の `[workspace.dependencies]` に定義する
- 各クレートでは `workspace = true` を使用して参照する

### 依存追加の例

```toml
# Workspace Cargo.toml
[workspace.dependencies]
new-crate = "1.0"

# Individual crate Cargo.toml
[dependencies]
new-crate.workspace = true
```

`{ workspace = true }` のようなインラインテーブルを使用するのは必要になったときのみで、テーブル内で指定する値が一つの場合は `.` で代入すること

## Rust コードを書くとき

### コード品質

- Clippy の警告は可能な限り解消すること
- 不要な警告を無視する場合は、理由をコメントで明記すること
- コードは `cargo fmt` でフォーマットすること

### コメント

構造体やフィールドについては doc コメントを使って、対象の目的や使い方などを記載する

```rust
/// ここ
struct Foo {
    /// ここ
    count: usize,
}
```

### import の書き方

- ブロックを分けて `use` する
  - `std`
  - `anyhow` や `clap` などの外部クレート
  - 自クレートからの依存 `crate`
  - 親からの依存 `super`
  - 自身からの依存 `self`
- トレイトメソッドのみを使用する場合の `use` は `as _;` を使う
- 同じクレートから `use` する場合はマージして書く

以下はこのプロジェクトが理想とする `use` の書き方

```rust
use std::{time::Duration, path::PathBuf};

use anyhow::{Context as _, Result};
use clap::Parser;
use tokio::sync::mpsc;
use tracing::info;

use crate::config::open_config;

use self::foo::FooState;
```

### モジュール内の書き方

外部に公開している関数や構造体をファイルの上部に置き、公開しないものを下部に配置すること

```rust
// 公開する単純な関数が一番上
pub fn create(value: usize) -> State {
    State::new(value)
}

// 公開する構造体
pub struct State {
    count: usize,
    text: String,
}

// 構造体への実装は構造体のすぐ下
impl State {
    pub fn new(count: usize) -> State {
        State::with_text(count, Default::default())
    }
    pub fn with_text(count: usize, text: String) -> State {
        State { count, text }
    }
    pub fn state_text(&self) -> String {
        create_text(count, &text)
    }
}

// 構造体へのトレイト実装は構造体の下
impl Default for State {
    fn default() -> State {
        todo!()
    }
}

// 公開しない構造体
struct Foo;

// 外部に公開しない関数
fn create_text(count: usize, text: &str) -> String {
    let suffix = create_suffix();
    todo!()
}

// create_text の内部で呼び出されるだけなので更に下
fn create_suffix() -> String {
    todo!()
}
```

外部から使用される関数、構造体などをなるべくファイルの先頭よりに実装する

`fn main()` からの距離が近いものほど浅い位置になるように

### 単体テスト

なるべく単体テストを実装すること

単体テストは `mod tests` 内に実装する

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_foo() {
        assert!(foo());
    }
}
```

### エラーメッセージの記載について

- tracing や標準エラー出力などの開発者向けのエラーメッセージについては原則、英文で記載すること
- ユーザーの目にはいる Discord のメッセージなどは日本語文で記載すること

### 型境界の書き方について

- できるだけ `impl T` を使う
- `impl` を使えない場合、 `where` を使って境界を示す
- 構造体メンバのジェネリクスは必要な箇所だけ境界を指定する

```rust
fn f0(text: impl AsRef<str>) {
    unreachable!()
}

fn f1<T>(text: T) -> T where T: AsRef<str> {
    unreachable!()
}

struct Foo<T> {
    x: T,
}

impl<T> Foo<T> {
    /// new 自体は境界を設定しない
    fn new(x: T) -> Foo<T> {
        todo!()
    }
}

impl<T> Foo<T> where T: AsRef<str> {
    /// 文字列を取得するところは絞り込む
    fn text(&self) -> &str {
        self.x.as_ref()
    }
}
```
