use super::*;

/// 同梱の config.example.toml を Config にパースし、
/// 各フィールドが期待どおりの値 (Discord 設定・サーバー一覧・日報設定など) に
/// デシリアライズされることを確認する。
#[test]
fn parse_example_config() {
    let content = include_str!("../../../../config.example.toml");
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
            write_channel_id: 123456789012345678,
            sync_reaction: "✅".to_string(),
            timezone: chrono_tz::Asia::Tokyo,
            url_rules: vec![],
            default_convert_to: vec!["link".to_string()],
            auto_close_enabled: false,
            auto_close_hour: 8,
            ogp_enabled: true,
            ogp_timeout: Duration::from_secs(10),
        },
    };

    assert_eq!(config, expected);
}
