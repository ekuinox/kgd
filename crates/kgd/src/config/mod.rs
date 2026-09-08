use std::{fs, path::Path, time::Duration};

use anyhow::{Context as _, Result, ensure};
use chrono_tz::Tz;
use macaddr::MacAddr6;
use serde::{Deserialize, Serialize};
use serde_with::{DisplayFromStr, serde_as};

use kgd_domain::{ServerTarget, UrlRuleConfig};
use kgd_infrastructure::NotionTagConfig;

mod defaults;

use defaults::*;

/// 指定されたパスから設定ファイルを読み込む。
pub fn open_config(path: impl AsRef<Path>) -> Result<Config> {
    let content = fs::read_to_string(path.as_ref()).context("Failed to read configuration file")?;
    let config: Config = toml::from_str(&content).context("Failed to parse configuration file")?;
    config.validate()?;
    Ok(config)
}

/// デフォルト設定を指定されたパスに書き出す。
pub fn write_default_config(path: impl AsRef<Path>) -> Result<()> {
    let content = include_str!("../../../../config.example.toml");
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
    /// 設定値の整合性を検証する。
    ///
    /// 型では表せない値域の制約を、起動時にまとめて弾くために用意している。
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.diary.day_start_hour < 24,
            "diary.day_start_hour must be in 0-23, but got {}",
            self.diary.day_start_hour
        );
        Ok(())
    }

    /// 監視・操作対象のサーバー一覧をドメイン型に変換して返す。
    pub fn server_targets(&self) -> Vec<ServerTarget> {
        self.servers
            .iter()
            .map(|server| ServerTarget {
                name: server.name.clone(),
                mac_address: server.mac_address,
                ip_address: server.ip_address.clone(),
                description: server.description.clone(),
            })
            .collect()
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
    /// 日報の書き込み用チャンネル ID
    /// このチャンネルへの投稿は最新の日報スレッドへ転記され、Notion ページに同期される
    pub write_channel_id: u64,
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
    /// 自動クローズ機能を有効にするか（デフォルト: false）
    #[serde(default)]
    pub auto_close_enabled: bool,
    /// 日報の一日が始まる時（0-23）（デフォルト: 8）
    ///
    /// 日付境界と自動クローズ通知の両方に使う。
    /// この時刻より前にクローズ & 新規作成ボタンを押すと、次の日報日を早出しできる。
    /// 旧名 `auto_close_hour` で書かれた設定ファイルもそのまま読める。
    #[serde(default = "default_day_start_hour", alias = "auto_close_hour")]
    pub day_start_hour: u32,
    /// OGP メタデータ取得を有効にするか（デフォルト: true）
    #[serde(default = "default_ogp_enabled")]
    pub ogp_enabled: bool,
    /// OGP メタデータ取得のタイムアウト（デフォルト: 10秒）
    #[serde(default = "default_ogp_timeout", with = "humantime_serde")]
    pub ogp_timeout: Duration,
}

#[cfg(test)]
mod tests;
