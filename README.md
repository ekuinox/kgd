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

### インターネットからの到達 (Cloudflare トンネル)

端末が外出先から受け口へ届くように、Cloudflare の名前付きトンネルを compose に同梱している。トンネルの作成には `cloudflared tunnel login` 済みのマシンが要る。

```bash
cloudflared tunnel create kgd-owntracks
cloudflared tunnel route dns kgd-owntracks <ホスト名>
```

作成すると `~/.cloudflared/<UUID>.json` に認証情報が出力される。これと ingress 設定を、kgd を動かすホストの `cloudflared/` ディレクトリに置く。

```bash
mkdir -p cloudflared
cp cloudflared.example/config.yml cloudflared/config.yml
cp ~/.cloudflared/<UUID>.json cloudflared/
# config.yml の tunnel / credentials-file / hostname を実際の値に書き換える
chmod 750 cloudflared && chmod 640 cloudflared/*
cp .env.example .env
# CLOUDFLARED_GID に `stat -c '%g' cloudflared` の値を書く
docker compose --profile tunnel up -d
```

`cloudflared/` は認証情報を含むため git 管理外。コンテナはイメージ既定の nonroot ユーザー (65532) のまま動き、グループだけホストに合わせて設定を読む。`CLOUDFLARED_GID` がホストの GID と合っていないと `permission denied` で起動に失敗する。

トンネルは `tunnel` プロファイルに属するため、プロファイルを指定しない `docker compose up` では起動しない。トンネルを使わない環境でも、プロファイルを省けば kgd と PostgreSQL は通常どおり起動する。

ingress 設定をダッシュボードではなくファイルで持つのは、`cloudflared tunnel create` で作ったトンネルがダッシュボードから ingress を受け取れないため。設定が無いと全てのリクエストに 503 を返す。

Cloudflare Access は使わない。OwnTracks はブラウザではないため Access のログイン画面を通過できない。公開 URL を守るのは `[location]` の Basic 認証のみになる。

同じポートには位置ログのビューア (`/viewer/`) も載る。ビューアにはログインが無いため、ingress の `path: ^/(pub|healthz)$` で OwnTracks の受け口だけをトンネルに通す。雛形 (`cloudflared.example/config.yml`) より前に作った `cloudflared/config.yml` にはこの行が無いので、足してから cloudflared を再起動すること。

それとは別に、以前運用していた HTTP 受け口が書き溜めた JSONL ログを取り込むには `import-owntracks` サブコマンドを使う。

```bash
just run import-owntracks /path/to/location-logs/
```

ファイルまたはディレクトリのどちらも指定でき、ディレクトリを渡した場合は配下の `*.jsonl` を再帰的に取り込む。`<user>-<device>/<date>.jsonl` という親ディレクトリ名からユーザー・端末識別子を復元するため、そのディレクトリ構成のまま渡すこと。壊れた行は読み飛ばし、終了時に取り込み件数・既存件数・スキップ件数 (`imported` / `existing` / `skipped`) をログへ出力する。データベース接続は `[diary].database_url` を使うため `[location]` セクションは無くても実行できる。

同じ JSONL を再度取り込んでも重複行は増えない（`user_id, device_id, msg_type, tst` が一致する行は無視される）。ただし `tst` を持たないメッセージ（`waypoints` など）は重複判定できず、再実行のたびに増え続けるので注意。

### 位置ログのビューア

記録した軌跡と集計を、LAN の中からブラウザで見られる。`[location]` に `[location.viewer]` を足すと有効になり、`http://<host>:8081/viewer/` で開ける。

```toml
[location.viewer]
# ビューアに届いてよい送信元 (省略時: LAN のプライベート帯。loopback は含まない)
allowed_cidrs = ["192.168.0.0/16"]
# IP アドレス以外で Host に来てよい名前 (省略時: 空。IP アドレスで開くなら不要)
allowed_hosts = ["aoi.local"]
# 地図に返す軌跡の点数の上限 (省略時: 20000)
max_track_points = 20000
```

ビューアにはログインが無い。代わりに、送信元が `allowed_cidrs` に無いリクエストと、Cloudflare を経由したリクエスト (`Cf-Connecting-IP` などのヘッダを持つもの) を 403 で拒否する。同じホストの cloudflared は 127.0.0.1 から接続してくるため、本番では `allowed_cidrs` に loopback を入れないこと。

DNS rebinding (攻撃者のページが自分の名前を kgd の LAN のアドレスへ向け直し、LAN のブラウザに API を読ませる攻撃) を防ぐため、`Host` が IP アドレスでも `allowed_hosts` にある名前でもないリクエストも 403 で拒否する。`http://192.168.1.5:8081/viewer/` のように IP アドレスで開くなら何も書かなくてよい。`http://aoi.local:8081/viewer/` のように名前で開くなら、その名前をポート無しで `allowed_hosts` に書く (大文字小文字と末尾のドットは区別しない)。

有効にする手順は次のとおり。順番を守ると、途中でビューアがインターネットに出ることが無い。

1. 本番の `cloudflared/config.yml` の OwnTracks のホスト名に `path: ^/(pub|healthz)$` を足し、`docker compose --profile tunnel restart tunnel` で反映する
2. `config.toml` に `[location.viewer]` を足し、kgd を新しいイメージで起動し直す
3. `just check-exposure https://<OwnTracks のホスト名>` を実行し、すべて `ok` になることを確かめる
4. LAN の端末のブラウザで `http://<host>:8081/viewer/` を開く

画面は `web/` にある React のアプリで、Docker のイメージをビルドするときにビルドしてバイナリに埋め込む。手元で開発するときは次のようにする。

```bash
just web-install   # 依存を入れる (mise で node と aube を入れておく)
just web-dev       # Vite の開発サーバー。/viewer/api は 127.0.0.1:8081 の kgd へ流す
just web-build     # web/dist にビルドする (デバッグビルドの kgd はここを直接読み、リリースビルドは kgd をビルドし直すと埋め込まれる)
just web-check     # 型チェック、lint、テスト
just gen-api       # Rust の API の型を変えたら、画面側のスキーマを作り直す
```

開発サーバーからのプロキシは 127.0.0.1 から届き、`Host` は `localhost:5173` のまま渡すため、手元の `config.toml` でだけ `allowed_cidrs` に `"127.0.0.1/32"` を、`allowed_hosts` に `"localhost"` を足す。

## テストカバレッジ

カバレッジは [Codecov](https://codecov.io/gh/ekuinox/kgd) で確認できる（PR には自動でカバレッジコメントが付く）。

ローカルで HTML レポートを生成する場合:

```bash
cargo install cargo-llvm-cov  # 初回のみ
just cov
```
