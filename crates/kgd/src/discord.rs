use std::{sync::Arc, time::Duration};

use anyhow::{Context as _, Result};
use serenity::{
    all::{
        ChannelId, CommandInteraction, CreateCommand, CreateCommandOption, CreateEmbed,
        CreateInteractionResponse, CreateInteractionResponseMessage, CreateMessage, GatewayIntents,
        Http,
    },
    async_trait,
    builder::CreateEmbedFooter,
    client::Context as SerenityContext,
    model::application::CommandOptionType,
    prelude::*,
};
use tokio::sync::mpsc;
use tracing::{error, info, warn};

use crate::{config::Config, status::ServerStatus, version, wol::send_wol_packet};

/// Discord イベントを処理するハンドラー。
pub struct Handler {
    /// アプリケーション設定
    config: Config,
}

#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, ctx: SerenityContext, ready: serenity::model::gateway::Ready) {
        info!(user = %ready.user.name, "Bot connected");

        let commands = vec![
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

        if let Err(e) = serenity::all::Command::set_global_commands(&ctx.http, commands).await {
            error!(error = %e, "Failed to register commands");
        } else {
            info!("Slash commands registered");
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
            error!(error = %e, command = %command.data.name, "Command error");

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
}

impl Handler {
    async fn handle_command(
        &self,
        ctx: &SerenityContext,
        command: &CommandInteraction,
    ) -> Result<()> {
        let user_id = command.user.id.get();
        if !self.config.discord.admins.contains(&user_id) {
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
                "✅ Sent WOL packet to {} ({})",
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
            let status_text = if status.online {
                "🟢 Online"
            } else {
                "🔴 Offline"
            };
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
    let intents = GatewayIntents::GUILDS;
    let handler = Handler {
        config: config.clone(),
    };

    let mut client = Client::builder(&config.discord.token, intents)
        .event_handler(handler)
        .await
        .context("Discord クライアントの作成に失敗しました")?;

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
    client
        .start()
        .await
        .context("Discord クライアントでエラーが発生しました")?;

    Ok(())
}

/// ステータスモニターからの通知を受信し、Discordに転送するループを実行する。
async fn run_status_receiver(notifier: StatusNotifier, mut rx: mpsc::Receiver<Vec<ServerStatus>>) {
    while let Some(statuses) = rx.recv().await {
        notifier.send(&statuses).await;
    }
}
