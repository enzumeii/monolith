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
pub struct SessionConfig {
    pub public_origin: String,
    pub insecure_local: bool,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Settings {
    pub server: ServerConfig,
    pub upstream: UpstreamConfig,
    pub session: SessionConfig,
}

impl Settings {
    pub fn validate(&self) -> Result<(), ConfigError> {
        let url = reqwest::Url::parse(&self.session.public_origin)
            .map_err(|_| ConfigError::Message("invalid public_origin".into()))?;
        let loopback = matches!(
            url.host_str(),
            Some("127.0.0.1" | "localhost" | "[::1]" | "::1")
        );
        if url.origin().ascii_serialization() != self.session.public_origin
            || (!self.session.insecure_local && url.scheme() != "https")
            || (self.session.insecure_local && (!loopback || url.scheme() != "http"))
        {
            return Err(ConfigError::Message(
                "cookie origin must be HTTPS, or explicit loopback HTTP for local development"
                    .into(),
            ));
        }
        Ok(())
    }

    pub fn new() -> Result<Self, ConfigError> {
        let _ = dotenvy::dotenv();
        let env_mode = env::var("RUN_ENV").unwrap_or_else(|_| "local".into());
        let mut builder = Config::builder()
            .add_source(File::with_name("application"))
            .add_source(File::with_name(&format!("application.{env_mode}")).required(false));
        for (name, key) in [
            ("UPSTREAM_API_BASE", "upstream.api_base"),
            ("BACKOFFICE_HOST", "server.host"),
            ("PUBLIC_ORIGIN", "session.public_origin"),
        ] {
            if let Ok(value) = env::var(name) {
                builder = builder.set_override(key, value)?;
            }
        }
        if let Ok(value) = env::var("BACKOFFICE_PORT") {
            builder = builder.set_override(
                "server.port",
                value
                    .parse::<u16>()
                    .map_err(|_| ConfigError::Message("invalid BACKOFFICE_PORT".into()))?,
            )?;
        }
        let settings: Self = builder.build()?.try_deserialize()?;
        settings.validate()?;
        Ok(settings)
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
