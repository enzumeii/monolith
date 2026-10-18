use crate::{
    client::{ClientError, resend::ResendClient, shopify::ShopifyClient},
    config::Settings,
};
use sqlx::PgPool;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub shopify: Result<Option<Arc<ShopifyClient>>, Arc<ClientError>>,
    pub resend: Result<Option<Arc<ResendClient>>, Arc<ClientError>>,
    pub jwt_key: Arc<Vec<u8>>,
    pub settings: Arc<Settings>,
}

pub fn app_state(pool: PgPool, settings: Settings) -> Result<AppState, config::ConfigError> {
    let jwt_key = Arc::new(settings.jwt.key()?);
    Ok(AppState {
        pool,
        shopify: settings
            .client
            .as_ref()
            .and_then(|c| c.shopify.as_ref())
            .map(ShopifyClient::new)
            .transpose()
            .map(|client| client.map(Arc::new))
            .map_err(Arc::new),
        resend: settings
            .client
            .as_ref()
            .and_then(|client| client.resend.as_ref())
            .map(ResendClient::new)
            .transpose()
            .map(|client| client.map(Arc::new))
            .map_err(Arc::new),
        jwt_key,
        settings: Arc::new(settings),
    })
}

#[cfg(test)]
#[path = "tests/state.rs"]
mod tests;
