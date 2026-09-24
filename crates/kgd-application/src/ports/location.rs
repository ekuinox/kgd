//! OwnTracks メッセージの永続化を抽象化するポート。

use anyhow::Result;

use kgd_domain::OwnTracksMessage;

/// OwnTracks メッセージの永続化を抽象化するポート。
#[cfg_attr(test, mockall::automock)]
#[async_trait::async_trait]
pub trait LocationRepository: Send + Sync {
    /// メッセージをまとめて保存し、実際に挿入された件数を返す。
    ///
    /// 既に同じメッセージが保存されている場合は挿入せず、件数にも含めない。
    async fn insert_messages(&self, messages: &[OwnTracksMessage]) -> Result<usize>;
}
