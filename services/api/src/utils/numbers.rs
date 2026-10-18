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
