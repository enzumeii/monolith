use super::PaypalClient;
use crate::models::paypal::{
    VerifyWebhookSignatureRequest, WebhookVerificationStatus,
    webhook::{self, VerifiedEvent, WebhookError, valid_header},
};
use reqwest::Url;

const MAX_BODY_BYTES: usize = 262_144;
const MAX_HEADER_BYTES: usize = 4096;

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
    let value = serde_json::from_str(request.webhook_event.get())
        .map_err(|_| WebhookError::InvalidEvent)?;
    let event = webhook::decode(&value)?;
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
        || request.webhook_event.get().len() > MAX_BODY_BYTES
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

#[cfg(test)]
#[path = "../../tests/client/paypal/webhook.rs"]
mod tests;
