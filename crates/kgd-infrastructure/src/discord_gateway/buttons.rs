//! クローズ & 新規作成ボタンの生成・検出ヘルパー。

use serenity::all::{
    ActionRowComponent, ButtonKind, CreateActionRow, CreateButton, CreateMessage, Message,
};

use kgd_domain::DIARY_CLOSE_AND_NEW_BUTTON_ID;

/// クローズ & 新規作成ボタンと一緒に送る案内メッセージ。
pub(super) const CLOSE_AND_NEW_PROMPT: &str =
    "日付が変わりました。このスレッドをクローズして新しい日報を作成しますか？";

/// クローズ & 新規作成ボタンの ActionRow を作成する。
pub(super) fn create_close_and_new_action_row() -> CreateActionRow {
    let button = CreateButton::new(DIARY_CLOSE_AND_NEW_BUTTON_ID)
        .label("クローズして新しい日報を作成")
        .style(serenity::all::ButtonStyle::Primary);
    CreateActionRow::Buttons(vec![button])
}

/// 日報スレッドの最初のメッセージを構築する。
pub(super) fn create_diary_thread_initial_message(page_url: &str) -> CreateMessage {
    CreateMessage::new()
        .content(format!("Notion: {}", page_url))
        .components(vec![create_close_and_new_action_row()])
}

/// メッセージにクローズ & 新規作成ボタンが含まれるかを判定する。
pub(super) fn message_has_close_and_new_button(message: &Message) -> bool {
    message.components.iter().any(|row| {
        row.components.iter().any(|component| {
            matches!(
                component,
                ActionRowComponent::Button(button)
                    if matches!(
                        &button.data,
                        ButtonKind::NonLink { custom_id, .. }
                            if custom_id == DIARY_CLOSE_AND_NEW_BUTTON_ID
                    )
            )
        })
    })
}
