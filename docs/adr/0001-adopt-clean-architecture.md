# 0001: クリーンアーキテクチャの採用と層ごとの crate 分割

## ステータス

受理 (2026-06-11, #70 / #64)

## 文脈

リファクタ前は discord.rs (1300 行超) に Discord イベント処理・コマンド処理・定時処理・
Notion 同期・状態管理が混在し、Discord / Notion / PostgreSQL / HTTP の IO と
アプリケーションロジックが密結合だった。このため IO をモックした単体テストが書けず (#64)、
定時処理の機構も再利用できなかった (#70)。

## 決定

クリーンアーキテクチャの 4 層をワークスペースの crate にマップする。

- kgd-domain: エンティティと純粋ロジック。IO ライブラリに依存しない
- kgd-application: ユースケースとポート (trait)。kgd-domain のみに依存
- kgd-infrastructure: ポートの実装。serenity / sqlx / reqwest などへの依存はここに閉じる
- kgd-presentation: Controller と Presenter
- kgd (binary): 設定読み込みと Composition Root (bootstrap)

依存方向 (外→内のみ) は Cargo の依存関係でコンパイル時に強制する。
モジュール分割ではなく crate 分割としたのは、規律を機械的に担保できることに加え、
内側の層のテスト (`cargo test -p kgd-application`) が重い IO 依存のビルドを
必要としなくなるため。

## 結果

- 全 IO がモック可能になり、ユースケースと判断ロジックの単体テストが書けるようになった
- 内側 crate のテストは serenity / sqlx / libheif をビルドせず高速に回せる
- crate 境界をまたぐ変更 (ポートのシグネチャ変更など) は複数 crate の修正を伴う。
  これは依存規律の対価として受け入れる
