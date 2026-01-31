use std::{fs, path::Path, time::Duration};

use anyhow::{Context as _, Result};
use chrono_tz::Tz;
use macaddr::MacAddr6;
use serde::{Deserialize, Serialize};
use serde_with::{DisplayFromStr, serde_as};

/// 指定されたパスから設定ファイルを読み込む。
pub fn open_config(path: impl AsRef<Path>) -> Result<Config> {
    let content = fs::read_to_string(path.as_ref()).context("Failed to read configuration file")?;
    let config: Config = toml::from_str(&content).context("Failed to parse configuration file")?;
    Ok(config)
}

/// デフォルト設定を指定されたパスに書き出す。
pub fn write_default_config(path: impl AsRef<Path>) -> Result<()> {
    let content = include_str!("../../../config.example.toml");
    fs::write(path.as_ref(), content).context("Failed to write configuration file")?;
    Ok(())
}

/// アプリケーション全体の設定を保持する構造体。
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct Config {
    /// Discord Bot の設定
    pub discord: DiscordConfig,
    /// 監視対象のサーバー一覧
    pub servers: Vec<ServerConfig>,
    /// ステータスモニターの設定
    pub status: StatusConfig,
    /// 日報機能の設定
    pub diary: DiaryConfig,
}

impl Config {
    /// 指定された名前のサーバー設定を検索する。
    pub fn find_server(&self, name: &str) -> Option<&ServerConfig> {
        self.servers.iter().find(|s| s.name == name)
    }
}

/// Discord Bot の設定。
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct DiscordConfig {
    /// Discord Bot のトークン
    pub token: String,
    /// コマンド実行を許可する管理者のユーザーID一覧
    #[serde(default)]
    pub admins: Vec<u64>,
    /// サーバーステータスを通知するDiscordチャンネルのID
    pub status_channel_id: u64,
}

impl Default for DiscordConfig {
    fn default() -> Self {
        Self {
            token: "YOUR_DISCORD_BOT_TOKEN".to_string(),
            admins: vec![],
            status_channel_id: 0,
        }
    }
}

/// 監視対象サーバーの設定。
#[serde_as]
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct ServerConfig {
    /// サーバー名（識別用）
    pub name: String,
    /// Wake-on-LAN 送信先の MAC アドレス
    #[serde_as(as = "DisplayFromStr")]
    pub mac_address: MacAddr6,
    /// ping 送信先の IP アドレス
    pub ip_address: String,
    /// サーバーの説明文
    #[serde(default)]
    pub description: String,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            name: "example-server".to_string(),
            mac_address: MacAddr6::new(0x00, 0x11, 0x22, 0x33, 0x44, 0x55),
            ip_address: "192.168.1.100".to_string(),
            description: "Example server".to_string(),
        }
    }
}

/// ステータスモニターの設定。
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct StatusConfig {
    /// ステータスチェックの実行間隔（デフォルト: 5分）
    #[serde(default = "default_interval", with = "humantime_serde")]
    pub interval: Duration,
}

impl Default for StatusConfig {
    fn default() -> Self {
        Self {
            interval: default_interval(),
        }
    }
}

fn default_interval() -> Duration {
    Duration::from_secs(300) // 5 minutes
}

/// 日報機能の設定。
#[serde_as]
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct DiaryConfig {
    /// PostgreSQL データベース URL
    pub database_url: String,
    /// Notion API トークン
    pub notion_token: String,
    /// 日報を保存する Notion データベース ID
    pub notion_database_id: String,
    /// Notion データベースのタイトルプロパティ名
    #[serde(default = "default_title_property")]
    pub notion_title_property: String,
    /// ページ作成時に設定するタグ（セレクトプロパティ）
    #[serde(default)]
    pub notion_tags: Vec<NotionTagConfig>,
    /// 日報スレッドを作成する Discord フォーラムチャンネル ID
    pub forum_channel_id: u64,
    /// 同期成功時にメッセージに付けるリアクション絵文字
    #[serde(default = "default_sync_reaction")]
    pub sync_reaction: String,
    /// 日報の日付計算に使用するタイムゾーン（デフォルト: Asia/Tokyo）
    #[serde(default = "default_timezone")]
    #[serde_as(as = "DisplayFromStr")]
    pub timezone: Tz,
    /// URL 変換ルール
    /// パターンにマッチした URL を指定したブロックタイプに変換する
    #[serde(default)]
    pub url_rules: Vec<UrlRuleConfig>,
    /// どのルールにもマッチしなかった URL に適用するデフォルトの変換（デフォルト: ["link"]）
    #[serde(default = "default_convert_to")]
    pub default_convert_to: Vec<String>,
}

/// URL 変換ルール設定。
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct UrlRuleConfig {
    /// マッチする URL パターン
    pub pattern: PatternConfig,
    /// 生成するブロックタイプのリスト（link, bookmark, embed）
    pub convert_to: Vec<String>,
}

/// URL マッチパターンの種類。
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PatternConfig {
    /// glob 形式のパターン
    Glob(String),
    /// 正規表現パターン
    Regex(String),
    /// 前方一致パターン
    Prefix(String),
}

/// Notion タグ設定。
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct NotionTagConfig {
    /// プロパティ名
    pub property: String,
    /// 設定する値
    pub value: String,
    /// マルチセレクトかどうか（デフォルト: false）
    #[serde(default)]
    pub multi_select: bool,
}

fn default_title_property() -> String {
    "Name".to_string()
}

fn default_sync_reaction() -> String {
    "✅".to_string()
}

fn default_timezone() -> Tz {
    chrono_tz::Asia::Tokyo
}

fn default_convert_to() -> Vec<String> {
    vec!["link".to_string()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_example_config() {
        let content = include_str!("../../../config.example.toml");
        let config: Config = toml::from_str(content).expect("Failed to parse config.example.toml");

        let expected = Config {
            discord: DiscordConfig {
                token: "YOUR_DISCORD_BOT_TOKEN".to_string(),
                admins: vec![],
                status_channel_id: 123456789012345678,
            },
            servers: vec![
                ServerConfig {
                    name: "Main Server".to_string(),
                    mac_address: MacAddr6::new(0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF),
                    ip_address: "192.168.1.100".to_string(),
                    description: "メインサーバー".to_string(),
                },
                ServerConfig {
                    name: "Storage Server".to_string(),
                    mac_address: MacAddr6::new(0x11, 0x22, 0x33, 0x44, 0x55, 0x66),
                    ip_address: "192.168.1.101".to_string(),
                    description: "ストレージサーバー".to_string(),
                },
            ],
            status: StatusConfig::default(),
            diary: DiaryConfig {
                database_url: "postgres://kgd:kgd@localhost:5432/kgd".to_string(),
                notion_token: "secret_xxxxxxxxxxxxxxxxxxxxx".to_string(),
                notion_database_id: "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx".to_string(),
                notion_title_property: "Name".to_string(),
                notion_tags: vec![],
                forum_channel_id: 123456789012345678,
                sync_reaction: "✅".to_string(),
                timezone: chrono_tz::Asia::Tokyo,
                url_rules: vec![],
                default_convert_to: vec!["link".to_string()],
            },
        };

        assert_eq!(config, expected);
    }
}
