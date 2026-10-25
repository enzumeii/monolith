use crate::{
    config::CheckoutSettings,
    error::AppError,
    models::{paypal::VerifyWebhookSignatureRequest, shopify::ShopifyId},
};
use axum::{
    body::Bytes,
    extract::rejection::BytesRejection,
    http::{HeaderMap, StatusCode, header::AUTHORIZATION},
};
use domain::dto::checkout::{CreateCheckoutRequest, MAX_CHECKOUT_NOTE_CHARS};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use uuid::Uuid;

#[derive(Clone, Copy)]
pub(crate) struct StripeAuthentication<'a> {
    pub signature: &'a str,
    pub secret: &'a str,
}

pub(crate) fn credential(headers: &HeaderMap) -> Result<Vec<u8>, AppError> {
    let raw = headers
        .get(AUTHORIZATION)
        .and_then(|header| header.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .filter(|value| value.len() == 64)
        .and_then(|value| hex::decode(value).ok())
        .filter(|value| value.len() == 32)
        .ok_or_else(|| {
            AppError::new(StatusCode::UNAUTHORIZED, "Invalid checkout credential")
                .with_bearer_challenge()
        })?;
    Ok(Sha256::digest(raw).to_vec())
}

pub(crate) fn bytes(body: Result<Bytes, BytesRejection>) -> Result<Bytes, AppError> {
    body.map_err(|error| body_error(error.status()))
}

pub(crate) fn body_error(status: StatusCode) -> AppError {
    if status == StatusCode::PAYLOAD_TOO_LARGE {
        AppError::new(status, "Checkout request body is too large")
    } else {
        invalid("Invalid checkout request body")
    }
}

pub(crate) fn create(request: &CreateCheckoutRequest) -> Result<(), AppError> {
    id(&request.operation_id)?;
    id(&request.convention_id)?;
    let email = super::cron::configs::validate_email(&request.email)
        .map_err(|_| invalid("email must be a valid email address"))?;
    if email != request.email {
        return Err(invalid("email must not contain surrounding whitespace"));
    }
    if !(1..=100).contains(&request.items.len()) {
        return Err(invalid("items must contain 1 to 100 distinct variants"));
    }
    if request.expected_currency.len() != 3
        || !request
            .expected_currency
            .bytes()
            .all(|byte| byte.is_ascii_uppercase())
    {
        return Err(invalid(
            "expected_currency must be an uppercase currency code",
        ));
    }
    if let Some(note) = &request.note {
        if note.chars().count() > MAX_CHECKOUT_NOTE_CHARS {
            return Err(invalid("note must contain at most 2000 characters"));
        }
        if note
            .chars()
            .any(|character| character.is_control() && !matches!(character, '\t' | '\r' | '\n'))
        {
            return Err(invalid("note contains unsupported control characters"));
        }
    }
    let mut variants = HashSet::new();
    for item in &request.items {
        let variant = if item.variant_id.starts_with("gid:") {
            let variant = ShopifyId::new(&item.variant_id)
                .and_then(|variant| variant.number("ProductVariant"))
                .map_err(|_| invalid("Invalid checkout variant identifier"))?;
            if item.variant_id != format!("gid://shopify/ProductVariant/{variant}") {
                return Err(invalid("Invalid checkout variant identifier"));
            }
            item.variant_id.clone()
        } else {
            id(&item.variant_id)?.to_string()
        };
        if !variants.insert(variant) {
            return Err(invalid("items must not contain duplicate variants"));
        }
        if !(1..=999).contains(&item.quantity) {
            return Err(invalid("quantity must be between 1 and 999"));
        }
        if item.expected_unit_price <= 0 {
            return Err(invalid("expected_unit_price must be positive minor units"));
        }
    }
    Ok(())
}

pub(crate) fn id(value: &str) -> Result<Uuid, AppError> {
    let id = Uuid::parse_str(value).map_err(|_| invalid("Invalid checkout identifier"))?;
    if id.is_nil() || id.get_version_num() != 4 {
        return Err(invalid("Checkout identifiers must be random UUIDs"));
    }
    Ok(id)
}

pub(crate) fn no_query(value: Option<&str>) -> Result<(), AppError> {
    if value.is_some_and(|value| !value.is_empty()) {
        return Err(invalid("Checkout requests do not accept query parameters"));
    }
    Ok(())
}

pub(crate) fn invalid(message: &'static str) -> AppError {
    AppError::new(StatusCode::BAD_REQUEST, message)
}

pub(crate) fn stripe_authentication<'a>(
    config: &'a CheckoutSettings,
    headers: &'a HeaderMap,
) -> Result<StripeAuthentication<'a>, AppError> {
    if headers.contains_key("paypal-transmission-id") {
        return Err(invalid("Ambiguous webhook authentication"));
    }
    let secret = config
        .stripe_webhook_secret
        .as_deref()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            AppError::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "Stripe webhook configuration unavailable",
            )
        })?;
    Ok(StripeAuthentication {
        signature: webhook_header(headers, "stripe-signature")?,
        secret,
    })
}

pub(crate) fn paypal_authentication(
    config: &CheckoutSettings,
    headers: &HeaderMap,
    raw: &[u8],
) -> Result<VerifyWebhookSignatureRequest, AppError> {
    if headers.contains_key("stripe-signature") {
        return Err(invalid("Ambiguous webhook authentication"));
    }
    if raw.len() > 262_144 {
        return Err(AppError::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            "PayPal webhook payload exceeds supported limit",
        ));
    }
    let webhook_id = config
        .paypal_webhook_id
        .as_deref()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            AppError::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "PayPal webhook configuration unavailable",
            )
        })?;
    let webhook_event =
        serde_json::from_slice(raw).map_err(|_| invalid("Invalid PayPal webhook event"))?;
    Ok(VerifyWebhookSignatureRequest {
        auth_algo: webhook_header(headers, "paypal-auth-algo")?.into(),
        cert_url: webhook_header(headers, "paypal-cert-url")?.into(),
        transmission_id: webhook_header(headers, "paypal-transmission-id")?.into(),
        transmission_sig: webhook_header(headers, "paypal-transmission-sig")?.into(),
        transmission_time: webhook_header(headers, "paypal-transmission-time")?.into(),
        webhook_id: webhook_id.into(),
        webhook_event,
    })
}

fn webhook_header<'a>(headers: &'a HeaderMap, name: &'static str) -> Result<&'a str, AppError> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| invalid("Missing or invalid webhook authentication header"))
}

#[cfg(test)]
#[path = "../tests/validations/checkout.rs"]
mod tests;
