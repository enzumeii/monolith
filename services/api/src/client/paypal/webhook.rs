use super::{ClientError, PaypalClient};
use crate::models::paypal::{
    VerifyWebhookSignatureRequest, WebhookEventType, WebhookVerificationStatus,
};
use reqwest::Url;
use serde_json::{Map, Value};

const MAX_BODY_BYTES: usize = 262_144;
const MAX_HEADER_BYTES: usize = 4096;

/// Authenticated notification identity; this is never proof of collected payment.
#[derive(Debug, PartialEq, Eq)]
pub struct VerifiedEvent {
    pub event_type: WebhookEventType,
    pub order_id: Option<String>,
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

/// Authenticate a notification using `PayPal`'s verification postback.
///
/// `request.webhook_id` must come from the consuming application's trusted
/// configuration. The caller owns original HTTP header extraction, ingress body
/// limits and durable replay handling. No certificate URL or event link is
/// fetched locally. Current payment evidence must be checked independently.
pub async fn verify_event(
    client: &PaypalClient,
    request: &VerifyWebhookSignatureRequest,
) -> Result<VerifiedEvent, WebhookError> {
    validate_request(request)?;
    let event = parse_event(&request.webhook_event)?;
    let verified = client.verify_webhook_signature(request).await?;
    if verified.verification_status != WebhookVerificationStatus::Success {
        return Err(WebhookError::InvalidSignature);
    }
    Ok(event)
}

fn validate_request(request: &VerifyWebhookSignatureRequest) -> Result<(), WebhookError> {
    let headers = [
        request.auth_algo.as_str(),
        request.cert_url.as_str(),
        request.transmission_id.as_str(),
        request.transmission_sig.as_str(),
        request.transmission_time.as_str(),
    ];
    if headers.iter().any(|value| value.len() > MAX_HEADER_BYTES)
        || request.webhook_id.len() > MAX_HEADER_BYTES
        || serde_json::to_vec(&request.webhook_event)
            .map_err(|_| WebhookError::InvalidEvent)?
            .len()
            > MAX_BODY_BYTES
    {
        return Err(WebhookError::InputTooLarge);
    }
    if !valid_header(&request.webhook_id) {
        return Err(WebhookError::InvalidConfiguration);
    }
    if headers.iter().any(|value| !valid_header(value)) {
        return Err(WebhookError::InvalidHeader);
    }
    chrono::DateTime::parse_from_rfc3339(&request.transmission_time)
        .map_err(|_| WebhookError::InvalidHeader)?;
    let certificate = Url::parse(&request.cert_url).map_err(|_| WebhookError::InvalidHeader)?;
    if certificate.scheme() != "https"
        || certificate.host_str().is_none()
        || !certificate.username().is_empty()
        || certificate.password().is_some()
        || certificate.fragment().is_some()
    {
        return Err(WebhookError::InvalidHeader);
    }
    Ok(())
}

fn parse_event(event: &Value) -> Result<VerifiedEvent, WebhookError> {
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
    Ok(VerifiedEvent {
        event_type,
        order_id,
    })
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

fn valid_header(value: &str) -> bool {
    !value.trim().is_empty() && !value.chars().any(char::is_control)
}

#[cfg(test)]
#[path = "../../tests/client/paypal/webhook.rs"]
mod tests;
