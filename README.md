# kgd

ekuinox 自身のためにいろいろやってもらう bot 的な何か

Discord Bot として機能して、ローカルのサーバーの起動や Notion のデータベース管理などを行いたい

## 機能

- ローカルにあるサーバーの起動と起動状況確認
- Discord フォーラムでの日報作成

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
