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
pub struct ClientConfig {
    pub paypal: Option<PaypalConfig>,
    pub shopify: Option<ShopifyConfig>,
    pub resend: Option<ResendConfig>,
    pub stripe: Option<StripeConfig>,
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

#[derive(Debug, Deserialize, Clone)]
pub struct Settings {
    pub shipping_sender: Option<ShippingSender>,
    pub client: Option<ClientConfig>,
    pub browser: BrowserConfig,
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub jwt: JwtSettings,
    #[serde(default)]
    pub scheduler: SchedulerConfig,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(default)]
pub struct SchedulerConfig {
    pub poll_interval_secs: u64,
    pub concurrency: usize,
    pub max_generation_attempts: u32,
    pub retry_backoff_secs: u64,
    pub lease_secs: u64,
    pub max_payload_bytes: usize,
    pub max_email_bytes: usize,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            poll_interval_secs: 10,
            concurrency: 2,
            max_generation_attempts: 5,
            retry_backoff_secs: 60,
            lease_secs: 300,
            max_payload_bytes: 20 * 1024 * 1024,
            max_email_bytes: 20 * 1024 * 1024,
        }
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
impl Settings {
    pub fn validate(&self) -> Result<(), ConfigError> {
        self.jwt.key()?;
        if self.scheduler.poll_interval_secs == 0
            || !(1..=32).contains(&self.scheduler.concurrency)
            || self.scheduler.max_generation_attempts == 0
            || self.scheduler.retry_backoff_secs == 0
            || self.scheduler.lease_secs == 0
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
        if let Ok(key) = env::var("STRIPE_SECRET_KEY") {
            builder = builder.set_override("client.stripe.secret_key", key)?;
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

#[cfg(test)]
#[path = "tests/config.rs"]
mod tests;
