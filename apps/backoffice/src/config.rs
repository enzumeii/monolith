#![cfg(feature = "server")]
//! Backoffice configuration: web/BFF bind address and upstream API base URL.
//! Read from `application.toml`, plus an optional `application.<RUN_ENV>.toml`
//! overlay (`RUN_ENV` defaults to `local`). No secrets are needed.

use std::env;

use config::{Config, ConfigError, File};
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
    pub fn new() -> Result<Self, ConfigError> {
        let _ = dotenvy::dotenv();
        let env_mode = env::var("RUN_ENV").unwrap_or_else(|_| "local".into());
        Config::builder()
            .add_source(File::with_name("application"))
            .add_source(File::with_name(&format!("application.{env_mode}")).required(false))
            .build()?
            .try_deserialize()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn reads_committed_file() {
        let s = Settings::new().unwrap();
        assert_eq!(s.server.host, "127.0.0.1");
        assert_eq!(s.server.port, 8084);
        assert_eq!(s.upstream.api_base, "http://127.0.0.1:3000/v1");
    }
}
