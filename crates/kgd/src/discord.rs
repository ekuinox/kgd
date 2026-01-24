use std::{collections::HashMap, sync::Arc, time::Duration};

use anyhow::{Context as _, Result};
use serenity::{
    all::{
        ChannelId, ChannelType, CommandInteraction, CreateCommand, CreateCommandOption,
        CreateEmbed, CreateForumPost, CreateInteractionResponse, CreateInteractionResponseMessage,
        CreateMessage, EditThread, GatewayIntents, Http, Message, ReactionType,
    },
    async_trait,
    builder::CreateEmbedFooter,
    client::Context as SerenityContext,
    model::application::CommandOptionType,
    prelude::*,
};
use tokio::sync::{RwLock, mpsc};
use tracing::{error, info, warn};

use crate::{
    config::Config,
    diary::{DiaryEntry, DiaryStore, MessageSyncer, NotionClient, today_jst},
    status::ServerStatus,
    version,
    wol::send_wol_packet,
};

/// Discord イベントを処理するハンドラー。
pub struct Handler {
    /// アプリケーション設定
    config: Config,
    /// 日報ストア（日報機能が有効な場合）
    diary_store: Option<Arc<RwLock<DiaryStore>>>,
    /// Notion クライアント（日報機能が有効な場合）
    notion_client: Option<Arc<NotionClient>>,
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

        // 日報機能が有効な場合はコマンドを追加
        if self.config.diary.is_some() {
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
                    )),
            );
        }

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
        if let serenity::model::application::Interaction::Command(command) = interaction
            && let Err(e) = self.handle_command(&ctx, &command).await
        {
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

    async fn message(&self, ctx: SerenityContext, message: Message) {
        // Bot 自身のメッセージは無視
        if message.author.bot {
            return;
        }

        // 日報機能が無効なら何もしない
        let Some(diary_config) = &self.config.diary else {
            return;
        };

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
        let store = self.diary_store.as_ref().unwrap().read().await;
        let Some(entry) = store.get_by_thread(message.channel_id.get()) else {
            return;
        };
        let page_id = entry.page_id.clone();
        drop(store);

        // Notion に同期
        let notion = self.notion_client.as_ref().unwrap();
        let syncer = MessageSyncer::new(notion.as_ref());
        match syncer.sync_message(&page_id, &message).await {
            Ok(true) => {
                // 成功したらリアクションを付ける
                let reaction = ReactionType::Unicode(diary_config.sync_reaction.clone());
                if let Err(e) = message.react(&ctx.http, reaction).await {
                    error!(error = %e, "Failed to add sync reaction");
                }
            }
            Ok(false) => {
                // スキップ (空メッセージなど)
            }
            Err(e) => {
                error!(error = %e, "Failed to sync message to Notion");
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
        if !self.config.discord.admins.is_empty() && !self.config.discord.admins.contains(&user_id)
        {
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

        let server = self
            .config
            .find_server(server_name)
            .context(format!("Server '{}' not found", server_name))?;

        send_wol_packet(server.mac_address, None).context("Failed to send WOL packet")?;
        info!(server = %server.name, mac = %server.mac_address, "WOL packet sent");

        let response = CreateInteractionResponseMessage::new()
            .content(format!(
                "Sent WOL packet to {} ({})",
                server.name, server.mac_address
            ))
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
        let mut embed = CreateEmbed::new()
            .title("Configured Servers")
            .color(0x00ff00);

        for server in &self.config.servers {
            let field_value = format!(
                "**IP:** {}\n**MAC:** {}\n**Description:** {}",
                server.ip_address, server.mac_address, server.description
            );
            embed = embed.field(&server.name, field_value, false);
        }

        embed = embed.footer(CreateEmbedFooter::new(format!(
            "Total: {} server(s)",
            self.config.servers.len()
        )));

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
        let embed = CreateEmbed::new()
            .title("kgd")
            .color(0x5865f2)
            .field("Version", version::VERSION, true)
            .field("Git SHA", version::GIT_SHA, true)
            .field("Target", version::TARGET_TRIPLE, true)
            .field("Built", version::BUILD_DATE, false);

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
            _ => Ok(()),
        }
    }

    async fn handle_diary_new(
        &self,
        ctx: &SerenityContext,
        command: &CommandInteraction,
    ) -> Result<()> {
        let diary_config = self
            .config
            .diary
            .as_ref()
            .context("Diary feature is not configured")?;

        // 今日の日付を JST で取得
        let date = today_jst();

        // 既に今日の日報が存在するかチェック
        {
            let store = self.diary_store.as_ref().unwrap().read().await;
            if let Some(entry) = store.get_by_date(&date) {
                let response = CreateInteractionResponseMessage::new()
                    .content(format!(
                        "今日の日報は既に作成されています: <#{}>",
                        entry.thread_id
                    ))
                    .ephemeral(true);
                command
                    .create_response(&ctx.http, CreateInteractionResponse::Message(response))
                    .await?;
                return Ok(());
            }
        }

        // Notion ページを作成
        let notion = self.notion_client.as_ref().unwrap();
        let (page_id, page_url) = notion
            .create_diary_page(&date)
            .await
            .context("Notion ページの作成に失敗しました")?;

        // Discord フォーラムにスレッドを作成
        let forum_channel = ChannelId::new(diary_config.forum_channel_id);
        let initial_message = CreateMessage::new().content(format!("Notion: {}", page_url));
        let forum_post = CreateForumPost::new(date.clone(), initial_message);

        let thread = forum_channel
            .create_forum_post(&ctx.http, forum_post)
            .await
            .context("フォーラムスレッドの作成に失敗しました")?;

        // 紐付け情報を保存
        let entry = DiaryEntry {
            thread_id: thread.id.get(),
            page_id,
            page_url: page_url.clone(),
            date: date.clone(),
            created_at: chrono::Utc::now(),
        };

        {
            let mut store = self.diary_store.as_ref().unwrap().write().await;
            store.insert(entry)?;
        }

        info!(date = %date, thread_id = thread.id.get(), "Diary created");

        // 成功レスポンス
        let response = CreateInteractionResponseMessage::new()
            .content(format!(
                "日報を作成しました\nスレッド: <#{}>\nNotion: {}",
                thread.id, page_url
            ))
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
        {
            let store = self.diary_store.as_ref().unwrap().read().await;
            if store.get_by_thread(command.channel_id.get()).is_none() {
                let response = CreateInteractionResponseMessage::new()
                    .content("このスレッドは日報スレッドではありません")
                    .ephemeral(true);
                command
                    .create_response(&ctx.http, CreateInteractionResponse::Message(response))
                    .await?;
                return Ok(());
            }
        }

        // スレッドをアーカイブ (クローズ)
        let edit = EditThread::new().archived(true);
        command
            .channel_id
            .edit_thread(&ctx.http, edit)
            .await
            .context("スレッドのクローズに失敗しました")?;

        info!(thread_id = command.channel_id.get(), "Diary thread closed");

        // 成功レスポンス
        let response = CreateInteractionResponseMessage::new()
            .content("日報スレッドをクローズしました")
            .ephemeral(false);

        command
            .create_response(&ctx.http, CreateInteractionResponse::Message(response))
            .await?;

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
    /// サーバーステータスをDiscordチャンネルに埋め込みメッセージとして送信する。
    pub async fn send(&self, statuses: &[ServerStatus]) {
        let mut embed = CreateEmbed::new().title("Server Status").color(0x00ff00);

        for status in statuses {
            let status_text = if status.online { "Online" } else { "Offline" };
            embed = embed.field(&status.name, status_text, true);
        }

        embed = embed.footer(CreateEmbedFooter::new(format!(
            "Updated every {}",
            humantime::format_duration(self.interval)
        )));

        let message = CreateMessage::new().embed(embed);
        if let Err(e) = self.channel_id.send_message(&self.http, message).await {
            error!(error = %e, "Failed to send status message");
        }
    }
}

/// Discord Bot を起動し、イベントループを開始する。
pub async fn run(config: Config, status_rx: mpsc::Receiver<Vec<ServerStatus>>) -> Result<()> {
    let mut intents = GatewayIntents::GUILDS;

    // 日報機能が有効な場合はメッセージイベントも購読
    let (diary_store, notion_client) = if let Some(diary_config) = &config.diary {
        intents |= GatewayIntents::GUILD_MESSAGES | GatewayIntents::MESSAGE_CONTENT;

        let store =
            DiaryStore::load(&diary_config.store_path).context("Failed to load diary store")?;
        let notion = NotionClient::new(
            &diary_config.notion_token,
            &diary_config.notion_database_id,
            &diary_config.notion_title_property,
        )
        .context("Failed to create Notion client")?;

        (Some(Arc::new(RwLock::new(store))), Some(Arc::new(notion)))
    } else {
        (None, None)
    };

    let handler = Handler {
        config: config.clone(),
        diary_store,
        notion_client,
    };

    let mut client = Client::builder(&config.discord.token, intents)
        .event_handler(handler)
        .await
        .context("Failed to create Discord client")?;

    let http = client.http.clone();
    let channel_id = ChannelId::new(config.discord.status_channel_id);
    let interval = config.status.interval;

    let notifier = StatusNotifier {
        http,
        channel_id,
        interval,
    };

    tokio::spawn(run_status_receiver(notifier, status_rx));

    info!("Starting bot");
    client.start().await.context("Discord client error")?;

    Ok(())
}

/// ステータスモニターからの通知を受信し、Discordに転送するループを実行する。
async fn run_status_receiver(notifier: StatusNotifier, mut rx: mpsc::Receiver<Vec<ServerStatus>>) {
    while let Some(statuses) = rx.recv().await {
        notifier.send(&statuses).await;
    }
}
