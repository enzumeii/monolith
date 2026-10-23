use crate::error::AppError;
use axum::http::StatusCode;

pub(super) fn exact_money(amount: &str, currency: &str) -> Result<i64, AppError> {
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
    whole
        .checked_mul(multiplier)
        .and_then(|value| value.checked_add(fractional))
        .filter(|value| *value > 0)
        .ok_or_else(invalid_money)
}

pub(super) fn currency_scale(currency: &str) -> Option<usize> {
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
#[path = "../../tests/services/paypal_payments/money.rs"]
mod tests;
