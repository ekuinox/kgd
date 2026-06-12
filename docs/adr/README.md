# Architecture Decision Records

設計上の重要な決定とその理由を記録する。1 決定 = 1 ファイル。

書式: 各 ADR は「ステータス / 文脈 / 決定 / 結果」の 4 節で構成する。
一度受理した ADR は書き換えず、覆す場合は新しい ADR で上書きして相互にリンクする。

| # | タイトル | ステータス |
|---|---|---|
| [0001](0001-adopt-clean-architecture.md) | クリーンアーキテクチャの採用と層ごとの crate 分割 | 受理 |
| [0002](0002-ports-with-dynamic-dispatch.md) | ポートは trait + 動的ディスパッチ (Arc dyn) で抽象化する | 受理 |
| [0003](0003-domain-dto-at-serenity-boundary.md) | serenity の型は境界でドメイン DTO に変換する | 受理 |
| [0004](0004-minimal-scheduler-with-job-self-decision.md) | スケジューラは最小の固定間隔ランナー + ジョブ内自己判定 | 受理 |
| [0005](0005-serialize-write-channel-relay.md) | 書き込みチャンネルの転記は単一ワーカーで直列化する | 受理 |
