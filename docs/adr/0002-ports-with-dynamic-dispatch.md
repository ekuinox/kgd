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
