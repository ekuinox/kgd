use anyhow::{Context, Result};
use macaddr::MacAddr6;
use serde::{Deserialize, Serialize};
use serde_with::{DisplayFromStr, serde_as};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Config {
    pub discord: DiscordConfig,
    pub servers: Vec<ServerConfig>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DiscordConfig {
    pub token: String,
}

impl Default for DiscordConfig {
    fn default() -> Self {
        Self {
            token: "YOUR_DISCORD_BOT_TOKEN".to_string(),
        }
    }
}

#[serde_as]
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServerConfig {
    pub name: String,
    #[serde_as(as = "DisplayFromStr")]
    pub mac_address: MacAddr6,
    pub ip_address: String,
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

impl Config {
    pub fn find_server(&self, name: &str) -> Option<&ServerConfig> {
        self.servers.iter().find(|s| s.name == name)
    }
}

pub fn open_config<P: AsRef<Path>>(path: P) -> Result<Config> {
    let content = fs::read_to_string(path.as_ref()).context("Failed to read configuration file")?;
    let config: Config = toml::from_str(&content).context("Failed to parse configuration file")?;
    Ok(config)
}

pub fn write_default_config<P: AsRef<Path>>(path: P) -> Result<()> {
    let config = Config {
        servers: vec![ServerConfig::default()],
        ..Default::default()
    };
    let content = toml::to_string_pretty(&config).context("Failed to serialize configuration")?;
    fs::write(path.as_ref(), content).context("Failed to write configuration file")?;
    Ok(())
}
