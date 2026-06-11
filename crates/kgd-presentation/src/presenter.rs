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
mod tests {
    use macaddr::MacAddr6;

    use super::*;

    #[test]
    fn present_wake_outcome_formats_sent_message() {
        let outcome = WakeOutcome::Sent {
            name: "nas".to_string(),
            mac_address: MacAddr6::new(0, 0x11, 0x22, 0x33, 0x44, 0x55),
        };
        assert_eq!(
            present_wake_outcome(&outcome),
            Some("Sent WOL packet to nas (00:11:22:33:44:55)".to_string())
        );
    }

    #[test]
    fn present_wake_outcome_returns_none_for_not_found() {
        assert_eq!(present_wake_outcome(&WakeOutcome::ServerNotFound), None);
    }

    #[test]
    fn present_servers_builds_fields_and_footer() {
        let servers = vec![ServerTarget {
            name: "nas".to_string(),
            mac_address: MacAddr6::new(0, 0x11, 0x22, 0x33, 0x44, 0x55),
            ip_address: "192.168.1.10".to_string(),
            description: "storage".to_string(),
        }];
        let spec = present_servers(&servers);

        assert_eq!(spec.title, "Configured Servers");
        assert_eq!(spec.fields.len(), 1);
        assert_eq!(spec.fields[0].name, "nas");
        assert!(spec.fields[0].value.contains("192.168.1.10"));
        assert!(spec.fields[0].value.contains("storage"));
        assert_eq!(spec.footer, Some("Total: 1 server(s)".to_string()));
    }

    #[test]
    fn present_server_status_marks_online_and_offline() {
        let statuses = vec![
            ServerStatus {
                name: "a".to_string(),
                online: true,
            },
            ServerStatus {
                name: "b".to_string(),
                online: false,
            },
        ];
        let spec = present_server_status(&statuses, Duration::from_secs(300));

        assert_eq!(spec.fields[0].value, "Online");
        assert_eq!(spec.fields[1].value, "Offline");
        assert_eq!(spec.footer, Some("Updated every 5m".to_string()));
    }

    #[test]
    fn present_diary_create_outcome_formats_each_variant() {
        assert_eq!(
            present_diary_create_outcome(&DiaryCreateOutcome::Reopened { thread_id: 1 }),
            "今日の日報を再開しました: <#1>"
        );
        assert_eq!(
            present_diary_create_outcome(&DiaryCreateOutcome::ExistsButNotReopened {
                thread_id: 2
            }),
            "今日の日報は既にありますが、再開はできません: <#2>"
        );
        let created = present_diary_create_outcome(&DiaryCreateOutcome::Created {
            thread_id: 3,
            page_url: "https://notion.example/x".to_string(),
            reused_page: false,
        });
        assert!(created.contains("日報を作成しました"));
        assert!(created.contains("<#3>"));
        assert!(created.contains("https://notion.example/x"));
    }

    #[test]
    fn present_close_and_new_precheck_formats_messages() {
        assert_eq!(
            present_close_and_new_precheck(&CloseAndNewPrecheck::AlreadyLatest),
            Some("このスレッドが今日の最新の日報です".to_string())
        );
        assert_eq!(
            present_close_and_new_precheck(&CloseAndNewPrecheck::LatestExists { thread_id: 5 }),
            Some("今日の最新の日報はこちらです: <#5>".to_string())
        );
        assert_eq!(
            present_close_and_new_precheck(&CloseAndNewPrecheck::ReadyToCreate),
            None
        );
    }
}
