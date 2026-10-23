use std::env;

use config::{Config, ConfigBuilder, ConfigError, File, builder::DefaultState};
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
    #[serde(default)]
    pub additional_origins: Vec<String>,
    pub public_origin: String,
    pub insecure_local: bool,
}
#[derive(Debug, Deserialize, Clone)]
pub struct ClientConfig {
    pub paypal: Option<PaypalConfig>,
    pub shopify: Option<ShopifyConfig>,
    pub resend: Option<ResendConfig>,
    pub stripe: Option<StripeConfig>,
    pub s3: Option<S3Config>,
}

#[derive(Deserialize, Clone)]
pub struct PaypalConfig {
    pub base_url: String,
    pub timeout_secs: u64,
    pub client_id: Option<String>,
    pub client_secret: Option<String>,
}

impl std::fmt::Debug for PaypalConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PaypalConfig { credentials: [REDACTED], .. }")
    }
}

#[derive(Deserialize, Clone)]
pub struct ShopifyConfig {
    pub admin_api_url: String,
    pub timeout_secs: u64,
    pub access_token: Option<String>,
}
impl std::fmt::Debug for ShopifyConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ShopifyConfig { access_token: [REDACTED], .. }")
    }
}

#[derive(Deserialize, Clone)]
pub struct ResendConfig {
    pub timeout_secs: Option<u64>,
    pub api_key: Option<String>,
}

impl std::fmt::Debug for ResendConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ResendConfig { api_key: [REDACTED], .. }")
    }
}

#[derive(Deserialize, Clone)]
pub struct S3Config {
    pub endpoint: String,
    pub region: String,
    pub bucket: String,
    pub force_path_style: bool,
    pub timeout_secs: u64,
    pub signed_url_ttl_secs: u32,
    pub access_key_id: Option<String>,
    pub secret_access_key: Option<String>,
}

impl std::fmt::Debug for S3Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("S3Config { credentials: [REDACTED], .. }")
    }
}

#[derive(Deserialize, Clone)]
pub struct StripeConfig {
    pub base_url: String,
    pub timeout_secs: u64,
    pub secret_key: Option<String>,
}
impl std::fmt::Debug for StripeConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("StripeConfig { secret_key: [REDACTED], .. }")
    }
}

#[derive(Deserialize, Clone)]
pub struct Settings {
    pub shipping_sender: Option<ShippingSender>,
    pub client: Option<ClientConfig>,
    pub browser: BrowserConfig,
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub jwt: JwtSettings,
    pub scheduler: SchedulerConfig,
    pub cron_secret: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct SchedulerConfig {
    pub implementation: CronImplementation,
    pub poll_interval_secs: u64,
    pub retention_interval_secs: u64,
    pub invocation_budget_secs: u64,
    pub retention_reserve_secs: u64,
    pub enabled: bool,
    pub concurrency: usize,
    pub max_attempts: u32,
    pub retry_backoff_secs: u64,
    pub lease_secs: u64,
    pub max_payload_bytes: usize,
    pub max_email_bytes: usize,
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CronImplementation {
    Vps,
    Vercel,
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
        if !(1..=32).contains(&self.scheduler.concurrency)
            || !(1..=100).contains(&self.scheduler.max_attempts)
            || !(1..=86400).contains(&self.scheduler.retry_backoff_secs)
            || !(1..=86400).contains(&self.scheduler.lease_secs)
            || !(1..=86400).contains(&self.scheduler.poll_interval_secs)
            || !(1..=86400).contains(&self.scheduler.retention_interval_secs)
            || !(6..=240).contains(&self.scheduler.invocation_budget_secs)
            || self.scheduler.retention_reserve_secs == 0
            || self.scheduler.retention_reserve_secs >= self.scheduler.invocation_budget_secs
            || self.scheduler.max_payload_bytes == 0
            || self.scheduler.max_email_bytes == 0
        {
            return Err(ConfigError::Message(
                "Scheduler intervals, limits, and concurrency must be positive and bounded".into(),
            ));
        }
        if let Some(sender) = &self.shipping_sender
            && [
                &sender.name,
                &sender.phone,
                &sender.email,
                &sender.address,
                &sender.sub_district,
                &sender.district,
                &sender.province,
                &sender.postcode,
                &sender.country_code,
            ]
            .iter()
            .any(|value| value.trim().is_empty())
        {
            return Err(ConfigError::Message(
                "Shipping sender profile is incomplete".into(),
            ));
        }
        validate_browser_origin(&self.browser.public_origin, self.browser.insecure_local)?;
        for origin in &self.browser.additional_origins {
            validate_browser_origin(origin, false)?;
        }
        Ok(())
    }

    pub fn new() -> Result<Self, ConfigError> {
        let explicit_qa = env::var("RUN_ENV").as_deref() == Ok("qa");
        if explicit_qa {
            match dotenvy::from_path(".env.qa") {
                Ok(()) => {}
                Err(dotenvy::Error::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => {
                    return Err(ConfigError::Message(
                        "Unable to load QA private environment file".into(),
                    ));
                }
            }
        } else {
            let _ = dotenvy::dotenv();
        }
        let mode = env::var("RUN_ENV").unwrap_or_else(|_| "local".into());
        if mode == "qa" && !explicit_qa {
            return Err(ConfigError::Message(
                "QA startup requires explicit RUN_ENV=qa before loading private environment files"
                    .into(),
            ));
        }
        Self::load(
            Config::builder()
                .add_source(File::with_name("application"))
                .add_source(File::with_name(&format!("application.{mode}")).required(mode == "qa")),
            &mode,
        )
    }

    fn load(mut builder: ConfigBuilder<DefaultState>, mode: &str) -> Result<Self, ConfigError> {
        if !matches!(mode, "local" | "qa" | "prod") {
            return Err(ConfigError::Message(
                "RUN_ENV must be local, qa or prod".into(),
            ));
        }
        if mode == "prod" {
            builder = builder.set_override("scheduler.implementation", "vercel")?;
        }
        if mode == "prod" || env::var("VERCEL_ENV").as_deref() == Ok("development") {
            let (origin, insecure_local) = deployment_origin()?;
            builder = builder
                .set_override("browser.public_origin", origin)?
                .set_override("browser.insecure_local", insecure_local)?;
            if env::var("VERCEL_ENV").as_deref() != Ok("production") {
                builder =
                    builder.set_override("browser.additional_origins", Vec::<String>::new())?;
            }
        }
        if let Ok(secret) = env::var("CRON_SECRET") {
            builder = builder.set_override("cron_secret", secret)?;
        }
        if let Ok(db_url) = env::var("DATABASE_URL") {
            builder = builder.set_override("database.url", db_url)?;
        }
        if let Ok(jwt_secret) = env::var("JWT_SECRET") {
            builder = builder.set_override("jwt.secret", jwt_secret)?;
        }

        if let Ok(shopify_access_token) = env::var("SHOPIFY_ACCESS_TOKEN") {
            builder = builder.set_override("client.shopify.access_token", shopify_access_token)?;
        }
        if let Ok(resend_api_key) = env::var("RESEND_API_KEY") {
            builder = builder.set_override("client.resend.api_key", resend_api_key)?;
        }
        for field in [
            "name",
            "phone",
            "email",
            "address",
            "sub_district",
            "district",
            "province",
            "postcode",
            "country_code",
        ] {
            if let Ok(value) = env::var(format!("SHIPPING_SENDER_{}", field.to_ascii_uppercase())) {
                builder = builder.set_override(format!("shipping_sender.{field}"), value)?;
            }
        }
        if let Ok(client_id) = env::var("PAYPAL_CLIENT_ID") {
            builder = builder.set_override("client.paypal.client_id", client_id)?;
        }
        if let Ok(client_secret) = env::var("PAYPAL_CLIENT_SECRET") {
            builder = builder.set_override("client.paypal.client_secret", client_secret)?;
        }
        for (variable, field) in [
            ("S3_ACCESS_KEY_ID", "access_key_id"),
            ("S3_SECRET_ACCESS_KEY", "secret_access_key"),
        ] {
            if let Ok(value) = env::var(variable) {
                builder = builder.set_override(format!("client.s3.{field}"), value)?;
            }
        }
        if let Ok(key) = env::var("STRIPE_SECRET_KEY") {
            builder = builder.set_override("client.stripe.secret_key", key)?;
        }
        let settings: Self = builder.build()?.try_deserialize()?;
        settings.validate()?;
        if mode == "qa" {
            let key = settings
                .client
                .as_ref()
                .and_then(|client| client.stripe.as_ref())
                .and_then(|stripe| stripe.secret_key.as_deref());
            if !key.is_some_and(|key| {
                ["sk_test_", "rk_test_"].iter().any(|prefix| {
                    key.strip_prefix(prefix)
                        .is_some_and(|suffix| !suffix.is_empty())
                }) && !key.chars().any(char::is_whitespace)
            }) {
                return Err(ConfigError::Message(
                    "QA requires STRIPE_SECRET_KEY to be a nonempty sk_test_ or rk_test_ server key".into(),
                ));
            }
        }
        Ok(settings)
    }
}

impl BrowserConfig {
    pub(crate) fn origins(&self) -> impl Iterator<Item = &str> {
        std::iter::once(self.public_origin.as_str())
            .chain(self.additional_origins.iter().map(String::as_str))
    }
}

fn validate_browser_origin(value: &str, insecure_local: bool) -> Result<(), ConfigError> {
    let origin: axum::http::Uri = value
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
    if authority.as_str().contains(['@', '*'])
        || (authority.as_str() != authority.host() && authority.port_u16().is_none())
        || format!("{scheme}://{authority}") != value
        || (insecure_local && (scheme != "http" || !loopback))
        || (!insecure_local && scheme != "https")
    {
        return Err(ConfigError::Message(
            "Browser cookies require HTTPS or explicit loopback HTTP".into(),
        ));
    }
    Ok(())
}

fn deployment_origin() -> Result<(String, bool), ConfigError> {
    let environment = env::var("VERCEL_ENV").map_err(|_| {
        ConfigError::Message("Vercel deployment environment metadata is required".into())
    })?;
    if !matches!(
        environment.as_str(),
        "production" | "preview" | "development"
    ) {
        return Err(ConfigError::Message(
            "Invalid Vercel deployment environment metadata".into(),
        ));
    }
    let hostname = if environment == "production" {
        env::var("VERCEL_PROJECT_PRODUCTION_URL").or_else(|_| env::var("VERCEL_URL"))
    } else {
        env::var("VERCEL_URL")
    }
    .map_err(|_| ConfigError::Message("Vercel public deployment metadata is required".into()))?;
    let development = environment == "development";
    if development {
        let valid = hostname.rsplit_once(':').is_some_and(|(host, port)| {
            matches!(host, "localhost" | "127.0.0.1" | "[::1]")
                && port.parse::<u16>().is_ok_and(|port| port != 0)
                && port.bytes().all(|byte| byte.is_ascii_digit())
        });
        if !valid {
            return Err(ConfigError::Message(
                "Invalid Vercel development metadata".into(),
            ));
        }
    } else if !hostname.ends_with(".vercel.app")
        || hostname.split('.').any(|label| {
            label.is_empty()
                || label.starts_with('-')
                || label.ends_with('-')
                || !label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        })
    {
        return Err(ConfigError::Message(
            "Invalid Vercel deployment metadata".into(),
        ));
    }
    Ok((
        format!(
            "{}://{hostname}",
            if development { "http" } else { "https" }
        ),
        development,
    ))
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

#[derive(Clone, Deserialize)]
pub struct ShippingSender {
    pub name: String,
    pub phone: String,
    pub email: String,
    pub address: String,
    pub sub_district: String,
    pub district: String,
    pub province: String,
    pub postcode: String,
    pub country_code: String,
}

impl std::fmt::Debug for ShippingSender {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ShippingSender { [REDACTED] }")
    }
}

impl std::fmt::Debug for Settings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Settings")
            .field("cron_secret", &"[REDACTED]")
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
#[path = "tests/config.rs"]
mod tests;
