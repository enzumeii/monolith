use crate::{
    client::{ClientError, shopify::ShopifyClient},
    error::{AppError, client_timeout},
    state::AppState,
};
use axum::http::StatusCode;

pub(super) fn shopify_client(state: &AppState) -> Result<&ShopifyClient, AppError> {
    state
        .shopify
        .as_ref()
        .map_err(|error| {
            AppError::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "Shopify report unavailable",
            )
            .with_source(error.clone())
            .at("api::client::shopify")
        })?
        .as_deref()
        .ok_or_else(|| {
            AppError::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "Shopify report unavailable",
            )
            .at(module_path!())
        })
}

pub(super) fn upstream(error: ClientError) -> AppError {
    let origin = match &error {
        ClientError::Origin { origin, .. } => *origin,
        _ => module_path!(),
    };
    let status = if client_timeout(&error) {
        StatusCode::GATEWAY_TIMEOUT
    } else {
        StatusCode::BAD_GATEWAY
    };
    AppError::new(status, "Shopify upstream failure")
        .with_source(error)
        .at(origin)
}

#[cfg(test)]
#[path = "../tests/services/common.rs"]
pub(super) mod tests;
