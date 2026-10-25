use crate::{error::AppError, state::AppState};
use axum::http::StatusCode;

pub(super) fn shopify_client(
    state: &AppState,
) -> Result<&crate::client::shopify::ShopifyClient, AppError> {
    crate::state::required_client(&state.shopify, "Shopify client unavailable")
}

pub(super) fn resend_client(
    state: &AppState,
) -> Result<&crate::client::resend::ResendClient, AppError> {
    crate::state::required_client(&state.resend, "Resend client unavailable")
}

pub(super) fn stripe_client(
    state: &AppState,
) -> Result<&crate::client::stripe::StripeClient, AppError> {
    crate::state::required_client(&state.stripe, "Stripe client unavailable")
}

pub(super) fn paypal_client(
    state: &AppState,
) -> Result<&crate::client::paypal::PaypalClient, AppError> {
    crate::state::required_client(&state.paypal, "PayPal client unavailable")
}

pub(super) fn s3_client(state: &AppState) -> Result<&crate::client::s3::S3Client, AppError> {
    crate::state::required_client(&state.s3, "S3 client unavailable")
}

pub(super) fn checkout_settings(
    state: &AppState,
) -> Result<&crate::config::CheckoutSettings, AppError> {
    state.settings.checkout.as_ref().ok_or_else(|| {
        AppError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "Checkout configuration unavailable",
        )
    })
}

pub(super) fn checkout_provider(
    state: &AppState,
    provider: domain::dto::checkout::CheckoutProvider,
) -> Result<(), AppError> {
    match provider {
        domain::dto::checkout::CheckoutProvider::Stripe => {
            stripe_client(state)?;
        }
        domain::dto::checkout::CheckoutProvider::Paypal => {
            paypal_client(state)?;
        }
    }
    Ok(())
}

pub(super) fn checkout_clients(
    state: &AppState,
) -> Result<crate::services::checkout::CheckoutClients<'_>, AppError> {
    Ok(crate::services::checkout::CheckoutClients {
        shopify: shopify_client(state)?,
        stripe: state
            .stripe
            .as_ref()
            .ok()
            .and_then(|client| client.as_deref()),
        paypal: state
            .paypal
            .as_ref()
            .ok()
            .and_then(|client| client.as_deref()),
    })
}

pub(super) fn checkout_webhook_clients(
    state: &AppState,
) -> crate::services::checkout::webhooks::WebhookClients<'_> {
    crate::services::checkout::webhooks::WebhookClients {
        stripe: stripe_client(state),
        paypal: paypal_client(state),
        advancement: checkout_clients(state),
    }
}

#[cfg(test)]
#[path = "../tests/handlers/dependencies.rs"]
mod tests;
