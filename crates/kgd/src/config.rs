use std::{fs, path::Path, time::Duration};

use anyhow::{Context as _, Result};
use macaddr::MacAddr6;
use serde::{Deserialize, Serialize};
use serde_with::{DisplayFromStr, serde_as};

/// アプリケーション全体の設定を保持する構造体。
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
pub struct Config {
    /// Discord Bot の設定
    pub discord: DiscordConfig,
    /// 監視対象のサーバー一覧
    pub servers: Vec<ServerConfig>,
    /// ステータスモニターの設定
    pub status: StatusConfig,
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

impl Config {
    /// 指定された名前のサーバー設定を検索する。
    pub fn find_server(&self, name: &str) -> Option<&ServerConfig> {
        self.servers.iter().find(|s| s.name == name)
    }
}

/// 指定されたパスから設定ファイルを読み込む。
pub fn open_config<P: AsRef<Path>>(path: P) -> Result<Config> {
    let content = fs::read_to_string(path.as_ref()).context("Failed to read configuration file")?;
    let config: Config = toml::from_str(&content).context("Failed to parse configuration file")?;
    Ok(config)
}

/// デフォルト設定を指定されたパスに書き出す。
pub fn write_default_config<P: AsRef<Path>>(path: P) -> Result<()> {
    let config = Config {
        servers: vec![ServerConfig::default()],
        ..Default::default()
    };
    let content = toml::to_string_pretty(&config).context("Failed to serialize configuration")?;
    fs::write(path.as_ref(), content).context("Failed to write configuration file")?;
    Ok(())
}

fn default_interval() -> Duration {
    Duration::from_secs(300) // 5 minutes
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
        };

        assert_eq!(config, expected);
    }
}
