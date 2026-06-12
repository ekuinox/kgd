# 0003: serenity の型は境界でドメイン DTO に変換する

## ステータス

受理 (2026-06-11, #70 / #64)

## 文脈

リファクタ前は serenity の Message / Attachment がそのまま同期ロジックまで渡っていた。
serenity の型はフィールドが多くテストで構築しづらいうえ、内側の層が
Discord フレームワークに引きずられる。

## 決定

同期処理が必要とする最小情報だけを持つ DTO (SyncMessage / SyncAttachment / ThreadState) を
kgd-domain に定義し、serenity の型からの変換は外側 (infrastructure の `to_sync_message`、
presentation の Controller) で行う。ポートのシグネチャも plain な型 (u64 / String / DTO) のみ
を使い、serenity の型を出さない。

転送 (Forward) メッセージのように serenity 固有の表現 (message_snapshots) がある場合も、
変換時に統合してドメイン側では単一の本文・添付リストとして扱う。

## 結果

- 内側の層のテストで serenity の型を組み立てる必要がなく、DTO リテラルで済む
- serenity のバージョンアップや表現変更 (転送メッセージ対応など) の影響が変換関数に閉じる
- 変換を 2 層 (infrastructure / presentation) で共有するため、presentation は
  infrastructure の公開関数 `to_sync_message` を import する (重複実装を持たない)
