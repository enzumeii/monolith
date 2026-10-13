use std::env;

use config::{Config, ConfigError, File};
use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

#[derive(Deserialize, Clone)]
pub struct DatabaseConfig {
    pub url: String,
    pub max_connections: u32,
    pub min_connections: u32,
}

#[derive(Deserialize, Clone)]
pub struct JwtSettings {
    pub secret: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct BrowserConfig {
    pub public_origin: String,
    pub insecure_local: bool,
}
#[derive(Debug, Deserialize, Clone)]
pub struct Settings {
    pub browser: BrowserConfig,
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub jwt: JwtSettings,
}

impl JwtSettings {
    pub fn key(&self) -> Result<Vec<u8>, ConfigError> {
        let key = hex::decode(&self.secret).map_err(|_| {
            ConfigError::Message("JWT_SECRET must be hex-encoded random bytes".into())
        })?;
        if key.len() < 32 {
            return Err(ConfigError::Message(
                "JWT_SECRET must contain at least 32 random bytes".into(),
            ));
        }
        Ok(key)
    }
}
impl Settings {
    pub fn validate(&self) -> Result<(), ConfigError> {
        self.jwt.key()?;
        let origin: axum::http::Uri = self
            .browser
            .public_origin
            .parse()
            .map_err(|_| ConfigError::Message("Invalid browser origin".into()))?;
        let authority = origin
            .authority()
            .ok_or_else(|| ConfigError::Message("Browser origin needs a host".into()))?;
        let scheme = origin.scheme_str().unwrap_or_default();
        let loopback = matches!(
            authority.host(),
            "127.0.0.1" | "localhost" | "[::1]" | "::1"
        );
        if authority.as_str().contains('@')
            || format!("{scheme}://{authority}") != self.browser.public_origin
            || (self.browser.insecure_local && (scheme != "http" || !loopback))
            || (!self.browser.insecure_local && scheme != "https")
        {
            return Err(ConfigError::Message(
                "Browser cookies require HTTPS or explicit loopback HTTP".into(),
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

        if let Ok(db_url) = env::var("DATABASE_URL") {
            builder = builder.set_override("database.url", db_url)?;
        }
        if let Ok(jwt_secret) = env::var("JWT_SECRET") {
            builder = builder.set_override("jwt.secret", jwt_secret)?;
        }

        let settings: Self = builder.build()?.try_deserialize()?;
        settings.validate()?;
        Ok(settings)
    }
}

impl std::fmt::Debug for DatabaseConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DatabaseConfig")
            .field("url", &"[REDACTED]")
            .field("max_connections", &self.max_connections)
            .field("min_connections", &self.min_connections)
            .finish()
    }
}

impl std::fmt::Debug for JwtSettings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("JwtSettings { secret: [REDACTED] }")
    }
}

#[cfg(test)]
#[path = "tests/config.rs"]
mod tests;
