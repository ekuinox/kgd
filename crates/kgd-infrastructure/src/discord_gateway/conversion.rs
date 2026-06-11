//! serenity 型からドメイン DTO への変換。

use serenity::all::Message;

use kgd_domain::{SyncAttachment, SyncMessage, merge_forwarded_content};

/// serenity の Message をドメインの [`SyncMessage`] に変換する。
///
/// 転送 (Forward) メッセージは本文・添付がスナップショット側に入るため統合する。
pub fn to_sync_message(message: &Message) -> SyncMessage {
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
