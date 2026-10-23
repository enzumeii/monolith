use crate::{
    client::{
        paypal::{self, PaypalClient},
        stripe,
    },
    config::CheckoutSettings,
    error::AppError,
    models::paypal::{VerifyWebhookSignatureRequest, WebhookEventType},
};
use axum::http::StatusCode;
use domain::dto::checkout::CheckoutProvider;
use serde_json::Value;
use std::time::Duration;
use uuid::Uuid;

#[derive(Clone, Copy)]
pub(crate) struct StripeAuthentication<'a> {
    pub signature: &'a str,
    pub secret: &'a str,
}

pub(crate) struct Notification {
    pub provider: CheckoutProvider,
    pub resource_id: Option<String>,
    pub correlation_id: Option<Uuid>,
}

pub(crate) fn stripe(
    config: &CheckoutSettings,
    authentication: StripeAuthentication<'_>,
    raw: &[u8],
) -> Result<Option<Notification>, AppError> {
    let event = stripe::webhook::verify_event(
        raw,
        authentication.signature,
        authentication.secret,
        chrono::Utc::now().timestamp(),
        Duration::from_secs(300),
    )
    .map_err(|_| invalid("Invalid Stripe webhook authentication or event"))?;
    if event.livemode != config.stripe_livemode
        || event
            .account
            .as_deref()
            .is_some_and(|account| account != config.stripe_expected_account)
    {
        return Err(invalid("Stripe webhook account or environment mismatch"));
    }
    if matches!(event.event_type, stripe::webhook::EventType::Other(_)) {
        return Ok(None);
    }
    let value: Value =
        serde_json::from_slice(raw).map_err(|_| invalid("Invalid Stripe webhook event"))?;
    let correlation_id = value
        .pointer("/data/object/metadata/checkout_id")
        .and_then(Value::as_str)
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

pub(crate) async fn paypal(
    client: &PaypalClient,
    request: &VerifyWebhookSignatureRequest,
) -> Result<Option<Notification>, AppError> {
    let event = paypal::webhook::verify_event(client, request)
        .await
        .map_err(|error| match error {
            paypal::webhook::WebhookError::Upstream(error) => crate::error::upstream(error),
            _ => invalid("Invalid PayPal webhook authentication or event"),
        })?;
    if matches!(event.event_type, WebhookEventType::Other(_)) {
        return Ok(None);
    }
    let value = &request.webhook_event;
    let correlation_id = value
        .pointer("/resource/custom_id")
        .or_else(|| value.pointer("/resource/purchase_units/0/custom_id"))
        .and_then(Value::as_str)
        .and_then(|value| Uuid::parse_str(value).ok());
    Ok(Some(Notification {
        provider: CheckoutProvider::Paypal,
        resource_id: event.order_id,
        correlation_id,
    }))
}

fn invalid(message: &'static str) -> AppError {
    AppError::new(StatusCode::BAD_REQUEST, message)
}

#[cfg(test)]
#[path = "../../tests/services/checkout/notification.rs"]
mod tests;
