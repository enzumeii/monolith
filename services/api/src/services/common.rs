use crate::{
    client::{ClientError, shopify::ShopifyClient},
    error::{AppError, client_timeout},
    state::AppState,
};
use axum::http::StatusCode;

pub(super) fn shopify_client<'a>(
    state: &'a AppState,
    unavailable_message: &'static str,
) -> Result<&'a ShopifyClient, AppError> {
    state
        .shopify
        .as_ref()
        .map_err(|error| {
            AppError::new(StatusCode::SERVICE_UNAVAILABLE, unavailable_message)
                .with_source(error.clone())
        })?
        .as_deref()
        .ok_or_else(|| AppError::new(StatusCode::SERVICE_UNAVAILABLE, unavailable_message))
}

#[track_caller]
pub(super) fn upstream(error: ClientError) -> AppError {
    let status = if client_timeout(&error) {
        StatusCode::GATEWAY_TIMEOUT
    } else {
        StatusCode::BAD_GATEWAY
    };
    AppError::new(status, "Upstream request failed").with_source(error)
}

#[cfg(test)]
#[path = "../tests/services/common.rs"]
pub(super) mod tests;
