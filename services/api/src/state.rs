use crate::config::Settings;
use sqlx::PgPool;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub jwt_key: Arc<Vec<u8>>,
    pub settings: Arc<Settings>,
}

impl AppState {
    pub fn shopify_client(
        &self,
    ) -> Result<crate::client::shopify::ShopifyClient, crate::client::ClientError> {
        let config = self
            .settings
            .client
            .as_ref()
            .and_then(|client| client.shopify.as_ref())
            .ok_or(crate::client::ClientError::InvalidInput(
                "Shopify configuration is missing",
            ))?;
        crate::client::shopify::ShopifyClient::new(config)
    }
}

pub fn app_state(pool: PgPool, settings: Settings) -> Result<AppState, config::ConfigError> {
    let jwt_key = Arc::new(settings.jwt.key()?);
    Ok(AppState {
        pool,
        jwt_key,
        settings: Arc::new(settings),
    })
}

#[cfg(test)]
#[path = "tests/state.rs"]
mod tests;
