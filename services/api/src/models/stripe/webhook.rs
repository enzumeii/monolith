use serde::Deserialize;

/// Authenticated snapshot event data; authentication alone never establishes payment success.
#[derive(Debug, PartialEq, Eq)]
pub struct VerifiedEvent {
    pub event_type: EventType,
    pub livemode: bool,
    pub account: Option<String>,
    pub object_id: String,
    pub checkout_reference: Option<String>,
}

#[derive(Debug, PartialEq, Eq, Deserialize)]
#[serde(from = "String")]
pub enum EventType {
    CheckoutSessionCompleted,
    CheckoutSessionExpired,
    CheckoutSessionAsyncPaymentSucceeded,
    CheckoutSessionAsyncPaymentFailed,
    PaymentIntentSucceeded,
    PaymentIntentPaymentFailed,
    Other(String),
}

#[derive(Debug, PartialEq, Eq, Deserialize)]
struct EventObject {
    id: String,
    #[serde(rename = "object")]
    object_type: String,
    payment_intent: Option<EventReference>,
    latest_charge: Option<EventReference>,
    #[serde(flatten)]
    additional_fields: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
enum EventReference {
    Id(String),
    Expanded { id: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum WebhookError {
    #[error("invalid Stripe webhook verification configuration")]
    InvalidConfiguration,
    #[error("Stripe webhook input exceeds supported bounds")]
    InputTooLarge,
    #[error("invalid Stripe signature header")]
    InvalidHeader,
    #[error("Stripe webhook timestamp outside tolerance")]
    TimestampOutsideTolerance,
    #[error("invalid Stripe webhook signature")]
    InvalidSignature,
    #[error("invalid Stripe event data")]
    InvalidEvent,
}

#[derive(Deserialize)]
struct EventEnvelope {
    id: String,
    #[serde(rename = "type")]
    event_type: EventType,
    livemode: bool,
    account: Option<String>,
    #[serde(rename = "api_version")]
    _api_version: Option<String>,
    data: EventData,
}

#[derive(Deserialize)]
struct EventData {
    object: EventObject,
}

impl VerifiedEvent {
    pub(crate) fn decode(raw: &[u8]) -> Result<Self, WebhookError> {
        let event: EventEnvelope =
            serde_json::from_slice(raw).map_err(|_| WebhookError::InvalidEvent)?;
        validate_event(&event)?;
        let checkout_reference = event
            .data
            .object
            .additional_fields
            .get("metadata")
            .and_then(|metadata| metadata.get("checkout_id"))
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned);
        Ok(Self {
            event_type: event.event_type,
            livemode: event.livemode,
            account: event.account,
            object_id: event.data.object.id,
            checkout_reference,
        })
    }
}

impl EventType {
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::CheckoutSessionCompleted => "checkout.session.completed",
            Self::CheckoutSessionExpired => "checkout.session.expired",
            Self::CheckoutSessionAsyncPaymentSucceeded => {
                "checkout.session.async_payment_succeeded"
            }
            Self::CheckoutSessionAsyncPaymentFailed => "checkout.session.async_payment_failed",
            Self::PaymentIntentSucceeded => "payment_intent.succeeded",
            Self::PaymentIntentPaymentFailed => "payment_intent.payment_failed",
            Self::Other(raw) => raw,
        }
    }
}

impl From<String> for EventType {
    fn from(raw: String) -> Self {
        match raw.as_str() {
            "checkout.session.completed" => Self::CheckoutSessionCompleted,
            "checkout.session.expired" => Self::CheckoutSessionExpired,
            "checkout.session.async_payment_succeeded" => {
                Self::CheckoutSessionAsyncPaymentSucceeded
            }
            "checkout.session.async_payment_failed" => Self::CheckoutSessionAsyncPaymentFailed,
            "payment_intent.succeeded" => Self::PaymentIntentSucceeded,
            "payment_intent.payment_failed" => Self::PaymentIntentPaymentFailed,
            _ => Self::Other(raw),
        }
    }
}

impl EventReference {
    #[must_use]
    fn id(&self) -> &str {
        match self {
            Self::Id(id) | Self::Expanded { id } => id,
        }
    }
}

fn validate_event(event: &EventEnvelope) -> Result<(), WebhookError> {
    let object = &event.data.object;
    if !super::valid_id(&event.id)
        || event.event_type.as_str().is_empty()
        || event.event_type.as_str().len() > 255
        || !super::valid_id(&object.id)
        || object.object_type.is_empty()
        || object.object_type.len() > 255
        || event
            .account
            .as_deref()
            .is_some_and(|id| !super::valid_id(id))
        || object
            .payment_intent
            .as_ref()
            .is_some_and(|reference| !super::valid_id(reference.id()))
        || object
            .latest_charge
            .as_ref()
            .is_some_and(|reference| !super::valid_id(reference.id()))
    {
        return Err(WebhookError::InvalidEvent);
    }
    Ok(())
}

#[cfg(test)]
#[path = "../../tests/models/stripe/webhook.rs"]
mod tests;
