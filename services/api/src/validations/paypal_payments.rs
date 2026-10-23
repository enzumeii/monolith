use crate::error::AppError;
use axum::http::StatusCode;
use domain::dto::CreatePaypalPaymentRequest;
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ExactMoney {
    pub amount: String,
    pub currency: String,
    pub minor_units: i64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ValidatedPaymentInput {
    pub idempotency_key: Uuid,
    pub reference: String,
    pub amount: String,
    pub currency: String,
    pub minor_units: i64,
}
pub(crate) fn exact_money(amount: &str, currency: &str) -> Result<ExactMoney, AppError> {
    let scale = currency_scale(currency).ok_or_else(invalid_money)?;
    if amount.is_empty() || amount.len() > 32 {
        return Err(invalid_money());
    }
    let (whole, fraction) = amount
        .split_once('.')
        .map_or((amount, None), |(whole, fraction)| (whole, Some(fraction)));
    if whole.is_empty()
        || (whole.len() > 1 && whole.starts_with('0'))
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(invalid_money());
    }
    if let Some(fraction) = fraction
        && (scale == 0
            || fraction.is_empty()
            || fraction.len() > scale
            || !fraction.bytes().all(|byte| byte.is_ascii_digit()))
    {
        return Err(invalid_money());
    }
    let whole: i64 = whole.parse().map_err(|_| invalid_money())?;
    let multiplier = if scale == 0 { 1 } else { 100 };
    let fractional = match fraction {
        Some(value) => {
            value.parse::<i64>().map_err(|_| invalid_money())?
                * if value.len() == 1 { 10 } else { 1 }
        }
        None => 0,
    };
    let minor_units = whole
        .checked_mul(multiplier)
        .and_then(|value| value.checked_add(fractional))
        .filter(|value| *value > 0)
        .ok_or_else(invalid_money)?;
    let amount = if scale == 0 {
        whole.to_string()
    } else {
        format!("{whole}.{fractional:02}")
    };
    Ok(ExactMoney {
        amount,
        currency: currency.into(),
        minor_units,
    })
}

pub(crate) fn validate_create(
    request: &CreatePaypalPaymentRequest,
    allowed: &[String],
) -> Result<ValidatedPaymentInput, AppError> {
    let idempotency_key = Uuid::parse_str(&request.idempotency_key)
        .map_err(|_| AppError::new(StatusCode::BAD_REQUEST, "Invalid payment idempotency key"))?;
    if request.reference.trim().is_empty()
        || request.reference.len() > 127
        || request.reference.chars().any(char::is_control)
    {
        return Err(AppError::new(
            StatusCode::BAD_REQUEST,
            "Invalid payment reference",
        ));
    }
    if !allowed.contains(&request.currency) {
        return Err(AppError::new(
            StatusCode::BAD_REQUEST,
            "Unsupported payment currency",
        ));
    }
    let money = exact_money(&request.amount, &request.currency)?;
    Ok(ValidatedPaymentInput {
        idempotency_key,
        reference: request.reference.clone(),
        amount: money.amount,
        currency: money.currency,
        minor_units: money.minor_units,
    })
}

pub(crate) fn capability_digest(value: &str) -> Result<[u8; 32], AppError> {
    let bytes = value.as_bytes();
    if bytes.len() != 43
        || !bytes
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        || !b"AEIMQUYcgkosw048".contains(&bytes[42])
    {
        return Err(AppError::new(
            StatusCode::BAD_REQUEST,
            "Invalid payment capability",
        ));
    }
    Ok(Sha256::digest(bytes).into())
}

pub(crate) fn capability_matches(stored: &[u8], candidate: &[u8]) -> bool {
    stored.len() == 32 && candidate.len() == 32 && bool::from(stored.ct_eq(candidate))
}

pub(crate) fn currency_scale(currency: &str) -> Option<usize> {
    match currency {
        "HUF" | "JPY" | "TWD" => Some(0),
        "AUD" | "BRL" | "CAD" | "CNY" | "CZK" | "DKK" | "EUR" | "HKD" | "ILS" | "MYR" | "MXN"
        | "NZD" | "NOK" | "PHP" | "PLN" | "GBP" | "SGD" | "SEK" | "CHF" | "THB" | "USD" => Some(2),
        _ => None,
    }
}

fn invalid_money() -> AppError {
    AppError::new(
        StatusCode::BAD_REQUEST,
        "Invalid payment amount or currency",
    )
}

#[cfg(test)]
#[path = "../tests/validations/paypal_payments.rs"]
mod tests;
