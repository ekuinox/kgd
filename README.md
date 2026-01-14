# KGD (Kuinox General Dashboard)

自宅サーバー管理用のDiscord Bot。Wake-on-LAN (WOL) 機能を提供します。

## 機能

- **Wake-on-LAN**: Discord のスラッシュコマンドからサーバーを起動
- **サーバー一覧**: 設定されているサーバーの情報を表示
- **拡張可能な設計**: 将来的にWebダッシュボードなども追加可能なworkspace構成

## 必要な環境

- Rust (最新の安定版)
- Discord Bot Token
- Wake-on-LANに対応したサーバー

## セットアップ

### 1. Discord Bot の作成

1. [Discord Developer Portal](https://discord.com/developers/applications) にアクセス
2. "New Application" をクリックして新しいアプリケーションを作成
3. "Bot" タブに移動して Bot を作成
4. Bot Token をコピー (後で使用)
5. "OAuth2" → "URL Generator" で以下を選択:
   - SCOPES: `bot`, `applications.commands`
   - BOT PERMISSIONS: `Send Messages`, `Use Slash Commands`
6. 生成されたURLからBotをサーバーに招待

### 2. 設定ファイルの作成

```bash
cp config.example.toml config.toml
```

`config.toml` を編集して、Discord Bot Token と管理したいサーバーの情報を記入:

```toml
[discord]
token = "YOUR_DISCORD_BOT_TOKEN"

[[servers]]
name = "Main Server"
mac_address = "AA:BB:CC:DD:EE:FF"
ip_address = "192.168.1.100"
description = "メインサーバー"
```

### 3. ビルドと実行

#### Just を使用する場合 (推奨)

```bash
# Bot を起動 (開発モード)
just run

# Bot を起動 (リリースモード)
just run-release

# または、リリースビルドしてから実行
just build
./target/release/kgd-bot
```

#### Cargo を直接使用する場合

```bash
# ビルド
cargo build --release

# 実行
cargo run --bin kgd-bot --release
```

## 使い方

Botが起動したら、Discordで以下のスラッシュコマンドが使えます:

### `/wol <server_name>`

指定したサーバーにWOLパケットを送信して起動します。

例: `/wol Main Server`

### `/servers`

設定されているすべてのサーバーの情報を表示します。

## プロジェクト構成

```
kgd/
├── Cargo.toml              # Workspace 設定
├── CONTRIBUTING.md         # 開発ルール
├── config.example.toml     # 設定ファイルのサンプル
├── .gitignore
└── crates/
    ├── bot/                # Discord Bot 実装
    ├── core/               # 共通ロジック (WOL など)
    └── config/             # 設定管理
```

## 開発

開発に関する詳細は [CONTRIBUTING.md](CONTRIBUTING.md) を参照してください。

### Just のインストール

このプロジェクトでは [Just](https://github.com/casey/just) コマンドランナーを使用します。

```bash
# Cargo経由でインストール
cargo install just

# または、各OSのパッケージマネージャーでインストール
# macOS
brew install just

# Windows (Scoop)
scoop install just

# Windows (Chocolatey)
choco install just
```

### コミット前のチェック

#### Just を使用する場合 (推奨)

```bash
just validate
```

これにより自動的に `cargo fmt`, `cargo check`, `cargo clippy` が実行されます。

#### Cargo を直接使用する場合

```bash
cargo fmt
cargo check
cargo clippy
```

### その他の便利なコマンド

```bash
just              # 利用可能なコマンド一覧を表示
just test         # テストを実行
just ci           # CI環境と同じチェックを実行
just clean        # ビルド成果物を削除
just outdated     # 古い依存をチェック
just update       # 依存を更新
```

## ライセンス

このプロジェクトは個人使用を目的としています。

## トラブルシューティング

### WOLパケットが届かない

- サーバーのWOL設定が有効になっているか確認
- ファイアウォールがUDPポート9を許可しているか確認
- 同じネットワーク内から実行しているか確認
- BIOSでWake-on-LANが有効になっているか確認

### Discord Botが応答しない

- Bot Tokenが正しいか確認
- Botがサーバーに招待されているか確認
- Botが起動しているか確認
- インターネット接続を確認

## 将来の拡張

- Webダッシュボード
- サーバー監視機能
- Docker コンテナ管理
- 通知機能
