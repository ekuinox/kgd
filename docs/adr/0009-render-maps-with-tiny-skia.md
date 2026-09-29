# 0009: 地図描画に staticmap を採用せず tiny-skia を直接使う

## ステータス

受理 (2026-09-29)

## 文脈

位置ログの日次レポートでは、軌跡を地図画像として描画する必要がある。
タイルベースの地図描画クレートとして `staticmap` を検討したが、次の理由で採用しない。

依存する `attohttpc` が MPL-2.0 であり、`deny.toml` の許可リスト
(MIT / Apache-2.0 / Apache-2.0 WITH LLVM-exception / BSD-2-Clause / BSD-3-Clause /
ISC / Zlib / Unicode-3.0 / CDLA-Permissive-2.0) に含まれないため、`cargo deny check` を
通らない。ライセンス以外にも次の難点がある。

- HTTP クライアントが同期の attohttpc であり、tokio ランタイム上でブロッキング呼び出しになる
- タイル取得の User-Agent を差し替えられない。OpenStreetMap のタイル利用規約は
  識別可能な User-Agent を要求する
- `tiny-skia 0.11` に固定されており、最新の 0.12 と重複する
  (`bans.multiple-versions = "warn"`)
- 上流の更新が 2024-01 で止まっている

## 決定

**`tiny-skia` を直接使う。** staticmap が描画に用いているのと同じクレートであり、
線の太さ、色、アンチエイリアスはクレートに任せられる。ライセンスは BSD-3-Clause で
許可リストに含まれる。

タイルの取得は既存の `reqwest` で行い、User-Agent とキャッシュを自前で制御する。
URL は `https://tile.openstreetmap.org/{z}/{x}/{y}.png`、User-Agent は
`kgd/<version> (+https://github.com/ekuinox/kgd)` を設定する。
取得したタイルは `[location].tile_cache_dir` にキャッシュし、同じタイルを取り直さない。
タイル PNG のデコードと地図画像のエンコードは tiny-skia の `png-format` 機能
(既定で有効) で行うため、画像デコードの依存を新たに増やさない。

## 結果

新規依存は tiny-skia のみになり、`cargo deny check` を通る。
HTTP クライアントは既存の reqwest に一本化され、同期呼び出しによるブロッキングは無い。

`MapRenderer` ポートの実装 `TileMapRenderer` (kgd-infrastructure) が投影済みの区間と
表示範囲を受け取って描画する。**タイルの取得に失敗した場合は、そのタイルを灰色で塗って
描画を続ける。** 失敗を描画全体の失敗にすると、定時ジョブが毎分再試行して OSM へ
繰り返し取りにいくことになり、利用規約の面で望ましくない。軌跡と集計値が載ることを
優先する。

`© OpenStreetMap contributors` の表示は、画像に文字を焼くとフォントとグリフのクレートが
増えるため、本文のテキストとして入れる。これにより依存を増やさずに表示義務を満たす。
