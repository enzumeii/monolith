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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn new_applies_database_url_and_jwt_secret_overrides() {
        let settings = temp_env::with_vars(
            [
                (
                    "DATABASE_URL",
                    Some("postgres://override:override@localhost/override"),
                ),
                ("JWT_SECRET", Some("overridden-secret")),
            ],
            Settings::new,
        )
        .unwrap();

        assert_eq!(
            settings.database.url,
            "postgres://override:override@localhost/override"
        );
        assert_eq!(settings.jwt.secret, "overridden-secret");
    }

    #[test]
    fn new_reads_non_secret_fields_from_file_when_only_secrets_come_from_env() {
        // Secrets (database.url, jwt.secret) are env-only by design and never
        // live in application.toml. Supply them via env, then assert the
        // non-secret fields still come from the file.
        let settings = temp_env::with_vars(
            [
                ("DATABASE_URL", Some("postgres://env:env@localhost/env")),
                ("JWT_SECRET", Some("env-secret")),
            ],
            Settings::new,
        )
        .unwrap();

        assert_eq!(settings.server.port, 3000);
        assert_eq!(settings.database.max_connections, 1);
        assert_eq!(settings.database.url, "postgres://env:env@localhost/env");
        assert_eq!(settings.jwt.secret, "env-secret");
    }
}
