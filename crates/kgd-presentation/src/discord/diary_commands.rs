//! 日報スラッシュコマンドの処理。

use anyhow::{Context as _, Result};
use serenity::{
    all::{
        ChannelType, CommandInteraction, CreateInteractionResponse,
        CreateInteractionResponseMessage, EditInteractionResponse,
    },
    client::Context as SerenityContext,
};

use crate::presenter::present_diary_create_outcome;

use super::DiscordController;

impl DiscordController {
    pub(crate) async fn handle_diary(
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
}
