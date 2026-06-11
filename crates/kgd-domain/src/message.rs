//! 同期処理が扱うメッセージのドメイン型。
//!
//! serenity の型をアプリケーションロジックに持ち込まないための最小表現。
//! serenity からの変換は infrastructure 側が行う。

/// 同期対象メッセージの最小表現。
///
/// Discord の `Message` から必要な情報だけを写し取った DTO。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncMessage {
    /// Discord メッセージ ID
    pub message_id: u64,
    /// メッセージが属するチャンネル (スレッド) ID
    pub channel_id: u64,
    /// メッセージ本文
    pub content: String,
    /// Bot が送信したメッセージかどうか
    pub is_bot: bool,
    /// 添付ファイル一覧
    pub attachments: Vec<SyncAttachment>,
}

/// 同期対象メッセージの添付ファイルの最小表現。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncAttachment {
    /// ファイル名
    pub filename: String,
    /// ダウンロード URL
    pub url: String,
    /// 添付ファイルの説明 (ALT テキスト)
    pub description: Option<String>,
}

/// 転送メッセージのスナップショット本文をメッセージ本文に統合する。
///
/// Discord の転送 (Forward) メッセージは `content` が空で、
/// 元メッセージの本文がスナップショットに入るため、両方を結合して扱う。
pub fn merge_forwarded_content(content: &str, snapshot_contents: &[String]) -> String {
    let mut parts: Vec<&str> = Vec::with_capacity(1 + snapshot_contents.len());
    if !content.is_empty() {
        parts.push(content);
    }
    parts.extend(
        snapshot_contents
            .iter()
            .map(String::as_str)
            .filter(|s| !s.is_empty()),
    );
    parts.join("\n")
}

/// Discord スレッドの状態。
///
/// 自動クローズ判定などで使用する。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThreadState {
    /// 公開スレッドかどうか
    pub is_public_thread: bool,
    /// アーカイブ済みかどうか
    pub archived: bool,
    /// ロック済みかどうか
    pub locked: bool,
}

impl ThreadState {
    /// アーカイブまたはロックされている (クローズ扱いの) 状態かどうかを返す。
    pub fn is_closed(&self) -> bool {
        self.archived || self.locked
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_forwarded_content_returns_own_content_when_no_snapshots() {
        assert_eq!(merge_forwarded_content("hello", &[]), "hello");
    }

    #[test]
    fn merge_forwarded_content_uses_snapshot_when_own_content_empty() {
        // 転送メッセージ: 本文が空でスナップショットに元の本文が入る
        assert_eq!(
            merge_forwarded_content("", &["forwarded".to_string()]),
            "forwarded"
        );
    }

    #[test]
    fn merge_forwarded_content_joins_own_and_snapshots() {
        assert_eq!(
            merge_forwarded_content("comment", &["a".to_string(), "b".to_string()]),
            "comment\na\nb"
        );
    }

    #[test]
    fn merge_forwarded_content_skips_empty_snapshots() {
        assert_eq!(
            merge_forwarded_content("", &[String::new(), "x".to_string()]),
            "x"
        );
    }

    #[test]
    fn merge_forwarded_content_returns_empty_when_all_empty() {
        assert_eq!(merge_forwarded_content("", &[String::new()]), "");
    }
}
