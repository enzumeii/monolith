use std::env;

use config::{Config, ConfigError, File};
use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DatabaseConfig {
    pub url: String,
    pub max_connections: u32,
    pub min_connections: u32,
}

#[derive(Debug, Deserialize, Clone)]
pub struct JwtSettings {
    pub secret: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Settings {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub jwt: JwtSettings,
}

impl Settings {
    pub fn new() -> Result<Self, ConfigError> {
        let _ = dotenvy::dotenv();
        let _ = dotenvy::from_filename(".env.local");
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

        builder.build()?.try_deserialize()
    }
}
