use crate::error::AppError;
use axum::{
    body::Bytes,
    extract::rejection::BytesRejection,
    http::{HeaderMap, StatusCode, header::AUTHORIZATION},
};
use domain::dto::checkout::CreateCheckoutRequest;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use uuid::Uuid;

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
    let mut variants = HashSet::new();
    for item in &request.items {
        let variant = id(&item.variant_id)?;
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

#[cfg(test)]
#[path = "../tests/validations/checkout.rs"]
mod tests;
