use crate::models::stripe::webhook::{VerifiedEvent, WebhookError};
use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;
use std::time::Duration;

const MAX_BODY_BYTES: usize = 1_048_576;
const MAX_HEADER_BYTES: usize = 16_384;
const MAX_HEADER_PARTS: usize = 64;
const MAX_TOLERANCE: Duration = Duration::from_mins(15);

/// Verify exact raw bytes before decoding a Stripe snapshot event.
///
/// `now` is Unix time in seconds. Tolerance must be between one and 900 seconds;
/// 300 seconds is the usual caller choice. This does not prevent replay within
/// that window: callers must reconcile authoritative payment state and guard
/// mutations against repeated delivery.
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
    VerifiedEvent::decode(raw)
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

#[cfg(test)]
#[path = "../../tests/client/stripe/webhook.rs"]
mod tests;
