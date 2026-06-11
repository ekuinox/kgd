//! Discord のイベントを受けてユースケースを呼び出すコントローラ。

use std::{collections::HashMap, sync::Arc, time::Duration};

use anyhow::{Context as _, Result};
use serenity::{
    all::{
        ChannelId, ChannelType, CommandInteraction, ComponentInteraction, CreateCommand,
        CreateCommandOption, CreateInteractionResponse, CreateInteractionResponseMessage,
        CreateMessage, EditInteractionResponse, Http, Message, MessageUpdateEvent,
    },
    async_trait,
    client::Context as SerenityContext,
    model::application::CommandOptionType,
    model::id::MessageId,
    prelude::*,
};
use tokio::sync::mpsc;
use tracing::{error, info, warn};

use kgd_application::{
    CloseAndNewPrecheck, ManageDiaryLifecycle, RelayWriteChannelMessage, RunDiaryMaintenance,
    SyncDiaryMessage, WakeServer, ports::DiaryRepository,
};
use kgd_domain::{
    DIARY_CLOSE_AND_NEW_BUTTON_ID, ServerStatus, ServerTarget, SyncAttachment, SyncMessage,
    merge_forwarded_content,
};

use crate::presenter::{
    VersionInfo, present_close_and_new_precheck, present_diary_create_outcome,
    present_server_status, present_servers, present_version, present_wake_outcome, render_embed,
};

/// コマンド実行が許可されているか判定する純粋関数。
///
/// 管理者リストが空の場合は全員許可。空でない場合はリストに含まれるユーザーのみ許可。
fn is_authorized(admins: &[u64], user_id: u64) -> bool {
    admins.is_empty() || admins.contains(&user_id)
}

/// serenity の Message をドメインの [`SyncMessage`] に変換する。
///
/// 転送 (Forward) メッセージは本文・添付がスナップショット側に入るため統合する。
fn to_sync_message(message: &Message) -> SyncMessage {
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

/// Handler の表示・認可まわりの設定。
#[derive(Debug, Clone)]
pub struct HandlerSettings {
    /// コマンド実行を許可する管理者のユーザー ID 一覧
    pub admins: Vec<u64>,
    /// バージョン情報
    pub version_info: VersionInfo,
    /// 操作対象のサーバー一覧
    pub servers: Vec<ServerTarget>,
    /// 日報の書き込み用チャンネル ID
    pub write_channel_id: u64,
}

/// Discord イベントを処理するハンドラー。
#[derive(Clone)]
pub struct Handler {
    /// 表示・認可まわりの設定
    settings: HandlerSettings,
    /// 日報リポジトリ
    diary_store: Arc<dyn DiaryRepository>,
    /// メッセージ同期ユースケース
    sync_service: Arc<SyncDiaryMessage>,
    /// 定期メンテナンスユースケース
    maintenance: Arc<RunDiaryMaintenance>,
    /// 日報ライフサイクルユースケース
    lifecycle: Arc<ManageDiaryLifecycle>,
    /// 書き込み用チャンネル転記ユースケース
    relay: Arc<RelayWriteChannelMessage>,
    /// WOL ユースケース
    wake_server: Arc<WakeServer>,
}

impl Handler {
    /// 新しい Handler を作成する。
    pub fn new(
        settings: HandlerSettings,
        diary_store: Arc<dyn DiaryRepository>,
        sync_service: Arc<SyncDiaryMessage>,
        maintenance: Arc<RunDiaryMaintenance>,
        lifecycle: Arc<ManageDiaryLifecycle>,
        relay: Arc<RelayWriteChannelMessage>,
        wake_server: Arc<WakeServer>,
    ) -> Self {
        Self {
            settings,
            diary_store,
            sync_service,
            maintenance,
            lifecycle,
            relay,
            wake_server,
        }
    }

    /// 書き込み用チャンネルでのメッセージ編集を転記ユースケースへ引き渡す。
    async fn handle_write_channel_update(
        &self,
        ctx: &SerenityContext,
        event: &MessageUpdateEvent,
    ) -> Result<()> {
        // コンテンツがない場合は無視
        // （埋め込み展開などユーザー編集以外の更新イベントを除外する）
        if event.content.is_none() {
            return Ok(());
        }

        // 編集後の内容を取得
        let message = event
            .channel_id
            .message(&ctx.http, event.id)
            .await
            .context("Failed to fetch updated write channel message")?;

        // REST で取得したメッセージは guild_id を持たないためイベント側から補う
        let mut sync_message = to_sync_message(&message);
        sync_message.guild_id = event.guild_id.map(|guild_id| guild_id.get());

        self.relay.update(&sync_message).await
    }
}

#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, ctx: SerenityContext, ready: serenity::model::gateway::Ready) {
        info!(user = %ready.user.name, "Bot connected");

        let mut commands = vec![
            CreateCommand::new("wol")
                .description("Wake up a server using Wake-on-LAN")
                .add_option(
                    CreateCommandOption::new(
                        CommandOptionType::String,
                        "server",
                        "Server name to wake up",
                    )
                    .required(true),
                ),
            CreateCommand::new("servers").description("List all configured servers"),
            CreateCommand::new("version").description("Show bot version information"),
        ];

        // 日報コマンドを追加
        commands.push(
            CreateCommand::new("diary")
                .description("日報機能")
                .add_option(CreateCommandOption::new(
                    CommandOptionType::SubCommand,
                    "new",
                    "新しい日報を作成する",
                ))
                .add_option(CreateCommandOption::new(
                    CommandOptionType::SubCommand,
                    "close",
                    "日報スレッドをクローズする",
                ))
                .add_option(CreateCommandOption::new(
                    CommandOptionType::SubCommand,
                    "sync",
                    "Sync unsynced messages in this diary thread",
                )),
        );

        match serenity::all::Command::set_global_commands(&ctx.http, commands).await {
            Ok(commands) => {
                let commands = commands
                    .iter()
                    .map(|command| {
                        (
                            command.name.as_str(),
                            (command.version.get(), command.version.created_at().to_utc()),
                        )
                    })
                    .collect::<HashMap<_, _>>();
                info!(?commands, "Slash commands registered");
            }
            Err(e) => {
                error!(error = %e, "Failed to register commands");
            }
        }
    }

    async fn interaction_create(
        &self,
        ctx: SerenityContext,
        interaction: serenity::model::application::Interaction,
    ) {
        match interaction {
            serenity::model::application::Interaction::Command(command) => {
                if let Err(e) = self.handle_command(&ctx, &command).await {
                    error!(error = ?e, command = %command.data.name, "Command error");

                    let response = CreateInteractionResponseMessage::new()
                        .content(format!("Error: {}", e))
                        .ephemeral(true);

                    if let Err(e) = command
                        .create_response(&ctx.http, CreateInteractionResponse::Message(response))
                        .await
                    {
                        error!(error = %e, "Failed to send error response");
                    }
                }
            }
            serenity::model::application::Interaction::Component(component) => {
                if let Err(e) = self.handle_component(&ctx, &component).await {
                    error!(error = ?e, custom_id = %component.data.custom_id, "Component interaction error");

                    let response = CreateInteractionResponseMessage::new()
                        .content(format!("Error: {}", e))
                        .ephemeral(true);

                    if let Err(e) = component
                        .create_response(&ctx.http, CreateInteractionResponse::Message(response))
                        .await
                    {
                        error!(error = %e, "Failed to send error response");
                    }
                }
            }
            _ => {}
        }
    }

    async fn message(&self, ctx: SerenityContext, message: Message) {
        // Bot 自身のメッセージは無視
        // 注意: write_channel 分岐より先に判定すること
        // （Bot が送る転記メッセージを処理対象にするとループする）
        if message.author.bot {
            return;
        }

        // 書き込み用チャンネルへの投稿は最新の日報スレッドへ転記する
        if self.settings.write_channel_id == message.channel_id.get() {
            let sync_message = to_sync_message(&message);
            if let Err(error) = self.relay.relay(&sync_message).await {
                error!(error = %error, "Failed to handle write channel message");
            }
            return;
        }

        // スレッドでない場合は無視
        let Ok(channel) = message.channel(&ctx).await else {
            return;
        };
        let Some(guild_channel) = channel.guild() else {
            return;
        };
        if guild_channel.kind != ChannelType::PublicThread {
            return;
        }

        // 該当スレッドの日報エントリを取得
        let Ok(Some(entry)) = self
            .diary_store
            .get_by_thread(message.channel_id.get())
            .await
        else {
            return;
        };
        let page_id = entry.page_id.clone();

        // Notion に同期（成功時はリアクションを付与する）
        let sync_message = to_sync_message(&message);
        match self
            .maintenance
            .sync_message_with_reaction(&page_id, &sync_message)
            .await
        {
            Ok((true, block_count)) => {
                info!(
                    thread_id = message.channel_id.get(),
                    message_id = message.id.get(),
                    blocks = block_count,
                    "Message synced to Notion"
                );
            }
            Ok(_) => {
                // スキップ (空メッセージなど)
            }
            Err(e) => {
                error!(error = %e, "Failed to sync message to Notion");
            }
        }
    }

    async fn message_update(
        &self,
        ctx: SerenityContext,
        _old_if_available: Option<Message>,
        _new: Option<Message>,
        event: MessageUpdateEvent,
    ) {
        // Bot 自身のメッセージは無視
        if event.author.as_ref().is_some_and(|a| a.bot) {
            return;
        }

        // 書き込み用チャンネルでの編集は Notion ブロックと転記メッセージに反映する
        if self.settings.write_channel_id == event.channel_id.get() {
            if let Err(error) = self.handle_write_channel_update(&ctx, &event).await {
                error!(error = %error, "Failed to handle write channel message update");
            }
            return;
        }

        // スレッドでない場合は無視
        let Ok(channel) = event.channel_id.to_channel(&ctx).await else {
            return;
        };
        let Some(guild_channel) = channel.guild() else {
            return;
        };
        if guild_channel.kind != ChannelType::PublicThread {
            return;
        }

        // 該当スレッドの日報エントリを取得
        let Ok(Some(_entry)) = self.diary_store.get_by_thread(event.channel_id.get()).await else {
            return;
        };

        // コンテンツがない場合は無視
        let Some(content) = event.content else {
            return;
        };

        // メッセージを取得して更新
        let Ok(message) = event.channel_id.message(&ctx.http, event.id).await else {
            return;
        };

        // メッセージの内容で上書きして更新
        let mut sync_message = to_sync_message(&message);
        sync_message.content = content;

        match self.sync_service.update(&sync_message).await {
            Ok(true) => {
                info!(
                    thread_id = event.channel_id.get(),
                    message_id = event.id.get(),
                    "Message updated in Notion"
                );
            }
            Ok(false) => {
                // 対応するブロックがなかった（新規メッセージの可能性）
            }
            Err(e) => {
                error!(error = %e, "Failed to update message in Notion");
            }
        }
    }

    async fn message_delete(
        &self,
        ctx: SerenityContext,
        channel_id: ChannelId,
        deleted_message_id: MessageId,
        _guild_id: Option<serenity::model::id::GuildId>,
    ) {
        // 書き込み用チャンネルでの削除は Notion ブロックと転記メッセージを削除する
        if self.settings.write_channel_id == channel_id.get() {
            if let Err(error) = self.relay.delete(deleted_message_id.get()).await {
                error!(error = %error, "Failed to handle write channel message delete");
            }
            return;
        }

        // スレッドでない場合は無視
        let Ok(channel) = channel_id.to_channel(&ctx).await else {
            return;
        };
        let Some(guild_channel) = channel.guild() else {
            return;
        };
        if guild_channel.kind != ChannelType::PublicThread {
            return;
        }

        // 該当スレッドの日報エントリを取得
        let Ok(Some(_entry)) = self.diary_store.get_by_thread(channel_id.get()).await else {
            return;
        };

        // Notion から対応するブロックを削除
        match self.sync_service.delete(deleted_message_id.get()).await {
            Ok(true) => {
                info!(
                    thread_id = channel_id.get(),
                    message_id = deleted_message_id.get(),
                    "Message deleted from Notion"
                );
            }
            Ok(false) => {
                // 対応するブロックがなかった
            }
            Err(e) => {
                error!(error = %e, "Failed to delete message from Notion");
            }
        }
    }
}

impl Handler {
    async fn handle_command(
        &self,
        ctx: &SerenityContext,
        command: &CommandInteraction,
    ) -> Result<()> {
        let user_id = command.user.id.get();
        if !is_authorized(&self.settings.admins, user_id) {
            warn!(user_id, "Unauthorized access attempt");
            let response = CreateInteractionResponseMessage::new()
                .content("You are not authorized to use this bot.")
                .ephemeral(true);
            command
                .create_response(&ctx.http, CreateInteractionResponse::Message(response))
                .await?;
            return Ok(());
        }

        match command.data.name.as_str() {
            "wol" => self.handle_wol(ctx, command).await,
            "servers" => self.handle_servers(ctx, command).await,
            "version" => self.handle_version(ctx, command).await,
            "diary" => self.handle_diary(ctx, command).await,
            _ => Ok(()),
        }
    }

    async fn handle_wol(&self, ctx: &SerenityContext, command: &CommandInteraction) -> Result<()> {
        let server_name = command
            .data
            .options
            .first()
            .and_then(|opt| opt.value.as_str())
            .context("Server name not provided")?;

        let outcome = self.wake_server.wake(server_name)?;
        let Some(message) = present_wake_outcome(&outcome) else {
            anyhow::bail!("Server '{}' not found", server_name);
        };

        let response = CreateInteractionResponseMessage::new()
            .content(message)
            .ephemeral(false);

        command
            .create_response(&ctx.http, CreateInteractionResponse::Message(response))
            .await?;

        Ok(())
    }

    async fn handle_servers(
        &self,
        ctx: &SerenityContext,
        command: &CommandInteraction,
    ) -> Result<()> {
        let embed = render_embed(&present_servers(&self.settings.servers));

        let response = CreateInteractionResponseMessage::new()
            .embed(embed)
            .ephemeral(false);

        command
            .create_response(&ctx.http, CreateInteractionResponse::Message(response))
            .await?;

        Ok(())
    }

    async fn handle_version(
        &self,
        ctx: &SerenityContext,
        command: &CommandInteraction,
    ) -> Result<()> {
        let embed = render_embed(&present_version(&self.settings.version_info));

        let response = CreateInteractionResponseMessage::new()
            .embed(embed)
            .ephemeral(false);

        command
            .create_response(&ctx.http, CreateInteractionResponse::Message(response))
            .await?;

        Ok(())
    }

    async fn handle_diary(
        &self,
        ctx: &SerenityContext,
        command: &CommandInteraction,
    ) -> Result<()> {
        let subcommand = command
            .data
            .options
            .first()
            .context("Subcommand not provided")?;

        match subcommand.name.as_str() {
            "new" => self.handle_diary_new(ctx, command).await,
            "close" => self.handle_diary_close(ctx, command).await,
            "sync" => self.handle_diary_sync(ctx, command).await,
            _ => Ok(()),
        }
    }

    async fn handle_diary_new(
        &self,
        ctx: &SerenityContext,
        command: &CommandInteraction,
    ) -> Result<()> {
        let outcome = self.lifecycle.create_or_reopen().await?;

        let response = CreateInteractionResponseMessage::new()
            .content(present_diary_create_outcome(&outcome))
            .ephemeral(false);

        command
            .create_response(&ctx.http, CreateInteractionResponse::Message(response))
            .await?;

        Ok(())
    }

    async fn handle_diary_close(
        &self,
        ctx: &SerenityContext,
        command: &CommandInteraction,
    ) -> Result<()> {
        // スレッド内からの呼び出しか確認
        let channel = command.channel_id.to_channel(&ctx.http).await?;
        let Some(guild_channel) = channel.guild() else {
            let response = CreateInteractionResponseMessage::new()
                .content("このコマンドはサーバー内でのみ使用できます")
                .ephemeral(true);
            command
                .create_response(&ctx.http, CreateInteractionResponse::Message(response))
                .await?;
            return Ok(());
        };

        if guild_channel.kind != ChannelType::PublicThread {
            let response = CreateInteractionResponseMessage::new()
                .content("このコマンドは日報スレッド内から実行してください")
                .ephemeral(true);
            command
                .create_response(&ctx.http, CreateInteractionResponse::Message(response))
                .await?;
            return Ok(());
        }

        // 該当スレッドが日報スレッドか確認
        if self
            .diary_store
            .get_by_thread(command.channel_id.get())
            .await?
            .is_none()
        {
            let response = CreateInteractionResponseMessage::new()
                .content("このスレッドは日報スレッドではありません")
                .ephemeral(true);
            command
                .create_response(&ctx.http, CreateInteractionResponse::Message(response))
                .await?;
            return Ok(());
        }

        // 先にレスポンスを返す（アーカイブ後はレスポンスを返せないため）
        let response = CreateInteractionResponseMessage::new()
            .content("日報スレッドをクローズしています...")
            .ephemeral(false);

        command
            .create_response(&ctx.http, CreateInteractionResponse::Message(response))
            .await?;

        self.lifecycle.close(command.channel_id.get()).await?;

        Ok(())
    }

    /// コンポーネント操作を処理する。
    /// 現在の日報スレッドを対象に、未同期メッセージの再同期を手動実行する。
    async fn handle_diary_sync(
        &self,
        ctx: &SerenityContext,
        command: &CommandInteraction,
    ) -> Result<()> {
        let channel = command.channel_id.to_channel(&ctx.http).await?;
        let Some(guild_channel) = channel.guild() else {
            let response = CreateInteractionResponseMessage::new()
                .content("このコマンドはサーバー内の日報スレッドでのみ使用できます")
                .ephemeral(true);
            command
                .create_response(&ctx.http, CreateInteractionResponse::Message(response))
                .await?;
            return Ok(());
        };

        if guild_channel.kind != ChannelType::PublicThread {
            let response = CreateInteractionResponseMessage::new()
                .content("このコマンドは日報スレッド内で実行してください")
                .ephemeral(true);
            command
                .create_response(&ctx.http, CreateInteractionResponse::Message(response))
                .await?;
            return Ok(());
        }

        if self
            .diary_store
            .get_by_thread(command.channel_id.get())
            .await?
            .is_none()
        {
            let response = CreateInteractionResponseMessage::new()
                .content("このスレッドは日報スレッドとして登録されていません")
                .ephemeral(true);
            command
                .create_response(&ctx.http, CreateInteractionResponse::Message(response))
                .await?;
            return Ok(());
        }

        command.defer_ephemeral(&ctx.http).await?;

        let report = self
            .maintenance
            .sync_missing_in_thread(command.channel_id.get())
            .await?;

        let result_message = format!(
            "日報スレッドの未同期メッセージを確認しました。\n確認件数: {}件\n新規同期: {}件\n既に同期済み: {}件\nスキップ: {}件",
            report.checked_messages,
            report.synced_messages,
            report.already_synced_messages,
            report.skipped_messages
        );

        command
            .edit_response(
                &ctx.http,
                EditInteractionResponse::new().content(result_message),
            )
            .await?;

        Ok(())
    }

    async fn handle_component(
        &self,
        ctx: &SerenityContext,
        component: &ComponentInteraction,
    ) -> Result<()> {
        if component.data.custom_id == DIARY_CLOSE_AND_NEW_BUTTON_ID {
            self.handle_diary_close_and_new(ctx, component).await
        } else {
            Ok(())
        }
    }

    /// 日報スレッドをクローズして新しいスレッドを作成する。
    async fn handle_diary_close_and_new(
        &self,
        ctx: &SerenityContext,
        component: &ComponentInteraction,
    ) -> Result<()> {
        let channel_id = component.channel_id.get();

        let precheck = self.lifecycle.close_and_new_precheck(channel_id).await?;

        if precheck == CloseAndNewPrecheck::NotDiaryThread {
            anyhow::bail!("このスレッドは日報スレッドではありません");
        }

        if let Some(message) = present_close_and_new_precheck(&precheck) {
            let response = CreateInteractionResponseMessage::new()
                .content(message)
                .ephemeral(false);
            component
                .create_response(&ctx.http, CreateInteractionResponse::Message(response))
                .await?;
            return Ok(());
        }

        // 先にレスポンスを返す（アーカイブ後はレスポンスを返せないため）
        let response = CreateInteractionResponseMessage::new()
            .content("日報スレッドをクローズして新しいスレッドを作成しています...")
            .ephemeral(false);

        component
            .create_response(&ctx.http, CreateInteractionResponse::Message(response))
            .await?;

        self.lifecycle.close_and_create_new(channel_id).await?;

        Ok(())
    }
}

/// サーバーステータスをDiscordチャンネルに通知するための構造体。
pub struct StatusNotifier {
    /// Discord API クライアント
    http: Arc<Http>,
    /// 通知先チャンネルID
    channel_id: ChannelId,
    /// ステータスチェック間隔（フッター表示用）
    interval: Duration,
}

impl StatusNotifier {
    /// 新しい StatusNotifier を作成する。
    pub fn new(http: Arc<Http>, channel_id: ChannelId, interval: Duration) -> Self {
        Self {
            http,
            channel_id,
            interval,
        }
    }

    /// サーバーステータスをDiscordチャンネルに埋め込みメッセージとして送信する。
    pub async fn send(&self, statuses: &[ServerStatus]) {
        let embed = render_embed(&present_server_status(statuses, self.interval));

        let message = CreateMessage::new().embed(embed);
        if let Err(e) = self.channel_id.send_message(&self.http, message).await {
            error!(error = %e, "Failed to send status message");
        }
    }
}

/// ステータスモニターからの通知を受信し、Discordに転送するループを実行する。
pub async fn run_status_receiver(
    notifier: StatusNotifier,
    mut rx: mpsc::Receiver<Vec<ServerStatus>>,
) {
    while let Some(statuses) = rx.recv().await {
        notifier.send(&statuses).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_authorized_allows_everyone_when_admins_empty() {
        assert!(is_authorized(&[], 1234));
    }

    #[test]
    fn is_authorized_allows_listed_user() {
        assert!(is_authorized(&[1, 2, 3], 2));
    }

    #[test]
    fn is_authorized_rejects_unlisted_user() {
        assert!(!is_authorized(&[1, 2, 3], 99));
    }
}
