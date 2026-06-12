//! 書き込み用チャンネルから日報スレッドへの転記メッセージ構築ロジック。

use crate::message::SyncMessage;

/// Discord メッセージ本文の最大文字数。
pub const DISCORD_MESSAGE_CONTENT_LIMIT: usize = 2000;

/// メッセージへのリンク URL を組み立てる。
///
/// DM など guild が無い場合は `@me` を使う。
pub fn message_link(guild_id: Option<u64>, channel_id: u64, message_id: u64) -> String {
    match guild_id {
        Some(guild_id) => format!(
            "https://discord.com/channels/{}/{}/{}",
            guild_id, channel_id, message_id
        ),
        None => format!(
            "https://discord.com/channels/@me/{}/{}",
            channel_id, message_id
        ),
    }
}

/// 転記メッセージの本文を組み立てる。
///
/// 元メッセージの本文に添付ファイルの URL と元メッセージへのリンクを付け加える。
pub fn build_relay_content(message: &SyncMessage) -> String {
    let source_link = message_link(message.guild_id, message.channel_id, message.message_id);
    let attachment_urls: Vec<&str> = message
        .attachments
        .iter()
        .map(|attachment| attachment.url.as_str())
        .collect();
    assemble_relay_content(&message.content, &attachment_urls, &source_link)
}

/// 転記メッセージの本文を本文・添付 URL・元メッセージリンクから組み立てる。
///
/// 全体が Discord の文字数上限を超える場合は本文側を切り詰める。
pub fn assemble_relay_content(
    content: &str,
    attachment_urls: &[&str],
    source_link: &str,
) -> String {
    let footer = format!("-# 元のメッセージ: {source_link}");

    let build = |content: &str| {
        let mut sections = Vec::new();
        if !content.is_empty() {
            sections.push(content);
        }
        sections.extend(attachment_urls.iter().copied());
        sections.push(&footer);
        sections.join("\n")
    };

    let assembled = build(content);
    let total = assembled.chars().count();
    if total <= DISCORD_MESSAGE_CONTENT_LIMIT {
        return assembled;
    }

    // 上限超過分と省略記号の分だけ本文を切り詰める
    let overflow = total - DISCORD_MESSAGE_CONTENT_LIMIT;
    let keep = content.chars().count().saturating_sub(overflow + 1);
    let truncated: String = content.chars().take(keep).chain(['…']).collect();

    // 本文以外（添付 URL とフッタ）だけで上限を超える場合に備え、
    // 最終的に全体を上限で丸めて送信失敗を防ぐ
    build(&truncated)
        .chars()
        .take(DISCORD_MESSAGE_CONTENT_LIMIT)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// guild_id がある場合、guild/channel/message を含む
    /// 通常のメッセージリンク URL が生成されることを確認する。
    #[test]
    fn message_link_uses_guild_id_when_present() {
        assert_eq!(
            message_link(Some(1), 2, 3),
            "https://discord.com/channels/1/2/3"
        );
    }

    /// guild_id が None（DM 等）の場合、guild 部分に @me を使った
    /// メッセージリンク URL が生成されることを確認する。
    #[test]
    fn message_link_uses_at_me_without_guild() {
        assert_eq!(
            message_link(None, 2, 3),
            "https://discord.com/channels/@me/2/3"
        );
    }

    /// 添付なしの本文から、本文の次行に元メッセージリンクのフッタが
    /// 付いた転記本文が組み立てられることを確認する。
    #[test]
    fn relay_content_contains_content_and_source_link() {
        let content =
            assemble_relay_content("今日の作業ログ", &[], "https://discord.com/channels/1/2/3");

        assert_eq!(
            content,
            "今日の作業ログ\n-# 元のメッセージ: https://discord.com/channels/1/2/3"
        );
    }

    /// 本文・各添付 URL・フッタが改行区切りでこの順に連結された
    /// 転記本文が組み立てられることを確認する。
    #[test]
    fn relay_content_includes_attachment_urls() {
        let urls = [
            "https://cdn.example.com/a.png",
            "https://cdn.example.com/b.png",
        ];
        let content = assemble_relay_content("写真", &urls, "https://discord.com/channels/1/2/3");

        assert_eq!(
            content,
            "写真\nhttps://cdn.example.com/a.png\nhttps://cdn.example.com/b.png\n-# 元のメッセージ: https://discord.com/channels/1/2/3"
        );
    }

    /// 本文が空の場合は本文行を出力せず、添付 URL とフッタのみで
    /// 転記本文が組み立てられることを確認する。
    #[test]
    fn relay_content_omits_empty_content_line() {
        let urls = ["https://cdn.example.com/a.png"];
        let content = assemble_relay_content("", &urls, "https://discord.com/channels/1/2/3");

        assert_eq!(
            content,
            "https://cdn.example.com/a.png\n-# 元のメッセージ: https://discord.com/channels/1/2/3"
        );
    }

    /// 添付 URL とフッタだけで Discord の文字数上限を超えるケースでも、
    /// 最終的に全体が上限以内に丸められることを確認する。
    #[test]
    fn relay_content_clamps_when_attachments_alone_exceed_limit() {
        // 添付 URL とフッタだけで上限を超えるケースでも全体が上限に収まる
        let long_urls: Vec<String> = (0..20)
            .map(|i| format!("https://cdn.example.com/{}/{}.png", "a".repeat(100), i))
            .collect();
        let url_refs: Vec<&str> = long_urls.iter().map(String::as_str).collect();
        let content =
            assemble_relay_content("本文", &url_refs, "https://discord.com/channels/1/2/3");

        assert!(content.chars().count() <= DISCORD_MESSAGE_CONTENT_LIMIT);
    }

    /// 本文が上限を超える場合、全体が上限ちょうどに収まるよう本文を切り詰め、
    /// 省略記号を付けつつ末尾のフッタは保持されることを確認する。
    #[test]
    fn relay_content_truncates_long_content_to_limit() {
        let long_content = "あ".repeat(3000);
        let content =
            assemble_relay_content(&long_content, &[], "https://discord.com/channels/1/2/3");

        assert_eq!(content.chars().count(), DISCORD_MESSAGE_CONTENT_LIMIT);
        assert!(content.contains('…'));
        assert!(content.ends_with("-# 元のメッセージ: https://discord.com/channels/1/2/3"));
    }
}
