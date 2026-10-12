//! Backoffice configuration. Read from `application.toml`, an optional
//! `application.<RUN_ENV>.toml` overlay (only when `RUN_ENV` is set), then
//! `.env` / process-environment overrides. No in-code defaults and no secrets
//! in committed TOML.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use config::{Config, ConfigError, File};
use serde::Deserialize;

#[derive(Debug, Deserialize, Clone, PartialEq, Eq)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Deserialize, Clone, PartialEq, Eq)]
pub struct UpstreamConfig {
    pub api_base: String,
}

#[derive(Debug, Deserialize, Clone, PartialEq, Eq)]
pub struct Settings {
    pub server: ServerConfig,
    pub upstream: UpstreamConfig,
}

impl Settings {
    fn load_from(dir: &Path, env_mode: Option<&str>) -> Result<Self, ConfigError> {
        let _ = dotenvy::from_path(dir.join(".env"));

        let mut builder =
            Config::builder().add_source(File::from(dir.join("application.toml")).required(false));
        if let Some(mode) = env_mode {
            builder = builder.add_source(
                File::from(dir.join(format!("application.{mode}.toml"))).required(false),
            );
        }

        for (key, target) in [
            ("SERVER_HOST", "server.host"),
            ("SERVER_PORT", "server.port"),
            ("UPSTREAM_API_BASE", "upstream.api_base"),
        ] {
            if let Ok(value) = std::env::var(key) {
                builder = builder.set_override(target, value)?;
            }
        }

        builder.build()?.try_deserialize()
    }

    pub fn load() -> Result<Self, ConfigError> {
        let dir = std::env::var("BACKOFFICE_CONFIG_DIR").map_or_else(
            |_| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            PathBuf::from,
        );
        let env_mode = std::env::var("RUN_ENV").ok().filter(|v| !v.is_empty());
        Self::load_from(&dir, env_mode.as_deref())
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.server.host.trim().is_empty() {
            return Err("server.host must not be empty".to_string());
        }
        if self.server.port == 0 {
            return Err("server.port must be between 1 and 65535".to_string());
        }
        url::Url::parse(&self.upstream.api_base)
            .map_err(|e| format!("upstream.api_base is not a valid URL: {e}"))?;
        Ok(())
    }
}

static SETTINGS: OnceLock<Settings> = OnceLock::new();

/// Process-wide settings, loaded and validated once. Missing/invalid config
/// panics so a misconfigured deployment fails fast on first use.
pub fn settings() -> &'static Settings {
    SETTINGS.get_or_init(|| {
        let s = Settings::load().expect("failed to load backoffice configuration");
        if let Err(e) = s.validate() {
            panic!("invalid backoffice configuration: {e}");
        }
        s
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn crate_dir() -> &'static Path {
        Path::new(env!("CARGO_MANIFEST_DIR"))
    }

    fn write(dir: &Path, name: &str, body: &str) {
        std::fs::write(dir.join(name), body).unwrap();
    }

    #[test]
    fn committed_toml_loads_and_env_override_wins() {
        let from_file = temp_env::with_vars_unset(["SERVER_PORT", "UPSTREAM_API_BASE"], || {
            Settings::load_from(crate_dir(), None)
        })
        .unwrap();
        assert_eq!(from_file.server.host, "127.0.0.1");
        assert_eq!(from_file.server.port, 8084);
        assert_eq!(from_file.upstream.api_base, "http://127.0.0.1:3000/v1");
        from_file.validate().unwrap();

        let overridden = temp_env::with_var("SERVER_PORT", Some("9999"), || {
            Settings::load_from(crate_dir(), None)
        })
        .unwrap();
        assert_eq!(overridden.server.port, 9999);
    }

    #[test]
    fn run_env_overlay_overrides_base() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            "application.toml",
            "[server]\nhost = \"127.0.0.1\"\nport = 8084\n[upstream]\napi_base = \"http://base:3000/v1\"\n",
        );
        write(
            dir.path(),
            "application.prod.toml",
            "[server]\nhost = \"0.0.0.0\"\n[upstream]\napi_base = \"http://prod:3000/v1\"\n",
        );

        let base = Settings::load_from(dir.path(), None).unwrap();
        assert_eq!(base.server.host, "127.0.0.1");
        assert_eq!(base.upstream.api_base, "http://base:3000/v1");

        let prod = Settings::load_from(dir.path(), Some("prod")).unwrap();
        assert_eq!(prod.server.host, "0.0.0.0");
        assert_eq!(prod.server.port, 8084);
        assert_eq!(prod.upstream.api_base, "http://prod:3000/v1");
    }

    #[test]
    fn missing_config_fails_without_env() {
        let dir = tempfile::tempdir().unwrap();
        let result =
            temp_env::with_vars_unset(["SERVER_HOST", "SERVER_PORT", "UPSTREAM_API_BASE"], || {
                Settings::load_from(dir.path(), None)
            });
        assert!(result.is_err(), "missing required values must fail");
    }

    #[test]
    fn invalid_upstream_url_fails_validation() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            "application.toml",
            "[server]\nhost = \"127.0.0.1\"\nport = 8084\n[upstream]\napi_base = \"not-a-url\"\n",
        );
        let s = Settings::load_from(dir.path(), None).unwrap();
        assert!(s.validate().is_err());
    }
}
