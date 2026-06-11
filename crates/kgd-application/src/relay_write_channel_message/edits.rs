//! 転記済みメッセージの編集・削除処理。

use anyhow::{Context as _, Result};
use tracing::{error, info, warn};

use kgd_domain::{RelayedMessage, SyncMessage, build_relay_content};

use super::RelayWriteChannelMessage;

impl RelayWriteChannelMessage {
    /// 書き込み用チャンネルでのメッセージ編集を処理する。
    ///
    /// Notion 側のブロックを作り直し、スレッドの転記メッセージをその場で編集する
    /// （スレッド内の時系列を保つため、削除や再投稿はしない）。
    pub async fn update(&self, message: &SyncMessage) -> Result<()> {
        // この機能経由で転記済みのメッセージのみ対象
        let Some(relayed) = self.repo.get_relayed_message(message.message_id).await? else {
            return Ok(());
        };

        // 転記先スレッドの日報エントリ（Notion ページ）を取得
        let Some(entry) = self.repo.get_by_thread(relayed.thread_id).await? else {
            warn!(
                thread_id = relayed.thread_id,
                "Diary entry not found for relayed message"
            );
            return Ok(());
        };

        // Notion 側は旧ブロックを削除して作り直す
        // （添付や URL 変換によるブロック構成の変化に追従するため）
        self.sync.delete(message.message_id).await?;
        let result = self.sync.sync(&entry.page_id, message).await?;

        if !result.synced {
            // 編集後の内容が同期対象でなくなった場合は転記メッセージと対応を削除する
            self.delete_relayed(&relayed).await?;
            return Ok(());
        }

        // 転記メッセージをその場で編集する
        let content = build_relay_content(message);
        self.gateway
            .edit_message_content(relayed.thread_id, relayed.relayed_message_id, &content)
            .await
            .context("Failed to edit relayed message")?;

        info!(
            thread_id = relayed.thread_id,
            message_id = message.message_id,
            relayed_message_id = relayed.relayed_message_id,
            "Write channel message updated and relayed message edited"
        );

        Ok(())
    }

    /// 書き込み用チャンネルでのメッセージ削除を処理する。
    ///
    /// Notion 側のブロックとスレッドの転記メッセージを削除する。
    pub async fn delete(&self, source_message_id: u64) -> Result<()> {
        // この機能経由で転記済みのメッセージのみ対象
        let Some(relayed) = self.repo.get_relayed_message(source_message_id).await? else {
            return Ok(());
        };

        if let Err(error) = self.sync.delete(source_message_id).await {
            error!(error = %error, "Failed to delete write channel message from Notion");
        }

        self.delete_relayed(&relayed).await?;

        info!(
            thread_id = relayed.thread_id,
            message_id = source_message_id,
            "Write channel message deleted"
        );

        Ok(())
    }

    /// スレッドの転記メッセージと対応レコードを削除する。
    ///
    /// 転記メッセージの削除失敗（手動で削除済みなど）は警告に留めて続行する。
    async fn delete_relayed(&self, relayed: &RelayedMessage) -> Result<()> {
        if let Err(error) = self
            .gateway
            .delete_message(relayed.thread_id, relayed.relayed_message_id)
            .await
        {
            warn!(error = %error, "Failed to delete relayed message");
        }

        self.repo
            .delete_relayed_message(relayed.source_message_id)
            .await
    }
}
