#![cfg(feature = "server")]
//! Backoffice configuration: the upstream `services/api` base URL.
//!
//! No value is defaulted in code. Configuration is read from `application.toml`
//! plus an optional `application.<RUN_ENV>.toml` override (`RUN_ENV` defaults
//! to `local`), with environment variables taking the highest precedence for
//! deployment (e.g. Kubernetes). This mirrors the `services/api` pattern.
//!
//! Sources (lowest → highest precedence):
//!   `application.toml` → `application.<RUN_ENV>.toml`
//!   → `BACKOFFICE__UPSTREAM__API_BASE` (also from `.env` via `dotenvy`)
//!
//! The web server's own bind address is controlled by the `IP`/`PORT` env
//! vars (set by `dx serve` for dev, or manually for deployment), which is the
//! Dioxus-native mechanism — see the README.

use std::sync::OnceLock;

use config::{Config, ConfigError, Environment, File};
use serde::Deserialize;

/// Upstream dependencies of the backoffice server.
#[derive(Debug, Deserialize, Clone)]
pub struct UpstreamConfig {
    /// Base URL of the internal API service, including the `/v1` prefix.
    pub api_base: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Settings {
    pub upstream: UpstreamConfig,
}

impl Settings {
    pub fn load() -> Result<Self, ConfigError> {
        // Load `.env` into the environment first so it can supply overrides.
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

/// Process-wide settings, loaded exactly once. Panics on invalid/missing
/// configuration so a misconfigured deployment fails fast at startup.
pub fn settings() -> &'static Settings {
    SETTINGS.get_or_init(|| Settings::load().expect("failed to load backoffice configuration"))
}
