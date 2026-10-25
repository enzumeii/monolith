use crate::{
    config::CheckoutSettings,
    error::AppError,
    models::{
        paypal::{WebhookEventType, webhook as paypal},
        stripe::webhook as stripe,
    },
};
use axum::http::StatusCode;
use domain::dto::checkout::CheckoutProvider;
use uuid::Uuid;

pub(crate) struct Notification {
    pub provider: CheckoutProvider,
    pub resource_id: Option<String>,
    pub correlation_id: Option<Uuid>,
}

pub(crate) fn stripe(
    config: &CheckoutSettings,
    event: stripe::VerifiedEvent,
    expected_account: Option<&str>,
) -> Result<Option<Notification>, AppError> {
    if event.livemode != config.stripe_livemode
        || event
            .account
            .as_deref()
            .is_some_and(|account| Some(account) != expected_account)
    {
        return Err(invalid("Stripe webhook account or environment mismatch"));
    }
    if matches!(event.event_type, stripe::EventType::Other(_)) {
        return Ok(None);
    }
    let correlation_id = event
        .checkout_reference
        .as_deref()
        .and_then(|value| Uuid::parse_str(value).ok());
    Ok(Some(Notification {
        provider: CheckoutProvider::Stripe,
        resource_id: event
            .object_id
            .starts_with("cs_")
            .then_some(event.object_id),
        correlation_id,
    }))
}

pub(crate) fn paypal(event: paypal::VerifiedEvent) -> Option<Notification> {
    if matches!(event.event_type, WebhookEventType::Other(_)) {
        return None;
    }
    let correlation_id = event
        .checkout_reference
        .as_deref()
        .and_then(|value| Uuid::parse_str(value).ok());
    Some(Notification {
        provider: CheckoutProvider::Paypal,
        resource_id: event.order_id,
        correlation_id,
    })
}

fn invalid(message: &'static str) -> AppError {
    AppError::new(StatusCode::BAD_REQUEST, message)
}

#[cfg(test)]
#[path = "../../tests/services/checkout/notification.rs"]
mod tests;
