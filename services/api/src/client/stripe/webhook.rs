use hmac::{Hmac, KeyInit, Mac};
use serde::Deserialize;
use sha2::Sha256;
use std::time::Duration;

const MAX_BODY_BYTES: usize = 1_048_576;
const MAX_HEADER_BYTES: usize = 16_384;
const MAX_HEADER_PARTS: usize = 64;
const MAX_TOLERANCE: Duration = Duration::from_mins(15);

/// Authenticated snapshot event data; authentication alone never establishes payment success.
#[derive(Debug, PartialEq, Eq)]
pub struct VerifiedEvent {
    pub id: String,
    pub event_type: EventType,
    pub livemode: bool,
    pub account: Option<String>,
    pub api_version: Option<String>,
    pub object: EventObject,
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
pub struct EventObject {
    pub id: String,
    #[serde(rename = "object")]
    pub object_type: String,
    pub payment_intent: Option<EventReference>,
    pub latest_charge: Option<EventReference>,
}

#[derive(Debug, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum EventReference {
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
    api_version: Option<String>,
    data: EventData,
}

#[derive(Deserialize)]
struct EventData {
    object: EventObject,
}

/// Verify exact raw bytes before decoding a Stripe snapshot event.
///
/// `now` is Unix time in seconds. Tolerance must be between one and 900 seconds;
/// 300 seconds is the usual caller choice. This does not prevent replay within
/// that window: a future ingestion consumer must durably deduplicate event IDs.
pub fn verify_event(
    raw: &[u8],
    signature_header: &str,
    secret: &str,
    now: i64,
    tolerance: Duration,
) -> Result<VerifiedEvent, WebhookError> {
    if secret.is_empty()
        || secret.len() > 4096
        || now < 0
        || tolerance < Duration::from_secs(1)
        || tolerance > MAX_TOLERANCE
    {
        return Err(WebhookError::InvalidConfiguration);
    }
    if raw.len() > MAX_BODY_BYTES || signature_header.len() > MAX_HEADER_BYTES {
        return Err(WebhookError::InputTooLarge);
    }
    let (timestamp_text, timestamp, signatures) = parse_header(signature_header)?;
    if now.abs_diff(timestamp) > tolerance.as_secs() {
        return Err(WebhookError::TimestampOutsideTolerance);
    }
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes())
        .map_err(|_| WebhookError::InvalidConfiguration)?;
    mac.update(timestamp_text.as_bytes());
    mac.update(b".");
    mac.update(raw);
    // Each candidate uses the MAC crate's constant-time comparison. Evaluate all
    // rotation candidates so the matching signature's position is not exposed.
    let valid = signatures.iter().fold(false, |valid, signature| {
        mac.clone().verify_slice(signature).is_ok() | valid
    });
    if !valid {
        return Err(WebhookError::InvalidSignature);
    }
    let event: EventEnvelope =
        serde_json::from_slice(raw).map_err(|_| WebhookError::InvalidEvent)?;
    validate_event(&event)?;
    Ok(VerifiedEvent {
        id: event.id,
        event_type: event.event_type,
        livemode: event.livemode,
        account: event.account,
        api_version: event.api_version,
        object: event.data.object,
    })
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
    pub fn id(&self) -> &str {
        match self {
            Self::Id(id) | Self::Expanded { id } => id,
        }
    }
}

fn parse_header(header: &str) -> Result<(&str, i64, Vec<[u8; 32]>), WebhookError> {
    let mut timestamp = None;
    let mut signatures = Vec::new();
    for (index, part) in header.split(',').enumerate() {
        if index >= MAX_HEADER_PARTS {
            return Err(WebhookError::InvalidHeader);
        }
        let (scheme, value) = part
            .trim()
            .split_once('=')
            .ok_or(WebhookError::InvalidHeader)?;
        match scheme {
            "t" => {
                if timestamp.is_some()
                    || value.is_empty()
                    || !value.bytes().all(|b| b.is_ascii_digit())
                {
                    return Err(WebhookError::InvalidHeader);
                }
                let seconds = value
                    .parse::<i64>()
                    .map_err(|_| WebhookError::InvalidHeader)?;
                timestamp = Some((value, seconds));
            }
            "v1" => {
                let mut bytes = [0; 32];
                hex::decode_to_slice(value, &mut bytes).map_err(|_| WebhookError::InvalidHeader)?;
                signatures.push(bytes);
            }
            _ => {}
        }
    }
    let (text, seconds) = timestamp.ok_or(WebhookError::InvalidHeader)?;
    if signatures.is_empty() {
        return Err(WebhookError::InvalidHeader);
    }
    Ok((text, seconds, signatures))
}

fn validate_event(event: &EventEnvelope) -> Result<(), WebhookError> {
    let object = &event.data.object;
    if super::validate_id(&event.id).is_err()
        || event.event_type.as_str().is_empty()
        || event.event_type.as_str().len() > 255
        || super::validate_id(&object.id).is_err()
        || object.object_type.is_empty()
        || object.object_type.len() > 255
        || event
            .account
            .as_deref()
            .is_some_and(|id| super::validate_id(id).is_err())
        || object
            .payment_intent
            .as_ref()
            .is_some_and(|reference| super::validate_id(reference.id()).is_err())
        || object
            .latest_charge
            .as_ref()
            .is_some_and(|reference| super::validate_id(reference.id()).is_err())
    {
        return Err(WebhookError::InvalidEvent);
    }
    Ok(())
}

#[cfg(test)]
#[path = "../../tests/client/stripe/webhook.rs"]
mod tests;
