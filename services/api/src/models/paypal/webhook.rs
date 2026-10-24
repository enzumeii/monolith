use super::WebhookEventType;
use crate::client::ClientError;
use serde_json::{Map, Value};

/// Authenticated notification identity; this is never proof of collected payment.
#[derive(Debug, PartialEq, Eq)]
pub struct VerifiedEvent {
    pub event_type: WebhookEventType,
    pub order_id: Option<String>,
    pub checkout_reference: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum WebhookError {
    #[error("invalid PayPal webhook configuration")]
    InvalidConfiguration,
    #[error("PayPal webhook input exceeds supported bounds")]
    InputTooLarge,
    #[error("invalid PayPal transmission header")]
    InvalidHeader,
    #[error("invalid PayPal event data")]
    InvalidEvent,
    #[error("PayPal webhook signature rejected")]
    InvalidSignature,
    #[error("PayPal webhook verification failed")]
    Upstream(#[from] ClientError),
}

pub(crate) fn decode(event: &Value) -> Result<VerifiedEvent, WebhookError> {
    required_id(event.get("id"))?;
    let event_type = WebhookEventType::from(required_id(event.get("event_type"))?);
    let resource = event
        .get("resource")
        .and_then(Value::as_object)
        .ok_or(WebhookError::InvalidEvent)?;
    let related = match resource.get("supplementary_data") {
        None => None,
        Some(data) => {
            let data = data.as_object().ok_or(WebhookError::InvalidEvent)?;
            match data.get("related_ids") {
                None => None,
                Some(ids) => Some(ids.as_object().ok_or(WebhookError::InvalidEvent)?),
            }
        }
    };
    let related_order = optional_id(related, "order_id")?;
    let related_capture = optional_id(related, "capture_id")?;
    let order_event = matches!(
        event_type,
        WebhookEventType::OrderApproved | WebhookEventType::OrderCompleted
    );
    let capture_event = matches!(
        event_type,
        WebhookEventType::CaptureCompleted
            | WebhookEventType::CapturePending
            | WebhookEventType::CaptureDenied
            | WebhookEventType::CaptureReversed
    );
    let resource_id = if matches!(event_type, WebhookEventType::Other(_)) {
        None
    } else {
        Some(required_id(resource.get("id"))?)
    };
    if (order_event
        && related_order
            .as_ref()
            .is_some_and(|id| Some(id) != resource_id.as_ref()))
        || (capture_event
            && related_capture
                .as_ref()
                .is_some_and(|id| Some(id) != resource_id.as_ref()))
    {
        return Err(WebhookError::InvalidEvent);
    }
    let order_id = if order_event {
        resource_id
    } else {
        related_order
    };
    let checkout_reference = event
        .pointer("/resource/custom_id")
        .or_else(|| event.pointer("/resource/purchase_units/0/custom_id"))
        .and_then(Value::as_str)
        .map(str::to_owned);
    Ok(VerifiedEvent {
        event_type,
        order_id,
        checkout_reference,
    })
}

pub(crate) fn valid_header(value: &str) -> bool {
    !value.trim().is_empty() && !value.chars().any(char::is_control)
}

fn required_id(value: Option<&Value>) -> Result<String, WebhookError> {
    value
        .and_then(Value::as_str)
        .filter(|value| value.len() <= 255 && valid_header(value))
        .map(str::to_owned)
        .ok_or(WebhookError::InvalidEvent)
}

fn optional_id(
    related: Option<&Map<String, Value>>,
    name: &str,
) -> Result<Option<String>, WebhookError> {
    related
        .and_then(|ids| ids.get(name))
        .map(|value| required_id(Some(value)))
        .transpose()
}

#[cfg(test)]
#[path = "../../tests/models/paypal/webhook.rs"]
mod tests;
