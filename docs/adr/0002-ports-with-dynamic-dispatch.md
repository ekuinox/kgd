# 0002: ポートは trait + 動的ディスパッチ (Arc dyn) で抽象化する

## ステータス

受理 (2026-06-11, #70 / #64)

## 文脈

IO を抽象化する方法として、trait + ジェネリクス (静的ディスパッチ) と
trait オブジェクト (動的ディスパッチ) の選択肢があった。

## 決定

ポートは `#[cfg_attr(test, mockall::automock)]` + `#[async_trait::async_trait]` を付けた
trait として kgd-application に定義し、利用側は `Arc<dyn Trait>` で保持する。

ジェネリクスを採らなかった理由:

- presentation の Handler は serenity の EventHandler を実装するため `Clone + 'static` が必要で、
  ポートをジェネリクスにすると型パラメータが 9 個近くに膨らみ、組み立てとシグネチャが破綻する
- スケジューラの `Vec<Arc<dyn ScheduledJob>>` とも自然に揃う
- IO バウンドな bot であり、動的ディスパッチのコストは無視できる

mockall との併用にあたり、trait のメソッドはジェネリクスを持たせず、
戻り値は所有値とし、ライフタイムが返り値に絡むシグネチャを避ける。

## 結果

- テストでは `MockNotionApi` などの自動生成モックで呼び出し回数・順序・引数を検証できる
- Composition Root (bootstrap) で `Arc::new(実装) as Arc<dyn ポート>` と組み立てるだけで配線できる

## 検討した代替案 (2026-06-12 追記)

### RPIT / AFIT (`async fn in trait`) で async-trait を外す → 不採用

AFIT / RPITIT は Rust 1.92 で安定しているが **`dyn` 互換ではない**ため、
`Arc<dyn ポート>` を保ったまま async-trait を外すことはできない。外す道は次の2つだけで、いずれも純損:

- ジェネリクス全面移行 → 本 ADR で却下済み。集約点の `DiscordController` は
  下位ポートの和集合で型パラメータが 8 個になり、serenity の `EventHandler` 実装が
  複数ファイルに分かれるため 8 個の where 句が全 impl ブロックに反復する。
  組み立て側は型推論が効くが、定義側の恒常的なノイズに見合う便益がない。
- `dynosaur` 等で AFIT + dyn ラッパーを生成 → `#[cfg_attr(test, mockall::automock)]` が
  AFIT 未対応のため、全ポートのテストモック基盤を作り直す必要があり過大。

async-trait の実体は呼び出しごとの `Box` 確保 1 回で、IO バウンドな bot では無視できる。
なお `Clock` / `ImageConverter` / `WolSender` は同期メソッドのため元から async-trait 不使用。

### DI コンテナ (shaku 等) の導入 → 不採用

bootstrap の手動コンストラクタ注入で配線は十分明快。コンテナは `Component` / `Interface`
derive や `module!` マクロで boilerplate を増やし、配線の見通しをむしろ下げる。
Rust では手動コンストラクタ注入が最も素直な DI である。将来 bootstrap が肥大化した場合は、
コンテナ導入ではなくヘルパー関数への分割で対処する。
