use crate::error::AppError;
use axum::http::StatusCode;

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct ExactMoney {
    pub minor: i64,
    pub currency: String,
    pub precision: u8,
}
impl ExactMoney {
    pub(crate) fn new(minor: i64, currency: &str, precision: u8) -> Result<Self, AppError> {
        let money = Self {
            minor,
            currency: currency.into(),
            precision,
        };
        money.validate()?;
        Ok(money)
    }
    pub(crate) fn validate(&self) -> Result<(), AppError> {
        if self.minor < 0
            || currency_precision(&self.currency) != Some(self.precision)
            || self.currency.len() != 3
            || !self.currency.bytes().all(|b| b.is_ascii_uppercase())
        {
            return Err(conflict("Invalid exact payment amount"));
        }
        Ok(())
    }
    pub(crate) fn parse_amount(&self, value: &str) -> Result<i64, AppError> {
        self.validate()?;
        let (whole, fraction) = value.split_once('.').unwrap_or((value, ""));
        if whole.is_empty()
            || !whole.bytes().all(|b| b.is_ascii_digit())
            || !fraction.bytes().all(|b| b.is_ascii_digit())
            || value.ends_with('.')
        {
            return Err(conflict("Invalid exact payment amount"));
        }
        let places = usize::from(self.precision);
        if fraction
            .get(places..)
            .is_some_and(|tail| tail.bytes().any(|b| b != b'0'))
        {
            return Err(conflict("Payment precision mismatch"));
        }
        let scale = 10_i64.pow(u32::from(self.precision));
        let whole = whole
            .parse::<i64>()
            .ok()
            .and_then(|v| v.checked_mul(scale))
            .ok_or_else(|| conflict("Payment amount overflow"))?;
        let digits = &fraction[..fraction.len().min(places)];
        let fraction = if digits.is_empty() {
            0
        } else {
            digits
                .parse::<i64>()
                .map_err(|_| conflict("Invalid exact payment amount"))?
        };
        let padding =
            u32::try_from(places - digits.len()).map_err(|_| conflict("Invalid precision"))?;
        whole
            .checked_add(fraction * 10_i64.pow(padding))
            .ok_or_else(|| conflict("Payment amount overflow"))
    }
}
pub(crate) fn currency_precision(currency: &str) -> Option<u8> {
    use crate::models::CurrencyCode;
    match CurrencyCode::from(currency.to_owned()) {
        CurrencyCode::JPY => Some(0),
        CurrencyCode::BHD
        | CurrencyCode::IQD
        | CurrencyCode::JOD
        | CurrencyCode::KWD
        | CurrencyCode::LYD
        | CurrencyCode::OMR
        | CurrencyCode::TND => Some(3),
        CurrencyCode::HUF
        | CurrencyCode::TWD
        | CurrencyCode::AUD
        | CurrencyCode::BRL
        | CurrencyCode::CAD
        | CurrencyCode::CHF
        | CurrencyCode::CNY
        | CurrencyCode::CZK
        | CurrencyCode::DKK
        | CurrencyCode::EUR
        | CurrencyCode::GBP
        | CurrencyCode::HKD
        | CurrencyCode::ILS
        | CurrencyCode::MYR
        | CurrencyCode::MXN
        | CurrencyCode::NOK
        | CurrencyCode::NZD
        | CurrencyCode::PHP
        | CurrencyCode::PLN
        | CurrencyCode::SEK
        | CurrencyCode::SGD
        | CurrencyCode::THB
        | CurrencyCode::USD => Some(2),
        _ => None,
    }
}
pub(crate) fn conflict(message: &'static str) -> AppError {
    AppError::new(StatusCode::CONFLICT, message)
}
#[derive(Debug, thiserror::Error)]
#[error("Shopify completion evidence conflicts")]
pub(crate) struct CompletionEvidenceConflict;

pub(crate) fn evidence_conflict(message: &'static str) -> AppError {
    conflict(message).with_source(CompletionEvidenceConflict)
}
#[cfg(test)]
#[path = "../tests/models/checkout_completion.rs"]
mod tests;
