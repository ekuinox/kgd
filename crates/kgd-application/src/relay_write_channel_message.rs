//! 書き込み用チャンネルへの投稿を最新の日報スレッドへ転記するユースケース。

use std::sync::Arc;

use anyhow::{Context as _, Result};
use chrono_tz::Tz;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

use kgd_domain::{DiaryEntry, RelayedMessage, SyncMessage, build_relay_content, today_in_timezone};

use super::{
    SyncDiaryMessage,
    ports::{Clock, DiaryRepository, DiscordGateway},
};

/// 書き込み用チャンネルで起きたイベント。到着順にワーカーが処理する。
#[derive(Debug)]
pub enum WriteChannelEvent {
    /// メッセージが投稿された
    Posted(SyncMessage),
    /// メッセージが編集された
    Updated(SyncMessage),
    /// メッセージが削除された
    Deleted {
        /// 書き込み用チャンネルの元メッセージ ID
        source_message_id: u64,
    },
}

/// キューからイベントを到着順に 1 件ずつ取り出して転記処理を行うワーカー。
///
/// Discord のイベントハンドラは並行実行されるため、転記の順序
/// （スレッドへの投稿順・Notion のブロック順）を保証するためにここで直列化する。
/// このループは全送信側が閉じられるまで戻らないため、呼び出し側で spawn すること。
pub async fn run_relay_worker(
    relay: Arc<RelayWriteChannelMessage>,
    mut rx: mpsc::Receiver<WriteChannelEvent>,
) {
    while let Some(event) = rx.recv().await {
        let result = match &event {
            WriteChannelEvent::Posted(message) => relay.relay(message).await,
            WriteChannelEvent::Updated(message) => relay.update(message).await,
            WriteChannelEvent::Deleted { source_message_id } => {
                relay.delete(*source_message_id).await
            }
        };
        if let Err(error) = result {
            error!(error = %error, "Failed to process write channel event");
        }
    }
}

/// 転記ユースケースの設定。
#[derive(Debug, Clone)]
pub struct RelaySettings {
    /// 日報の日付計算に使用するタイムゾーン
    pub timezone: Tz,
    /// 同期成功時にメッセージに付けるリアクション絵文字
    pub sync_reaction: String,
}

/// 書き込み用チャンネルへの投稿を最新の日報スレッドへ転記するユースケース。
pub struct RelayWriteChannelMessage {
    /// 日報リポジトリポート
    repo: Arc<dyn DiaryRepository>,
    /// Discord ゲートウェイポート
    gateway: Arc<dyn DiscordGateway>,
    /// 時刻ポート
    clock: Arc<dyn Clock>,
    /// メッセージ同期ユースケース
    sync: Arc<SyncDiaryMessage>,
    /// 転記設定
    settings: RelaySettings,
}

impl RelayWriteChannelMessage {
    /// 新しい RelayWriteChannelMessage を作成する。
    pub fn new(
        repo: Arc<dyn DiaryRepository>,
        gateway: Arc<dyn DiscordGateway>,
        clock: Arc<dyn Clock>,
        sync: Arc<SyncDiaryMessage>,
        settings: RelaySettings,
    ) -> Self {
        Self {
            repo,
            gateway,
            clock,
            sync,
            settings,
        }
    }

    /// 書き込み用チャンネルへの投稿を処理する。
    ///
    /// メッセージを最新の日報スレッドの Notion ページへ同期し、
    /// 同期に成功したらスレッドへ転記して対応を記録する。
    /// 同期がスキップされた場合（空メッセージなど）は転記も行わない。
    pub async fn relay(&self, message: &SyncMessage) -> Result<()> {
        let Some(entry) = self.latest_diary_entry_for_relay().await? else {
            warn!(
                message_id = message.message_id,
                "No diary entry found to relay write channel message"
            );
            return Ok(());
        };

        let result = self.sync.sync(&entry.page_id, message).await?;
        if !result.synced {
            // 同期対象がない場合は転記もしない
            return Ok(());
        }
        info!(
            thread_id = entry.thread_id,
            message_id = message.message_id,
            blocks = result.block_count,
            "Write channel message synced to Notion"
        );

        // 転記に失敗した場合は Notion ブロックを巻き戻して整合を保つ
        // （対応レコードが無いまま Notion にブロックが残ると、後続の編集・削除が追従できないため）
        let content = build_relay_content(message);
        let relayed_message_id = match self.gateway.send_text(entry.thread_id, &content).await {
            Ok(relayed_message_id) => relayed_message_id,
            Err(error) => {
                if let Err(rollback_error) = self.sync.delete(message.message_id).await {
                    error!(
                        error = %rollback_error,
                        message_id = message.message_id,
                        "Failed to roll back Notion blocks after relay failure"
                    );
                }
                return Err(error).context("Failed to relay message to diary thread");
            }
        };

        self.repo
            .upsert_relayed_message(&RelayedMessage {
                source_message_id: message.message_id,
                thread_id: entry.thread_id,
                relayed_message_id,
            })
            .await?;

        // すべて成功したらリアクションを付ける（失敗は同期の失敗とは扱わない）
        if let Err(error) = self
            .gateway
            .add_reaction(
                message.channel_id,
                message.message_id,
                &self.settings.sync_reaction,
            )
            .await
        {
            error!(error = %error, "Failed to add sync reaction");
        }

        info!(
            thread_id = entry.thread_id,
            message_id = message.message_id,
            relayed_message_id,
            "Write channel message relayed to diary thread"
        );

        Ok(())
    }

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

    /// 転記先となる最新の日報エントリを取得する。
    ///
    /// 今日の日報があればそれを、無ければ最新のエントリを返す。
    async fn latest_diary_entry_for_relay(&self) -> Result<Option<DiaryEntry>> {
        let today = today_in_timezone(self.clock.now(), &self.settings.timezone);
        if let Some(entry) = self.repo.get_by_date(today).await? {
            return Ok(Some(entry));
        }
        self.repo.get_latest_entry().await
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

#[cfg(test)]
mod tests {
    use chrono::{DateTime, TimeZone as _, Utc};
    use mockall::predicate::eq;

    use kgd_domain::compile_url_rules;

    use crate::ports::{
        MockAttachmentDownloader, MockClock, MockDiaryRepository, MockDiscordGateway,
        MockImageConverter, MockNotionApi,
    };

    use super::*;

    /// テスト用に UTC の DateTime を作る。
    fn utc(year: i32, month: u32, day: u32, hour: u32, min: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(year, month, day, hour, min, 0)
            .unwrap()
    }

    fn settings() -> RelaySettings {
        RelaySettings {
            timezone: chrono_tz::UTC,
            sync_reaction: "✅".to_string(),
        }
    }

    fn entry(thread_id: u64, date: DateTime<Utc>) -> DiaryEntry {
        DiaryEntry {
            thread_id,
            page_id: format!("page-{}", thread_id),
            page_url: format!("https://notion.example/{}", thread_id),
            date,
            created_at: date,
        }
    }

    fn message(message_id: u64, content: &str) -> SyncMessage {
        SyncMessage {
            message_id,
            channel_id: 500,
            guild_id: Some(1),
            content: content.to_string(),
            is_bot: false,
            attachments: vec![],
        }
    }

    /// テキスト同期だけが成功する SyncDiaryMessage を作る。
    fn sync_service(expected_syncs: usize) -> Arc<SyncDiaryMessage> {
        let url_rules =
            compile_url_rules(&[], &["link".to_string()]).expect("default rules should be ok");
        let mut notion = MockNotionApi::new();
        notion
            .expect_append_blocks()
            .times(expected_syncs)
            .returning(|_, _| Ok(vec!["b1".to_string()]));
        let mut repo = MockDiaryRepository::new();
        repo.expect_insert_message_block()
            .times(expected_syncs)
            .returning(|_| Ok(()));
        Arc::new(SyncDiaryMessage::new(
            Arc::new(notion),
            Arc::new(repo),
            Arc::new(MockAttachmentDownloader::new()),
            Arc::new(MockImageConverter::new()),
            None,
            url_rules,
        ))
    }

    fn relay_use_case(
        repo: MockDiaryRepository,
        gateway: MockDiscordGateway,
        sync: Arc<SyncDiaryMessage>,
    ) -> RelayWriteChannelMessage {
        let mut clock = MockClock::new();
        clock.expect_now().returning(|| utc(2025, 1, 2, 9, 0));
        RelayWriteChannelMessage::new(
            Arc::new(repo),
            Arc::new(gateway),
            Arc::new(clock),
            sync,
            settings(),
        )
    }

    #[tokio::test]
    async fn relay_syncs_posts_and_records_mapping() {
        let today = utc(2025, 1, 2, 0, 0);
        let mut repo = MockDiaryRepository::new();
        repo.expect_get_by_date()
            .times(1)
            .returning(move |_| Ok(Some(entry(100, today))));
        repo.expect_upsert_relayed_message()
            .withf(|relayed| {
                relayed.source_message_id == 10
                    && relayed.thread_id == 100
                    && relayed.relayed_message_id == 900
            })
            .times(1)
            .returning(|_| Ok(()));
        let mut gateway = MockDiscordGateway::new();
        gateway
            .expect_send_text()
            .withf(|thread_id, content| {
                *thread_id == 100
                    && content.contains("hello")
                    && content.contains("https://discord.com/channels/1/500/10")
            })
            .times(1)
            .returning(|_, _| Ok(900));
        gateway
            .expect_add_reaction()
            .with(eq(500u64), eq(10u64), eq("✅"))
            .times(1)
            .returning(|_, _, _| Ok(()));

        let relay = relay_use_case(repo, gateway, sync_service(1));
        relay.relay(&message(10, "hello")).await.unwrap();
    }

    #[tokio::test]
    async fn relay_skips_when_no_diary_entry() {
        let mut repo = MockDiaryRepository::new();
        repo.expect_get_by_date().times(1).returning(|_| Ok(None));
        repo.expect_get_latest_entry()
            .times(1)
            .returning(|| Ok(None));
        let mut gateway = MockDiscordGateway::new();
        gateway.expect_send_text().times(0);

        let relay = relay_use_case(repo, gateway, sync_service(0));
        relay.relay(&message(10, "hello")).await.unwrap();
    }

    #[tokio::test]
    async fn relay_skips_posting_for_empty_message() {
        let today = utc(2025, 1, 2, 0, 0);
        let mut repo = MockDiaryRepository::new();
        repo.expect_get_by_date()
            .times(1)
            .returning(move |_| Ok(Some(entry(100, today))));
        repo.expect_upsert_relayed_message().times(0);
        let mut gateway = MockDiscordGateway::new();
        gateway.expect_send_text().times(0);

        let relay = relay_use_case(repo, gateway, sync_service(0));
        relay.relay(&message(10, "")).await.unwrap();
    }

    #[tokio::test]
    async fn update_edits_relayed_message() {
        let mut repo = MockDiaryRepository::new();
        repo.expect_get_relayed_message()
            .with(eq(10u64))
            .times(1)
            .returning(|_| {
                Ok(Some(RelayedMessage {
                    source_message_id: 10,
                    thread_id: 100,
                    relayed_message_id: 900,
                }))
            });
        repo.expect_get_by_thread()
            .with(eq(100u64))
            .times(1)
            .returning(|_| Ok(Some(entry(100, utc(2025, 1, 2, 0, 0)))));
        let mut gateway = MockDiscordGateway::new();
        gateway
            .expect_edit_message_content()
            .withf(|thread_id, message_id, content| {
                *thread_id == 100 && *message_id == 900 && content.contains("edited")
            })
            .times(1)
            .returning(|_, _, _| Ok(()));

        // 旧ブロック削除 + 再同期を行う SyncDiaryMessage
        let url_rules =
            compile_url_rules(&[], &["link".to_string()]).expect("default rules should be ok");
        let mut notion = MockNotionApi::new();
        notion.expect_delete_block().times(1).returning(|_| Ok(()));
        notion
            .expect_append_blocks()
            .times(1)
            .returning(|_, _| Ok(vec!["b2".to_string()]));
        let mut sync_repo = MockDiaryRepository::new();
        sync_repo
            .expect_get_blocks_by_message()
            .times(1)
            .returning(|_| {
                Ok(vec![kgd_domain::MessageBlock {
                    message_id: 10,
                    block_id: "b1".to_string(),
                    block_type: "text".to_string(),
                    block_order: 0,
                }])
            });
        sync_repo
            .expect_delete_blocks_by_message()
            .times(1)
            .returning(|_| Ok(()));
        sync_repo
            .expect_insert_message_block()
            .times(1)
            .returning(|_| Ok(()));
        let sync = Arc::new(SyncDiaryMessage::new(
            Arc::new(notion),
            Arc::new(sync_repo),
            Arc::new(MockAttachmentDownloader::new()),
            Arc::new(MockImageConverter::new()),
            None,
            url_rules,
        ));

        let relay = relay_use_case(repo, gateway, sync);
        relay.update(&message(10, "edited")).await.unwrap();
    }

    #[tokio::test]
    async fn update_ignores_messages_not_relayed() {
        let mut repo = MockDiaryRepository::new();
        repo.expect_get_relayed_message()
            .times(1)
            .returning(|_| Ok(None));
        let mut gateway = MockDiscordGateway::new();
        gateway.expect_edit_message_content().times(0);

        let relay = relay_use_case(repo, gateway, sync_service(0));
        relay.update(&message(10, "edited")).await.unwrap();
    }

    #[tokio::test]
    async fn delete_removes_blocks_relayed_message_and_mapping() {
        let mut repo = MockDiaryRepository::new();
        repo.expect_get_relayed_message()
            .with(eq(10u64))
            .times(1)
            .returning(|_| {
                Ok(Some(RelayedMessage {
                    source_message_id: 10,
                    thread_id: 100,
                    relayed_message_id: 900,
                }))
            });
        repo.expect_delete_relayed_message()
            .with(eq(10u64))
            .times(1)
            .returning(|_| Ok(()));
        let mut gateway = MockDiscordGateway::new();
        gateway
            .expect_delete_message()
            .with(eq(100u64), eq(900u64))
            .times(1)
            .returning(|_, _| Ok(()));

        // Notion 側のブロック削除
        let url_rules =
            compile_url_rules(&[], &["link".to_string()]).expect("default rules should be ok");
        let mut notion = MockNotionApi::new();
        notion.expect_delete_block().times(1).returning(|_| Ok(()));
        let mut sync_repo = MockDiaryRepository::new();
        sync_repo
            .expect_get_blocks_by_message()
            .times(1)
            .returning(|_| {
                Ok(vec![kgd_domain::MessageBlock {
                    message_id: 10,
                    block_id: "b1".to_string(),
                    block_type: "text".to_string(),
                    block_order: 0,
                }])
            });
        sync_repo
            .expect_delete_blocks_by_message()
            .times(1)
            .returning(|_| Ok(()));
        let sync = Arc::new(SyncDiaryMessage::new(
            Arc::new(notion),
            Arc::new(sync_repo),
            Arc::new(MockAttachmentDownloader::new()),
            Arc::new(MockImageConverter::new()),
            None,
            url_rules,
        ));

        let relay = relay_use_case(repo, gateway, sync);
        relay.delete(10).await.unwrap();
    }

    #[tokio::test]
    async fn worker_processes_events_in_arrival_order() {
        use mockall::Sequence;

        let today = utc(2025, 1, 2, 0, 0);
        let mut repo = MockDiaryRepository::new();
        repo.expect_get_by_date()
            .times(2)
            .returning(move |_| Ok(Some(entry(100, today))));
        repo.expect_upsert_relayed_message()
            .times(2)
            .returning(|_| Ok(()));
        let mut gateway = MockDiscordGateway::new();
        // 転記が到着順（メッセージ 1 → 2）に行われることを Sequence で検証する
        let mut seq = Sequence::new();
        gateway
            .expect_send_text()
            .withf(|_, content| content.contains("https://discord.com/channels/1/500/1"))
            .times(1)
            .in_sequence(&mut seq)
            .returning(|_, _| Ok(901));
        gateway
            .expect_send_text()
            .withf(|_, content| content.contains("https://discord.com/channels/1/500/2"))
            .times(1)
            .in_sequence(&mut seq)
            .returning(|_, _| Ok(902));
        gateway
            .expect_add_reaction()
            .times(2)
            .returning(|_, _, _| Ok(()));

        let relay = Arc::new(relay_use_case(repo, gateway, sync_service(2)));

        let (tx, rx) = mpsc::channel(8);
        let worker = tokio::spawn(run_relay_worker(relay, rx));

        tx.send(WriteChannelEvent::Posted(message(1, "first")))
            .await
            .unwrap();
        tx.send(WriteChannelEvent::Posted(message(2, "second")))
            .await
            .unwrap();

        // 送信側を閉じるとワーカーは残りを処理してから終了する
        drop(tx);
        worker.await.unwrap();
    }
}
