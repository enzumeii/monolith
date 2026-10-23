use crate::{
    client::{ClientError, ClientErrorKind},
    models::CurrencyCode,
};

pub(super) fn exact_money(amount: &str, currency: &str) -> Result<i64, ClientError> {
    let scale = currency_scale(currency).ok_or_else(|| invalid_money())?;
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
        .ok_or_else(|| invalid_money())
}

pub(crate) fn currency_scale(currency: &str) -> Option<usize> {
    let code = CurrencyCode::from(currency.to_owned());
    if code.as_str() != currency {
        return None;
    }
    match code {
        CurrencyCode::HUF | CurrencyCode::JPY | CurrencyCode::TWD => Some(0),
        CurrencyCode::AUD
        | CurrencyCode::BRL
        | CurrencyCode::CAD
        | CurrencyCode::CNY
        | CurrencyCode::CZK
        | CurrencyCode::DKK
        | CurrencyCode::EUR
        | CurrencyCode::HKD
        | CurrencyCode::ILS
        | CurrencyCode::MYR
        | CurrencyCode::MXN
        | CurrencyCode::NZD
        | CurrencyCode::NOK
        | CurrencyCode::PHP
        | CurrencyCode::PLN
        | CurrencyCode::GBP
        | CurrencyCode::SGD
        | CurrencyCode::SEK
        | CurrencyCode::CHF
        | CurrencyCode::THB
        | CurrencyCode::USD => Some(2),
        CurrencyCode::BHD
        | CurrencyCode::IQD
        | CurrencyCode::JOD
        | CurrencyCode::KWD
        | CurrencyCode::LYD
        | CurrencyCode::OMR
        | CurrencyCode::TND
        | CurrencyCode::Other(_) => None,
    }
}

#[track_caller]
fn invalid_money() -> ClientError {
    ClientError::new(ClientErrorKind::Protocol(
        "Invalid payment amount or currency",
    ))
}

#[cfg(test)]
#[path = "../../../tests/client/paypal/payment/money.rs"]
mod tests;
