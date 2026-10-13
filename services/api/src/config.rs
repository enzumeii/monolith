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
pub struct Settings {
    #[serde(default = "default_password_concurrency")]
    pub password_concurrency: usize,
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub jwt: JwtSettings,
}

impl std::fmt::Debug for JwtSettings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("JwtSettings { secret: [REDACTED] }")
    }
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
fn default_password_concurrency() -> usize {
    2
}

impl Settings {
    pub fn validate(&self) -> Result<(), ConfigError> {
        self.jwt.key()?;
        if self.password_concurrency == 0 {
            return Err(ConfigError::Message(
                "password_concurrency must be positive".into(),
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

        if let Ok(host) = env::var("API_HOST") {
            builder = builder.set_override("server.host", host)?;
        }
        if let Ok(port) = env::var("API_PORT") {
            builder = builder.set_override(
                "server.port",
                port.parse::<u16>()
                    .map_err(|_| ConfigError::Message("invalid API_PORT".into()))?,
            )?;
        }
        if let Ok(db_url) = env::var("DATABASE_URL") {
            builder = builder.set_override("database.url", db_url)?;
        }
        if let Ok(jwt_secret) = env::var("JWT_SECRET") {
            builder = builder.set_override("jwt.secret", jwt_secret)?;
        }

        if let Ok(value) = env::var("PASSWORD_CONCURRENCY") {
            builder = builder.set_override(
                "password_concurrency",
                value
                    .parse::<u32>()
                    .map_err(|_| ConfigError::Message("invalid PASSWORD_CONCURRENCY".into()))?,
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
    fn new_applies_database_url_and_jwt_secret_overrides() {
        let settings = temp_env::with_vars(
            [
                (
                    "DATABASE_URL",
                    Some("postgres://override:override@localhost/override"),
                ),
                (
                    "JWT_SECRET",
                    Some("abababababababababababababababababababababababababababababababab"),
                ),
            ],
            Settings::new,
        )
        .unwrap();

        assert_eq!(
            settings.database.url,
            "postgres://override:override@localhost/override"
        );
        assert_eq!(
            settings.jwt.secret,
            "abababababababababababababababababababababababababababababababab"
        );
    }

    #[test]
    fn new_reads_non_secret_fields_from_file_when_only_secrets_come_from_env() {
        // Secrets (database.url, jwt.secret) are env-only by design and never
        // live in application.toml. Supply them via env, then assert the
        // non-secret fields still come from the file.
        let settings = temp_env::with_vars(
            [
                ("DATABASE_URL", Some("postgres://env:env@localhost/env")),
                (
                    "JWT_SECRET",
                    Some("abababababababababababababababababababababababababababababababab"),
                ),
            ],
            Settings::new,
        )
        .unwrap();

        assert_eq!(settings.server.port, 3000);
        assert_eq!(settings.database.max_connections, 1);
        assert_eq!(settings.database.url, "postgres://env:env@localhost/env");
        assert_eq!(
            settings.jwt.secret,
            "abababababababababababababababababababababababababababababababab"
        );
    }
}

#[cfg(test)]
mod invalid_config_tests {
    use super::*;
    #[test]
    fn rejects_invalid_password_admission_limits() {
        for limit in ["0", "not-a-number"] {
            let result = temp_env::with_vars(
                [
                    ("DATABASE_URL", Some("postgres://unused")),
                    (
                        "JWT_SECRET",
                        Some("abababababababababababababababababababababababababababababababab"),
                    ),
                    ("PASSWORD_CONCURRENCY", Some(limit)),
                ],
                Settings::new,
            );
            assert!(result.is_err());
        }
    }
    #[test]
    fn database_debug_hides_credentials() {
        let config = DatabaseConfig {
            url: "postgres://private:secret@localhost/db".into(),
            min_connections: 0,
            max_connections: 1,
        };
        assert!(!format!("{config:?}").contains("secret"));
    }
    #[test]
    fn startup_rejects_placeholder_and_undersized_signing_keys() {
        for key in ["", "CHANGE_ME", "not-hex", &"ab".repeat(31)] {
            let result = temp_env::with_vars(
                [
                    ("DATABASE_URL", Some("postgres://unused")),
                    ("JWT_SECRET", Some(key)),
                ],
                Settings::new,
            );
            assert!(result.is_err(), "unsafe signing key was accepted");
        }
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
