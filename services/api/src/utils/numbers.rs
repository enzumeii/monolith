use crate::client::{ClientError, ClientErrorKind};
use serde_json::Number;

pub(crate) fn parse_number(raw: &str) -> Result<Number, ClientError> {
    let value = raw.parse::<f64>().map_err(|_| invalid_number())?;
    number(value)
}

pub(crate) fn multiply_number(value: &Number, quantity: i64) -> Result<Number, ClientError> {
    let value = value.as_f64().ok_or_else(|| invalid_number())?;
    let quantity = Number::from(quantity)
        .as_f64()
        .ok_or_else(|| invalid_number())?;
    number((value * quantity * 100.0).round() / 100.0)
}

pub(crate) fn parse_f64(value: &str) -> Result<f64, ClientError> {
    parse_number(value)?
        .as_f64()
        .ok_or_else(|| invalid_number())
}

pub(crate) fn integer_as_f64(value: i64) -> f64 {
    Number::from(value).as_f64().unwrap_or_default()
}

pub(crate) fn minor_units(value: i64) -> f64 {
    integer_as_f64(value) / 100.0
}

fn number(value: f64) -> Result<Number, ClientError> {
    value.to_string().parse().map_err(|_| invalid_number())
}

#[track_caller]
fn invalid_number() -> ClientError {
    ClientError::new(ClientErrorKind::Protocol("invalid report number"))
}

#[cfg(test)]
#[path = "../tests/utils/numbers.rs"]
mod tests;
