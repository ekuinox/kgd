//! スラッシュコマンド（wol / servers / version）の処理。

use anyhow::{Context as _, Result};
use serenity::{
    all::{CommandInteraction, CreateInteractionResponse, CreateInteractionResponseMessage},
    client::Context as SerenityContext,
};
use tracing::warn;

use crate::presenter::{present_servers, present_version, present_wake_outcome, render_embed};

use super::{DiscordController, is_authorized};

impl DiscordController {
    /// コマンドの認可判定とディスパッチを行う。
    pub(crate) async fn handle_command(
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
}
