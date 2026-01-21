use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use serenity::all::{
    ChannelId, CommandInteraction, CreateCommand, CreateCommandOption, CreateEmbed,
    CreateInteractionResponse, CreateInteractionResponseMessage, CreateMessage, GatewayIntents,
    Http,
};
use serenity::async_trait;
use serenity::builder::CreateEmbedFooter;
use serenity::client::Context as SerenityContext;
use serenity::model::application::CommandOptionType;
use serenity::prelude::*;
use tracing::{error, info, warn};

use crate::config::{Config, ServerConfig};
use crate::ping::ping;
use crate::wol::send_wol_packet;

pub struct Handler {
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
        ];

        if let Err(e) = serenity::all::Command::set_global_commands(&ctx.http, commands).await {
            error!(error = %e, "Failed to register commands");
        } else {
            info!("Slash commands registered");
        }

        let http = ctx.http.clone();
        let servers = self.config.servers.clone();
        let channel_id = self.config.discord.status_channel_id;
        let interval = self.config.status.interval;
        tokio::spawn(async move {
            run_status_monitor(http, servers, channel_id, interval).await;
        });
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
}

async fn run_status_monitor(
    http: Arc<Http>,
    servers: Vec<ServerConfig>,
    channel_id: u64,
    interval: Duration,
) {
    let channel_id = ChannelId::new(channel_id);
    let ping_timeout = Duration::from_secs(5);

    info!(
        channel_id = channel_id.get(),
        interval = ?interval,
        "Starting status monitor"
    );

    loop {
        let mut embed = CreateEmbed::new()
            .title("Server Status")
            .color(0x00ff00);

        for server in &servers {
            let ip: IpAddr = match server.ip_address.parse() {
                Ok(ip) => ip,
                Err(_) => {
                    embed = embed.field(&server.name, "❌ Invalid IP address", true);
                    continue;
                }
            };

            let is_online = ping(ip, ping_timeout).await;
            let status = if is_online { "🟢 Online" } else { "🔴 Offline" };
            embed = embed.field(&server.name, status, true);
        }

        embed = embed.footer(CreateEmbedFooter::new(format!(
            "Updated every {}",
            humantime::format_duration(interval)
        )));

        let message = CreateMessage::new().embed(embed);
        if let Err(e) = channel_id.send_message(&http, message).await {
            error!(error = %e, "Failed to send status message");
        }

        tokio::time::sleep(interval).await;
    }
}

pub async fn run(config: Config) -> Result<()> {
    let intents = GatewayIntents::GUILDS;
    let handler = Handler {
        config: config.clone(),
    };

    let mut client = Client::builder(&config.discord.token, intents)
        .event_handler(handler)
        .await
        .context("Failed to create client")?;

    info!("Starting bot");
    client.start().await.context("Client error")?;

    Ok(())
}
