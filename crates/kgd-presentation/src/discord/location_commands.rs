//! 位置ログのスラッシュコマンドの処理。

use anyhow::{Context as _, Result, bail};
use serenity::{
    all::{
        CommandDataOptionValue, CommandInteraction, CreateAttachment, CreateInteractionResponse,
        CreateInteractionResponseMessage, EditInteractionResponse,
    },
    client::Context as SerenityContext,
};

use kgd_domain::format_empty_location_report;

use crate::presenter::{present_location_report, render_embed, resolve_report_date};

use super::DiscordController;

impl DiscordController {
    /// `/location` を処理する。
    pub(crate) async fn handle_location(
        &self,
        ctx: &SerenityContext,
        command: &CommandInteraction,
    ) -> Result<()> {
        let Some(location) = &self.location_report else {
            return Ok(());
        };
        let subcommand = command
            .data
            .options
            .first()
            .context("Subcommand not provided")?;
        if subcommand.name != "report" {
            return Ok(());
        }
        let CommandDataOptionValue::SubCommand(options) = &subcommand.value else {
            bail!("Unexpected option value for /location report");
        };
        let input = options
            .iter()
            .find(|option| option.name == "date")
            .and_then(|option| option.value.as_str());

        let now = location.clock.now();
        let date = match resolve_report_date(input, &location.calendar, now) {
            Ok(date) => date,
            Err(error) => {
                let response = CreateInteractionResponseMessage::new()
                    .content(error.message())
                    .ephemeral(true);
                command
                    .create_response(&ctx.http, CreateInteractionResponse::Message(response))
                    .await?;
                return Ok(());
            }
        };

        // 描画に数秒かかりうるため、先に本人だけに見える形で応答を保留する
        command.defer_ephemeral(&ctx.http).await?;

        let report = location.build.build(date, Some(now)).await?;
        let response = match &report.image {
            Some(png) => {
                let filename = format!("location-{}.png", date.format("%Y-%m-%d"));
                let embed = render_embed(&present_location_report(&report, &location.calendar))
                    .attachment(&filename);
                EditInteractionResponse::new()
                    .embed(embed)
                    .new_attachment(CreateAttachment::bytes(png.clone(), filename))
            }
            None => EditInteractionResponse::new().content(format_empty_location_report(date)),
        };
        command.edit_response(&ctx.http, response).await?;

        Ok(())
    }
}
