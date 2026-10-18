use crate::client::{ClientError, ClientErrorKind};
use rust_decimal::Decimal;
use serde_json::Number;
use std::str::FromStr;

pub(crate) fn parse_decimal(raw: &str) -> Result<Number, ClientError> {
    let value = if let Some((base, _)) = raw.split_once(['e', 'E']) {
        Decimal::from_str_exact(base).map_err(|_| protocol("invalid report decimal"))?;
        Decimal::from_scientific(raw)
    } else {
        Decimal::from_str_exact(raw)
    }
    .map_err(|_| protocol("invalid report decimal"))?;
    Number::from_str(&value.normalize().to_string()).map_err(|_| protocol("invalid JSON decimal"))
}
pub(crate) fn multiply_decimal(value: &Number, quantity: i64) -> Result<Number, ClientError> {
    let value = Decimal::from_str_exact(&value.to_string())
        .map_err(|_| protocol("invalid report decimal"))?
        .normalize();
    let mut mantissa = value.mantissa();
    let mut multiplier = i128::from(quantity);
    let mut scale = value.scale();
    if mantissa == 0 || multiplier == 0 {
        return Ok(Number::from(0));
    }
    // Cancel decimal places before multiplying so a temporary coefficient cannot overflow an exact result.
    while scale > 0 {
        let factors = [(10, 1), (1, 10), (2, 5), (5, 2)]
            .into_iter()
            .find(|(a, b)| mantissa % a == 0 && multiplier % b == 0);
        let Some((a, b)) = factors else {
            break;
        };
        mantissa /= a;
        multiplier /= b;
        scale -= 1;
    }
    let mantissa = mantissa
        .checked_mul(multiplier)
        .ok_or_else(|| protocol("report total overflow"))?;
    let result = Decimal::try_from_i128_with_scale(mantissa, scale)
        .map_err(|_| protocol("report total overflow"))?;
    parse_decimal(&result.to_string())
}

#[track_caller]
fn protocol(message: &'static str) -> ClientError {
    ClientError::new(ClientErrorKind::Protocol(message))
}

#[cfg(test)]
#[path = "../tests/utils/numbers.rs"]
mod tests;
