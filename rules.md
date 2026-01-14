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
new-crate = { workspace = true }
```

## コード品質

- Clippy の警告は可能な限り解消すること
- 不要な警告を無視する場合は、理由をコメントで明記すること
- コードは `cargo fmt` でフォーマットすること
