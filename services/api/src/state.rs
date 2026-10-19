use crate::{
    client::{
        ClientError, paypal::PaypalClient, resend::ResendClient, shopify::ShopifyClient,
        stripe::StripeClient,
    },
    config::Settings,
};
use sqlx::PgPool;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub shopify: Result<Option<Arc<ShopifyClient>>, Arc<ClientError>>,
    pub paypal: Result<Option<Arc<PaypalClient>>, Arc<ClientError>>,
    pub resend: Result<Option<Arc<ResendClient>>, Arc<ClientError>>,
    pub stripe: Result<Option<Arc<StripeClient>>, Arc<ClientError>>,
    pub jwt_key: Arc<Vec<u8>>,
    pub settings: Arc<Settings>,
}

pub fn app_state(pool: PgPool, settings: Settings) -> Result<AppState, config::ConfigError> {
    let jwt_key = Arc::new(settings.jwt.key()?);
    let clients = settings.client.as_ref();
    Ok(AppState {
        pool,
        shopify: optional_client(
            clients.and_then(|client| client.shopify.as_ref()),
            ShopifyClient::new,
        ),
        resend: optional_client(
            clients.and_then(|client| client.resend.as_ref()),
            ResendClient::new,
        ),
        paypal: optional_client(
            clients.and_then(|client| client.paypal.as_ref()),
            PaypalClient::new,
        ),
        stripe: optional_client(
            clients.and_then(|client| client.stripe.as_ref()),
            StripeClient::new,
        ),
        jwt_key,
        settings: Arc::new(settings),
    })
}

fn optional_client<C, T>(
    config: Option<&C>,
    create: impl FnOnce(&C) -> Result<T, ClientError>,
) -> Result<Option<Arc<T>>, Arc<ClientError>> {
    config
        .map(create)
        .transpose()
        .map(|client| client.map(Arc::new))
        .map_err(Arc::new)
}

#[cfg(test)]
#[path = "tests/state.rs"]
mod tests;
