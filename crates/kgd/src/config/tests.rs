use std::{net::SocketAddr, path::PathBuf};

use ipnet::IpNet;

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
            day_start_hour: 8,
            ogp_enabled: true,
            ogp_timeout: Duration::from_secs(10),
        },
        location: None,
    };

    assert_eq!(config, expected);
}

/// 日報設定だけを差し替えられる最小の設定 TOML を作る。
fn minimal_config_toml(diary_extra: &str) -> String {
    format!(
        r#"
servers = []

[discord]
token = "token"
status_channel_id = 1

[status]

[diary]
database_url = "postgres://kgd:kgd@localhost:5432/kgd"
notion_token = "secret"
notion_database_id = "db"
forum_channel_id = 1
write_channel_id = 2
{diary_extra}
"#
    )
}

/// day_start_hour を書かなければ既定値 8 になることを確認する。
#[test]
fn day_start_hour_defaults_to_eight() {
    let config: Config = toml::from_str(&minimal_config_toml("")).expect("should parse");
    assert_eq!(config.diary.day_start_hour, 8);
}

/// 旧名の auto_close_hour で書かれた設定でも day_start_hour として読めることを確認する。
///
/// 設定ファイルを書き換えなくても動くようにするため。
#[test]
fn day_start_hour_accepts_legacy_auto_close_hour_name() {
    let config: Config =
        toml::from_str(&minimal_config_toml("auto_close_hour = 7")).expect("should parse");
    assert_eq!(config.diary.day_start_hour, 7);
}

/// day_start_hour が 0-23 の範囲内なら検証を通ることを確認する。
#[test]
fn validate_accepts_day_start_hour_in_range() {
    let config: Config =
        toml::from_str(&minimal_config_toml("day_start_hour = 23")).expect("should parse");
    assert!(config.validate().is_ok());
}

/// day_start_hour が 24 以上なら検証で弾かれることを確認する。
///
/// 範囲外のまま動かすと全時刻が前日扱いになり、日報日が進まなくなるため。
#[test]
fn validate_rejects_day_start_hour_out_of_range() {
    let config: Config =
        toml::from_str(&minimal_config_toml("day_start_hour = 24")).expect("should parse");
    let error = config.validate().expect_err("should be rejected");
    assert!(error.to_string().contains("day_start_hour"));
}

/// [location] を書かなければ None になり、既存の設定ファイルがそのまま読めることを確認する。
#[test]
fn location_is_optional() {
    let config: Config = toml::from_str(&minimal_config_toml("")).expect("should parse");
    assert_eq!(config.location, None);
}

/// [location] を書くと待ち受けアドレスと資格情報が読めることを確認する。
#[test]
fn location_parses_listen_and_credentials() {
    let toml_text = format!(
        "{}\n[location]\nlisten = \"0.0.0.0:8081\"\nusername = \"ekuinox\"\npassword = \"secret\"\n",
        minimal_config_toml("")
    );

    let config: Config = toml::from_str(&toml_text).expect("should parse");
    let location = config.location.expect("should be present");

    assert_eq!(
        location.listen,
        "0.0.0.0:8081".parse::<SocketAddr>().unwrap()
    );
    assert_eq!(location.username, "ekuinox");
    assert_eq!(location.password, "secret");
}

/// listen を省略すると既定の 0.0.0.0:8081 になることを確認する。
#[test]
fn location_listen_defaults_to_8081() {
    let toml_text = format!(
        "{}\n[location]\nusername = \"ekuinox\"\npassword = \"secret\"\n",
        minimal_config_toml("")
    );

    let config: Config = toml::from_str(&toml_text).expect("should parse");

    assert_eq!(
        config.location.unwrap().listen,
        "0.0.0.0:8081".parse::<SocketAddr>().unwrap()
    );
}

/// [location] にレポートの項目を書かなければ既定値になることを確認する。
#[test]
fn location_report_settings_have_defaults() {
    let toml_text = format!(
        "{}\n[location]\nusername = \"ekuinox\"\npassword = \"secret\"\n",
        minimal_config_toml("")
    );

    let config: Config = toml::from_str(&toml_text).expect("should parse");
    let location = config.location.expect("should be present");

    assert!(location.daily_report_enabled);
    assert_eq!(location.max_accuracy_m, 200);
    assert_eq!(location.image_width, 1024);
    assert_eq!(location.image_height, 1024);
    assert_eq!(
        location.tile_cache_dir,
        PathBuf::from("/var/cache/kgd/tiles")
    );
}

/// 地図画像の大きさに 0 を書くと検証で弾かれることを確認する。
///
/// 0 のままだと描画のたびに失敗し、定時ジョブが毎分エラーになるため。
#[test]
fn validate_rejects_zero_image_size() {
    let toml_text = format!(
        "{}\n[location]\nusername = \"ekuinox\"\npassword = \"secret\"\nimage_width = 0\n",
        minimal_config_toml("")
    );

    let config: Config = toml::from_str(&toml_text).expect("should parse");
    let error = config.validate().expect_err("should be rejected");
    assert!(error.to_string().contains("image_width"));
}

/// 地図画像の大きさに上限 (2048) を超える値を書くと検証で弾かれることを確認する。
///
/// 上限が無いと、巨大なピクマップの確保と OSM への大量のタイル要求が
/// 1 回の描画で発生してしまうため。
#[test]
fn validate_rejects_too_large_image_size() {
    let toml_text = format!(
        "{}\n[location]\nusername = \"ekuinox\"\npassword = \"secret\"\nimage_height = 4096\n",
        minimal_config_toml("")
    );

    let config: Config = toml::from_str(&toml_text).expect("should parse");
    let error = config.validate().expect_err("should be rejected");
    assert!(error.to_string().contains("image_width"));
}

/// [location] と、その後ろに続けるテキストから設定 TOML を作る。
fn location_config_toml(extra: &str) -> String {
    format!(
        "{}\n[location]\nusername = \"ekuinox\"\npassword = \"secret\"\n{extra}",
        minimal_config_toml("")
    )
}

/// [location.viewer] を書かなければビューアは無効 (None) になることを確認する。
///
/// kgd を更新しただけでビューアが動き出さないようにするため。
#[test]
fn location_viewer_is_disabled_by_default() {
    let config: Config = toml::from_str(&location_config_toml("")).expect("should parse");

    assert_eq!(config.location.expect("should be present").viewer, None);
}

/// [location.viewer] を書くと、送信元の許可リストが LAN のプライベート帯、名前の許可リストが空、
/// 点数の上限が 20000 になることを確認する。
///
/// 同じホストの cloudflared から届くリクエストを拒否するため、既定の許可リストに loopback を含めない。
#[test]
fn location_viewer_has_private_ranges_and_point_limit_by_default() {
    let config: Config =
        toml::from_str(&location_config_toml("[location.viewer]\n")).expect("should parse");

    let viewer = config
        .location
        .expect("should be present")
        .viewer
        .expect("viewer should be enabled");
    let expected: Vec<IpNet> = ["10.0.0.0/8", "172.16.0.0/12", "192.168.0.0/16", "fd00::/8"]
        .iter()
        .map(|net| net.parse().unwrap())
        .collect();
    assert_eq!(viewer.allowed_cidrs, expected);
    assert!(viewer.allowed_hosts.is_empty());
    assert_eq!(viewer.max_track_points, 20000);
    assert!(
        !viewer
            .allowed_cidrs
            .iter()
            .any(|net| net.contains(&"127.0.0.1".parse::<std::net::IpAddr>().unwrap()))
    );
}

/// 許可リストと点数の上限を書き換えられることを確認する。
#[test]
fn location_viewer_parses_custom_values() {
    let config: Config = toml::from_str(&location_config_toml(
        "[location.viewer]\nallowed_cidrs = [\"192.168.1.0/24\", \"127.0.0.1/32\"]\nmax_track_points = 500\n",
    ))
    .expect("should parse");

    let viewer = config.location.unwrap().viewer.unwrap();
    assert_eq!(
        viewer.allowed_cidrs,
        vec![
            "192.168.1.0/24".parse::<IpNet>().unwrap(),
            "127.0.0.1/32".parse::<IpNet>().unwrap(),
        ]
    );
    assert_eq!(viewer.max_track_points, 500);
}

/// 点数の上限に 0 を書くと検証で弾かれることを確認する。
#[test]
fn validate_rejects_zero_max_track_points() {
    let config: Config = toml::from_str(&location_config_toml(
        "[location.viewer]\nmax_track_points = 0\n",
    ))
    .expect("should parse");

    let error = config.validate().expect_err("should be rejected");
    assert!(error.to_string().contains("max_track_points"));
}

/// 許可リストを空にすると検証で弾かれることを確認する。
///
/// 空だとどこからも見られず、設定の誤りに気づきにくいため。
#[test]
fn validate_rejects_empty_allowed_cidrs() {
    let config: Config = toml::from_str(&location_config_toml(
        "[location.viewer]\nallowed_cidrs = []\n",
    ))
    .expect("should parse");

    let error = config.validate().expect_err("should be rejected");
    assert!(error.to_string().contains("allowed_cidrs"));
}

/// Host の許可リストに名前を書け、検証も通ることを確認する。
#[test]
fn location_viewer_parses_allowed_hosts() {
    let config: Config = toml::from_str(&location_config_toml(
        "[location.viewer]\nallowed_hosts = [\"aoi.local\", \"localhost\"]\n",
    ))
    .expect("should parse");

    assert!(config.validate().is_ok());
    let viewer = config.location.unwrap().viewer.unwrap();
    assert_eq!(viewer.allowed_hosts, vec!["aoi.local", "localhost"]);
}

/// Host の許可リストに、空の名前やポート、パスを含む名前を書くと検証で弾かれることを確認する。
///
/// 照合するのはポートを除いた名前だけなので、`aoi.local:8081` のような書き方は決して一致せず、
/// 設定の誤りに気づきにくいため。
#[test]
fn validate_rejects_malformed_allowed_hosts() {
    for host in ["", "aoi.local:8081", "http://aoi.local", "aoi.local/viewer"] {
        let config: Config = toml::from_str(&location_config_toml(&format!(
            "[location.viewer]\nallowed_hosts = [\"{host}\"]\n"
        )))
        .expect("should parse");

        let error = config.validate().expect_err("should be rejected");
        assert!(
            error.to_string().contains("allowed_hosts"),
            "{host}: {error}"
        );
    }
}
