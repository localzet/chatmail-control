use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    pub server: ServerConfig,
    pub auth: AuthConfig,
    pub bans: BansConfig,
    pub settings: SettingsConfig,
    pub invites: InvitesConfig,
    pub health: HealthConfig,
}

impl Config {
    pub fn load(path: impl AsRef<Path>) -> AppResult<Self> {
        let path = path.as_ref();
        let raw = fs::read_to_string(path)
            .map_err(|e| AppError::Config(format!("failed to read {}: {e}", path.display())))?;
        toml::from_str(&raw)
            .map_err(|e| AppError::Config(format!("failed to parse {}: {e}", path.display())))
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServerConfig {
    pub bind: String,
    pub public_url: String,
    #[serde(default = "default_true")]
    pub secure_cookies: bool,
    pub database_url: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AuthConfig {
    pub session_secret: String,
    pub session_ttl_hours: i64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BansConfig {
    pub address_file: String,
    pub domain_file: String,
    pub ip_file: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SettingsConfig {
    pub generated_policy_file: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct InvitesConfig {
    pub export_file: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HealthConfig {
    pub domain: String,
    pub dkim_selector: String,
    #[serde(default)]
    pub services: Vec<String>,
    #[serde(default)]
    pub ports: Vec<u16>,
}

fn default_true() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::Config;

    #[test]
    fn parses_config() {
        let raw = include_str!("../config.example.toml");
        let cfg: Config = toml::from_str(raw).expect("config should parse");
        assert_eq!(cfg.server.bind, "127.0.0.1:8088");
        assert_eq!(cfg.health.domain, "example.com");
        assert_eq!(cfg.health.ports, vec![25, 587, 993]);
    }
}
