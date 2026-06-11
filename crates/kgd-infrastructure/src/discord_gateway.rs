//! serenity による DiscordGateway ポートの実装とヘルパー群。
//!
//! ボタンの生成・検出や serenity 型からドメイン DTO への変換など、
//! serenity 固有のコードはこのモジュールに閉じ込める。

use std::sync::Arc;

use anyhow::{Context as _, Result};
use serenity::all::{
    ActionRowComponent, ButtonKind, ChannelId, ChannelType, CreateActionRow, CreateButton,
    CreateForumPost, CreateMessage, EditMessage, EditThread, GetMessages, Http, Message, MessageId,
    ReactionType,
};
use tracing::warn;

use kgd_application::ports::DiscordGateway;
use kgd_domain::{
    DIARY_CLOSE_AND_NEW_BUTTON_ID, SyncAttachment, SyncMessage, ThreadState,
    merge_forwarded_content,
};

/// クローズ & 新規作成ボタンと一緒に送る案内メッセージ。
const CLOSE_AND_NEW_PROMPT: &str =
    "日付が変わりました。このスレッドをクローズして新しい日報を作成しますか？";

/// serenity の Http クライアントを使った [`DiscordGateway`] 実装。
pub struct SerenityGateway {
    /// Discord API クライアント
    http: Arc<Http>,
}

impl SerenityGateway {
    /// 新しい SerenityGateway を作成する。
    pub fn new(http: Arc<Http>) -> Self {
        Self { http }
    }
}

#[async_trait::async_trait]
impl DiscordGateway for SerenityGateway {
    async fn thread_state(&self, thread_id: u64) -> Result<Option<ThreadState>> {
        // 取得失敗（削除済みなど）はエラーではなく None として扱う
        let Ok(channel) = ChannelId::new(thread_id).to_channel(&self.http).await else {
            return Ok(None);
        };
        let Some(thread) = channel.guild() else {
            return Ok(None);
        };
        let (archived, locked) = thread
            .thread_metadata
            .map(|m| (m.archived, m.locked))
            .unwrap_or((false, false));
        Ok(Some(ThreadState {
            is_public_thread: thread.kind == ChannelType::PublicThread,
            archived,
            locked,
        }))
    }

    async fn create_diary_forum_post(
        &self,
        forum_channel_id: u64,
        title: &str,
        page_url: &str,
    ) -> Result<u64> {
        let initial_message = create_diary_thread_initial_message(page_url);
        let forum_post = CreateForumPost::new(title, initial_message);
        let thread = ChannelId::new(forum_channel_id)
            .create_forum_post(&self.http, forum_post)
            .await
            .context("Failed to create forum post")?;
        Ok(thread.id.get())
    }

    async fn close_thread(&self, thread_id: u64) -> Result<()> {
        // locked=true にすることで、ユーザーが書き込んでも自動的に再開されない
        let edit = EditThread::new().archived(true).locked(true);
        ChannelId::new(thread_id)
            .edit_thread(&self.http, edit)
            .await
            .context("Failed to close thread")?;
        Ok(())
    }

    async fn reopen_thread(&self, thread_id: u64) -> Result<bool> {
        let channel_id = ChannelId::new(thread_id);
        if let Err(error) = channel_id.join_thread(&self.http).await {
            warn!(error = %error, thread_id, "Failed to join thread for reopening");
            return Ok(false);
        }
        let edit = EditThread::new().archived(false).locked(false);
        match channel_id.edit_thread(&self.http, edit).await {
            Ok(_) => Ok(true),
            Err(error) => {
                warn!(error = %error, thread_id, "Failed to reopen thread");
                Ok(false)
            }
        }
    }

    async fn has_close_and_new_button(&self, thread_id: u64, limit: u8) -> Result<bool> {
        let messages = ChannelId::new(thread_id)
            .messages(&self.http, GetMessages::new().limit(limit))
            .await
            .context("Failed to fetch thread messages to inspect close button")?;
        Ok(messages.iter().any(message_has_close_and_new_button))
    }

    async fn send_text(&self, channel_id: u64, content: &str) -> Result<u64> {
        let message = CreateMessage::new().content(content);
        let sent = ChannelId::new(channel_id)
            .send_message(&self.http, message)
            .await
            .context("Failed to send message")?;
        Ok(sent.id.get())
    }

    async fn edit_message_content(
        &self,
        channel_id: u64,
        message_id: u64,
        content: &str,
    ) -> Result<()> {
        ChannelId::new(channel_id)
            .edit_message(
                &self.http,
                MessageId::new(message_id),
                EditMessage::new().content(content),
            )
            .await
            .context("Failed to edit message")?;
        Ok(())
    }

    async fn delete_message(&self, channel_id: u64, message_id: u64) -> Result<()> {
        ChannelId::new(channel_id)
            .delete_message(&self.http, MessageId::new(message_id))
            .await
            .context("Failed to delete message")?;
        Ok(())
    }

    async fn send_close_and_new_button(&self, thread_id: u64) -> Result<()> {
        let message = CreateMessage::new()
            .content(CLOSE_AND_NEW_PROMPT)
            .components(vec![create_close_and_new_action_row()]);
        ChannelId::new(thread_id)
            .send_message(&self.http, message)
            .await
            .context("Failed to send close-and-new button message")?;
        Ok(())
    }

    async fn send_write_channel_new_diary_button(&self, channel_id: u64) -> Result<()> {
        let message = CreateMessage::new()
            .content("日付が変わりました。前日の日報をクローズして新しい日報を作成しますか？")
            .components(vec![create_close_and_new_action_row()]);
        ChannelId::new(channel_id)
            .send_message(&self.http, message)
            .await
            .context("Failed to send new-diary button message to write channel")?;
        Ok(())
    }

    async fn fetch_messages_before(
        &self,
        thread_id: u64,
        before: Option<u64>,
        limit: u8,
    ) -> Result<Vec<SyncMessage>> {
        let mut request = GetMessages::new().limit(limit);
        if let Some(before_message_id) = before {
            request = request.before(MessageId::new(before_message_id));
        }
        let messages = ChannelId::new(thread_id)
            .messages(&self.http, request)
            .await
            .with_context(|| format!("Failed to fetch messages for thread {}", thread_id))?;
        Ok(messages.iter().map(to_sync_message).collect())
    }

    async fn add_reaction(&self, channel_id: u64, message_id: u64, emoji: &str) -> Result<()> {
        let reaction = ReactionType::Unicode(emoji.to_string());
        self.http
            .create_reaction(
                ChannelId::new(channel_id),
                MessageId::new(message_id),
                &reaction,
            )
            .await
            .context("Failed to add reaction")?;
        Ok(())
    }
}

/// serenity の Message をドメインの [`SyncMessage`] に変換する。
///
/// 転送 (Forward) メッセージは本文・添付がスナップショット側に入るため統合する。
pub(crate) fn to_sync_message(message: &Message) -> SyncMessage {
    let snapshot_contents: Vec<String> = message
        .message_snapshots
        .iter()
        .map(|snapshot| snapshot.content.clone())
        .collect();
    let snapshot_attachments = message
        .message_snapshots
        .iter()
        .flat_map(|snapshot| snapshot.attachments.iter());

    SyncMessage {
        message_id: message.id.get(),
        channel_id: message.channel_id.get(),
        guild_id: message.guild_id.map(|guild_id| guild_id.get()),
        content: merge_forwarded_content(&message.content, &snapshot_contents),
        is_bot: message.author.bot,
        attachments: message
            .attachments
            .iter()
            .chain(snapshot_attachments)
            .map(|attachment| SyncAttachment {
                filename: attachment.filename.clone(),
                url: attachment.url.clone(),
                description: attachment.description.clone(),
            })
            .collect(),
    }
}

/// クローズ & 新規作成ボタンの ActionRow を作成する。
fn create_close_and_new_action_row() -> CreateActionRow {
    let button = CreateButton::new(DIARY_CLOSE_AND_NEW_BUTTON_ID)
        .label("クローズして新しい日報を作成")
        .style(serenity::all::ButtonStyle::Primary);
    CreateActionRow::Buttons(vec![button])
}

/// 日報スレッドの最初のメッセージを構築する。
fn create_diary_thread_initial_message(page_url: &str) -> CreateMessage {
    CreateMessage::new()
        .content(format!("Notion: {}", page_url))
        .components(vec![create_close_and_new_action_row()])
}

/// メッセージにクローズ & 新規作成ボタンが含まれるかを判定する。
fn message_has_close_and_new_button(message: &Message) -> bool {
    message.components.iter().any(|row| {
        row.components.iter().any(|component| {
            matches!(
                component,
                ActionRowComponent::Button(button)
                    if matches!(
                        &button.data,
                        ButtonKind::NonLink { custom_id, .. }
                            if custom_id == DIARY_CLOSE_AND_NEW_BUTTON_ID
                    )
            )
        })
    })
}
