//! OwnTracks メッセージの永続化を抽象化するポート。

use anyhow::Result;
use chrono::{DateTime, Utc};

use kgd_domain::{OwnTracksMessage, TrackPoint};

/// OwnTracks メッセージの永続化を抽象化するポート。
#[cfg_attr(test, mockall::automock)]
#[async_trait::async_trait]
pub trait LocationRepository: Send + Sync {
    /// メッセージをまとめて保存し、実際に挿入された件数を返す。
    ///
    /// 既に同じメッセージが保存されている場合は挿入せず、件数にも含めない。
    async fn insert_messages(&self, messages: &[OwnTracksMessage]) -> Result<usize>;

    /// 指定した範囲 (開始を含み終了を含まない) の位置の点を時刻順に返す。
    ///
    /// 対象は緯度経度を持つ location メッセージに限る。
    async fn locations_between(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<TrackPoint>>;
}
