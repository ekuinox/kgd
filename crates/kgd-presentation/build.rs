//! 埋め込む画面 (`web/dist`) が変わったら、このクレートを作り直させる。
//!
//! rust-embed は `#[allow_missing]` のとき、あるファイルだけを `include_bytes!` で取り込み、
//! フォルダそのものは見張らない。そのため `web/dist` が無い状態でビルドしたあとに
//! `just web-build` で作っても、cargo は入力が変わっていないと判断して空のまま使い続ける。
//! ここでフォルダを見張り、中身が変わったらビルドスクリプトごと作り直させる。

use std::fs;

/// 埋め込む画面のフォルダ (このクレートからの相対パス)。`assets.rs` の `#[folder]` と揃える。
const WEB_DIST: &str = "../../web/dist";

fn main() {
    // 見張るパスが無いと cargo は毎回作り直すため、未ビルドなら空のフォルダを置いておく。
    // 作れなくても (読み取り専用のソースなど) 毎回作り直すだけで、結果は変わらない
    let _ = fs::create_dir_all(WEB_DIST);
    println!("cargo:rerun-if-changed={WEB_DIST}");
}
