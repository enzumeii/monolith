#![cfg(feature = "server")]

use std::sync::OnceLock;

use config::{Config, ConfigError, Environment, File};
use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Deserialize, Clone)]
pub struct UpstreamConfig {
    pub api_base: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Settings {
    pub server: ServerConfig,
    pub upstream: UpstreamConfig,
}

impl Settings {
    pub fn load() -> Result<Self, ConfigError> {
        let _ = dotenvy::dotenv();
        let env_mode = std::env::var("RUN_ENV").unwrap_or_else(|_| "local".into());

        Config::builder()
            .add_source(File::with_name("application").required(false))
            .add_source(File::with_name(&format!("application.{env_mode}")).required(false))
            .add_source(Environment::with_prefix("BACKOFFICE").separator("__"))
            .build()?
            .try_deserialize()
    }
}

static SETTINGS: OnceLock<Settings> = OnceLock::new();

pub fn settings() -> &'static Settings {
    SETTINGS.get_or_init(|| Settings::load().expect("failed to load backoffice configuration"))
}
