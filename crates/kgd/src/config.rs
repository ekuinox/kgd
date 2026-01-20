use anyhow::{Context, Result};
use macaddr::MacAddr6;
use serde::Deserialize;
use serde_with::{DisplayFromStr, serde_as};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub discord: DiscordConfig,
    pub servers: Vec<ServerConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DiscordConfig {
    pub token: String,
}

#[serde_as]
#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    pub name: String,
    #[serde_as(as = "DisplayFromStr")]
    pub mac_address: MacAddr6,
    pub ip_address: String,
    pub description: Option<String>,
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

impl ServerConfig {
    /// Get the description or a default message
    pub fn description(&self) -> &str {
        self.description.as_deref().unwrap_or("No description")
    }
}
