# kgd

[![codecov](https://codecov.io/gh/ekuinox/kgd/graph/badge.svg)](https://codecov.io/gh/ekuinox/kgd)

ekuinox 自身のためにいろいろやってもらう bot 的な何か

Discord Bot として機能して、ローカルのサーバーの起動や Notion のデータベース管理などを行いたい

## 機能

- ローカルにあるサーバーの起動と起動状況確認
- Discord フォーラムでの日報作成
- OwnTracks からの位置情報の受信と保存

## 開発環境

- Rust 1.92
- Just 1.45
- Docker 27.2

設定は `config.example.toml` を参考に `config.toml` を作成する

Discord と Notion の bot トークンが必要。

```bash
# ローカルのネイティブで kgd を起動する
just run

# ローカルで docker compose を使って起動する
just compose-local
```

## 位置情報の受信 (OwnTracks)

[OwnTracks](https://owntracks.org/) アプリからの位置情報を HTTP で受信し、日報用の PostgreSQL データベースに保存する。

有効化するには `config.toml` に `[location]` セクションを追加する（セクションごと省略すると機能は無効のまま）。

```toml
[location]
# HTTP 受け口の待ち受けアドレス (省略時: 0.0.0.0:8081)
listen = "0.0.0.0:8081"
# OwnTracks アプリの UserID / Password に一致させる Basic 認証
username = "ekuinox"
password = "CHANGE_ME"
```

指定した `listen` に bind できない場合、kgd は起動処理を中断してエラー終了する（bind 後の実行時エラーはログに記録して動作を継続する）。OwnTracks アプリの endpoint には `http://<host>:8081/pub` を設定し、同じ username/password を Basic 認証として登録する。

過去にエクスポートした JSONL を取り込むには `import-owntracks` サブコマンドを使う。

```bash
just run import-owntracks ~/owntracks-export/
```

ファイルまたはディレクトリのどちらも指定でき、ディレクトリを渡した場合は配下の `*.jsonl` を再帰的に取り込む。OwnTracks の書き出しレイアウト `<user>-<device>/<date>.jsonl` の親ディレクトリ名からユーザー・端末識別子を復元するため、そのディレクトリ構成のまま渡すこと。壊れた行は読み飛ばし、最後に「取り込み / 既存 / スキップ」の件数を表示する。データベース接続は `[diary].database_url` を使うため `[location]` セクションは無くても実行できる。

同じ JSONL を再度取り込んでも重複行は増えない（`user_id, device_id, msg_type, tst` が一致する行は無視される）。ただし `tst` を持たないメッセージ（`waypoints` など）は重複判定できず、再実行のたびに増え続けるので注意。

## テストカバレッジ

カバレッジは [Codecov](https://codecov.io/gh/ekuinox/kgd) で確認できる（PR には自動でカバレッジコメントが付く）。

ローカルで HTML レポートを生成する場合:

```bash
cargo install cargo-llvm-cov  # 初回のみ
just cov
```
