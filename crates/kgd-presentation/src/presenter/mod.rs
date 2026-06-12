//! ユースケースの結果からユーザー向けメッセージ・埋め込みを組み立てるプレゼンター。
//!
//! 文言の組み立ては serenity 非依存の純粋関数として実装し、単体テスト可能にする。
//! serenity 型への変換は [`render_embed`] のみが行う。

use std::time::Duration;

use serenity::all::CreateEmbed;
use serenity::builder::CreateEmbedFooter;

use kgd_application::{CloseAndNewPrecheck, DiaryCreateOutcome, WakeOutcome};
use kgd_domain::{ServerStatus, ServerTarget};

/// バージョン情報の表示用 DTO。
///
/// ビルド時に埋め込まれる定数はバイナリ側にあるため、表示に必要な値を引数で受ける。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionInfo {
    /// バージョン文字列
    pub version: String,
    /// Git コミット SHA
    pub git_sha: String,
    /// ビルドターゲット
    pub target_triple: String,
    /// ビルド日時
    pub build_date: String,
}

/// 埋め込みメッセージのフィールド。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbedField {
    /// フィールド名
    pub name: String,
    /// フィールド値
    pub value: String,
    /// インライン表示するかどうか
    pub inline: bool,
}

/// 埋め込みメッセージの内容。
///
/// serenity の `CreateEmbed` に依存せずに内容を表すための DTO。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbedSpec {
    /// タイトル
    pub title: String,
    /// 色
    pub color: u32,
    /// フィールド一覧
    pub fields: Vec<EmbedField>,
    /// フッター
    pub footer: Option<String>,
}

/// WOL 送信結果のメッセージを組み立てる。
pub fn present_wake_outcome(outcome: &WakeOutcome) -> Option<String> {
    match outcome {
        WakeOutcome::ServerNotFound => None,
        WakeOutcome::Sent { name, mac_address } => {
            Some(format!("Sent WOL packet to {} ({})", name, mac_address))
        }
    }
}

/// サーバー一覧の埋め込みを組み立てる。
pub fn present_servers(servers: &[ServerTarget]) -> EmbedSpec {
    EmbedSpec {
        title: "Configured Servers".to_string(),
        color: 0x00ff00,
        fields: servers
            .iter()
            .map(|server| EmbedField {
                name: server.name.clone(),
                value: format!(
                    "**IP:** {}\n**MAC:** {}\n**Description:** {}",
                    server.ip_address, server.mac_address, server.description
                ),
                inline: false,
            })
            .collect(),
        footer: Some(format!("Total: {} server(s)", servers.len())),
    }
}

/// バージョン情報の埋め込みを組み立てる。
pub fn present_version(info: &VersionInfo) -> EmbedSpec {
    EmbedSpec {
        title: "kgd".to_string(),
        color: 0x5865f2,
        fields: vec![
            EmbedField {
                name: "Version".to_string(),
                value: info.version.clone(),
                inline: true,
            },
            EmbedField {
                name: "Git SHA".to_string(),
                value: info.git_sha.clone(),
                inline: true,
            },
            EmbedField {
                name: "Target".to_string(),
                value: info.target_triple.clone(),
                inline: true,
            },
            EmbedField {
                name: "Built".to_string(),
                value: info.build_date.clone(),
                inline: false,
            },
        ],
        footer: None,
    }
}

/// サーバーステータス通知の埋め込みを組み立てる。
pub fn present_server_status(statuses: &[ServerStatus], interval: Duration) -> EmbedSpec {
    EmbedSpec {
        title: "Server Status".to_string(),
        color: 0x00ff00,
        fields: statuses
            .iter()
            .map(|status| EmbedField {
                name: status.name.clone(),
                value: if status.online { "Online" } else { "Offline" }.to_string(),
                inline: true,
            })
            .collect(),
        footer: Some(format!(
            "Updated every {}",
            humantime::format_duration(interval)
        )),
    }
}

/// 日報作成（/diary new）結果のメッセージを組み立てる。
pub fn present_diary_create_outcome(outcome: &DiaryCreateOutcome) -> String {
    match outcome {
        DiaryCreateOutcome::Reopened { thread_id } => {
            format!("今日の日報を再開しました: <#{}>", thread_id)
        }
        DiaryCreateOutcome::ExistsButNotReopened { thread_id } => {
            format!(
                "今日の日報は既にありますが、再開はできません: <#{}>",
                thread_id
            )
        }
        DiaryCreateOutcome::Created {
            thread_id,
            page_url,
            reused_page,
        } => {
            if *reused_page {
                format!(
                    "既存の Notion ページを使用して日報を作成しました\nスレッド: <#{}>\nNotion: {}",
                    thread_id, page_url
                )
            } else {
                format!(
                    "日報を作成しました\nスレッド: <#{}>\nNotion: {}",
                    thread_id, page_url
                )
            }
        }
    }
}

/// クローズ & 新規作成の事前確認結果のメッセージを組み立てる。
///
/// `ReadyToCreate` の場合は実処理に進むため `None` を返す。
pub fn present_close_and_new_precheck(precheck: &CloseAndNewPrecheck) -> Option<String> {
    match precheck {
        CloseAndNewPrecheck::NotDiaryThread => None,
        CloseAndNewPrecheck::AlreadyLatest => {
            Some("このスレッドが今日の最新の日報です".to_string())
        }
        CloseAndNewPrecheck::LatestExists { thread_id } => {
            Some(format!("今日の最新の日報はこちらです: <#{}>", thread_id))
        }
        CloseAndNewPrecheck::ReadyToCreate => None,
    }
}

/// [`EmbedSpec`] を serenity の `CreateEmbed` に変換する。
pub fn render_embed(spec: &EmbedSpec) -> CreateEmbed {
    let mut embed = CreateEmbed::new().title(&spec.title).color(spec.color);
    for field in &spec.fields {
        embed = embed.field(&field.name, &field.value, field.inline);
    }
    if let Some(footer) = &spec.footer {
        embed = embed.footer(CreateEmbedFooter::new(footer));
    }
    embed
}

#[cfg(test)]
mod tests;
