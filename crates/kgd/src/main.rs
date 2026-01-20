mod config;
mod wol;

use anyhow::{Context, Result};
use clap::Parser;
use config::{Config, open_config, write_default_config};
use serenity::all::{
    CommandInteraction, CreateCommand, CreateCommandOption, CreateEmbed, CreateInteractionResponse,
    CreateInteractionResponseMessage, GatewayIntents,
};
use serenity::async_trait;
use serenity::builder::CreateEmbedFooter;
use serenity::client::Context as SerenityContext;
use serenity::model::application::CommandOptionType;
use serenity::prelude::*;
use std::path::PathBuf;
use wol::send_wol_packet;

#[derive(Parser)]
struct Args {
    #[arg(long, default_value = "config.toml")]
    config: PathBuf,

    #[arg(long)]
    init: bool,
}

struct Handler {
    config: Config,
}

#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, ctx: SerenityContext, ready: serenity::model::gateway::Ready) {
        println!("{} is connected!", ready.user.name);

        // Register slash commands
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
            eprintln!("Failed to register commands: {}", e);
        } else {
            println!("Successfully registered slash commands");
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
            eprintln!("Error handling command: {}", e);

            let response = CreateInteractionResponseMessage::new()
                .content(format!("Error: {}", e))
                .ephemeral(true);

            if let Err(e) = command
                .create_response(&ctx.http, CreateInteractionResponse::Message(response))
                .await
            {
                eprintln!("Failed to send error response: {}", e);
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

        // Send WOL packet
        send_wol_packet(server.mac_address, None).context("Failed to send WOL packet")?;

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

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    if args.init {
        write_default_config(&args.config)?;
        println!("Created default configuration at {:?}", args.config);
        return Ok(());
    }

    // Load configuration
    let config = open_config(&args.config).context("Failed to load configuration")?;
    println!(
        "Loaded configuration with {} server(s)",
        config.servers.len()
    );

    // Create client
    let intents = GatewayIntents::empty(); // We don't need any gateway intents for slash commands
    let handler = Handler {
        config: config.clone(),
    };

    let mut client = Client::builder(&config.discord.token, intents)
        .event_handler(handler)
        .await
        .context("Failed to create client")?;

    // Start listening
    println!("Starting bot...");
    client.start().await.context("Client error")?;

    Ok(())
}
