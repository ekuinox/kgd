# 0014: ビューアの画面は React と Vite で作り、バイナリに埋め込み、API の型は Rust から生成する

## ステータス

受理 (2026-09-30)

## 文脈

ビューアの画面は、期間を選び、GL の地図に軌跡を描き、日ごとの集計をグラフにする。
kgd はこれまで Rust だけでビルドしており、Dockerfile にも Node は無い。

## 決定

**画面は React、Vite、TypeScript で作り、パッケージマネージャーには aube を使う。**
地図は MapLibre GL JS (`@vis.gl/react-maplibre`) で、OpenFreeMap のベクタータイルをブラウザから
直接取得する。タイルの中継はしない。

**Docker のビルド専用の Node の段で画面をビルドし、rust-embed でバイナリに埋め込む。**
実行用のイメージには Node を入れない。rust-embed は `web/dist` が無くてもビルドを通すため、
Node が無い環境でも `cargo build` と `cargo test` は動く。その代わり、Docker の builder の段で
`web/dist/index.html` の有無を確かめ、画面を含まないイメージができないようにする。

**API の入出力の型は Rust の DTO を正とし、schemars の JSON Schema から valibot のスキーマを生成する。**
JSON Schema から valibot への変換は自前の小さなスクリプトで行う。既存の変換ツールは、最も
知られていた `liam-hq/json-schema-to-valibot` が 2026 年 6 月にアーカイブされており、他の実装も
利用者が少なかった。スクリプトは今回の DTO が使うキーワードだけに対応し、知らないキーワードに
出会ったらエラーで止まる。

Rust の型から valibot を直接出す specta-valibot は採らなかった。specta 2 は RC の段階にあり、
specta-valibot は部分的な実装で crates.io に公開されておらず、`deny.toml` の `allow-git` に
例外が要るためである。

生成物 (`web/src/api/schema.json` と `schema.gen.ts`) はコミットする。Docker の Node の段が
Rust 無しでビルドでき、API の変更がレビューの差分に現れる。

## 結果

手元では mise で Node と aube を入れる。
aube は `github:aubepkg/aube` で入れる。
mise の npm backend で `@endevco/aube` を入れる方法は、ネイティブバイナリを取得する preinstall を実行しないため使えない。
Docker と CI では `npm install -g --ignore-scripts=false @endevco/aube` で入れる。

Rust の型を変えて生成し忘れると、kgd-presentation のテストが `schema.json` のずれを、
CI の web ジョブが `schema.gen.ts` のずれを検出する。

OpenFreeMap の提供が止まると地図の背景が表示されなくなる。軌跡と集計は表示される。
