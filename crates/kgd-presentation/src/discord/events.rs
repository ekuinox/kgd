//! serenity の EventHandler trait 実装。各イベントを Handler のメソッドへ委譲する。

use std::collections::HashMap;

use serenity::{
    all::{
        ChannelId, CreateCommand, CreateCommandOption, CreateInteractionResponse,
        CreateInteractionResponseMessage, Message, MessageUpdateEvent,
    },
    async_trait,
    client::Context as SerenityContext,
    model::application::CommandOptionType,
    model::id::MessageId,
    prelude::*,
};
use tracing::{error, info};

use super::Handler;

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
        self.on_message(ctx, message).await;
    }

    async fn message_update(
        &self,
        ctx: SerenityContext,
        _old_if_available: Option<Message>,
        _new: Option<Message>,
        event: MessageUpdateEvent,
    ) {
        self.on_message_update(ctx, event).await;
    }

    async fn message_delete(
        &self,
        ctx: SerenityContext,
        channel_id: ChannelId,
        deleted_message_id: MessageId,
        _guild_id: Option<serenity::model::id::GuildId>,
    ) {
        self.on_message_delete(ctx, channel_id, deleted_message_id)
            .await;
    }
}
