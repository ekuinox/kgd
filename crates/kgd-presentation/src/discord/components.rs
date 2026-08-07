//! コンポーネント操作（ボタン押下など）の処理。

use anyhow::Result;
use serenity::{
    all::{
        ComponentInteraction, CreateInteractionResponse, CreateInteractionResponseMessage,
        EditInteractionResponse,
    },
    client::Context as SerenityContext,
};
use tracing::error;

use kgd_application::CloseAndNewPrecheck;
use kgd_domain::DIARY_CLOSE_AND_NEW_BUTTON_ID;

use crate::presenter::{present_close_and_new_failure, present_close_and_new_precheck};

use super::DiscordController;

impl DiscordController {
    pub(crate) async fn handle_component(
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

        // 失敗しても「作成しています...」のまま残らないよう、この場でレスポンスを差し替える
        // （応答済みのため、呼び出し元のエラー通知は届かない）
        if let Err(error) = self.lifecycle.close_and_create_new(channel_id).await {
            error!(error = ?error, channel_id, "Failed to close and create new diary thread");

            component
                .edit_response(
                    &ctx.http,
                    EditInteractionResponse::new()
                        .content(present_close_and_new_failure(&error.to_string())),
                )
                .await?;
        }

        Ok(())
    }
}
