use crate::{client::shopify::ShopifyClient, error::AppError, state::AppState};
use axum::http::StatusCode;

pub(super) fn shopify_client(state: &AppState) -> Result<&ShopifyClient, AppError> {
    state
        .shopify
        .as_ref()
        .map_err(|error| {
            AppError::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "Shopify client unavailable",
            )
            .with_source(error.clone())
        })?
        .as_deref()
        .ok_or_else(|| {
            AppError::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "Shopify client unavailable",
            )
        })
}

pub(super) fn resend_client(
    state: &AppState,
) -> Result<&crate::client::resend::ResendClient, AppError> {
    state
        .resend
        .as_ref()
        .map_err(|error| {
            AppError::new(StatusCode::SERVICE_UNAVAILABLE, "Resend client unavailable")
                .with_source(error.clone())
        })?
        .as_deref()
        .ok_or_else(|| AppError::new(StatusCode::SERVICE_UNAVAILABLE, "Resend client unavailable"))
}

pub(super) fn stripe_client(
    state: &AppState,
) -> Result<Option<&crate::client::stripe::StripeClient>, AppError> {
    state
        .stripe
        .as_ref()
        .map(|value| value.as_deref())
        .map_err(|error| {
            AppError::new(StatusCode::SERVICE_UNAVAILABLE, "Stripe client unavailable")
                .with_source(error.clone())
        })
}

pub(super) fn paypal_client(
    state: &AppState,
) -> Result<Option<&crate::client::paypal::PaypalClient>, AppError> {
    state
        .paypal
        .as_ref()
        .map(|value| value.as_deref())
        .map_err(|error| {
            AppError::new(StatusCode::SERVICE_UNAVAILABLE, "PayPal client unavailable")
                .with_source(error.clone())
        })
}

#[cfg(test)]
#[path = "../tests/handlers/dependencies.rs"]
mod tests;
