//! ブラウザで位置ログを見るビューアのコントローラ。
//!
//! OwnTracks の受け口と同じ待ち受けで `/viewer/` 以下に置く。ログインは無く、
//! 送信元の許可リストと Cloudflare 経由の印で守る (ADR-0013)。

use ipnet::IpNet;

#[allow(dead_code)] // Task 7 で viewer_router から使う
mod guard;

/// ビューアの設定。
#[derive(Debug, Clone)]
pub struct ViewerSettings {
    /// ビューアに届いてよい送信元
    pub allowed_cidrs: Vec<IpNet>,
}
