use crate::models::CurrencyCode;
use domain::dto::ConventionPaymentMethod;
#[derive(Debug, thiserror::Error)]
#[error("Invalid convention amount or currency/payment method combination")]
pub(crate) struct InvalidMoney;
pub(crate) fn precision(code: &str) -> Result<usize, InvalidMoney> {
    if code.len() != 3 || !code.bytes().all(|c| c.is_ascii_uppercase()) {
        return Err(InvalidMoney);
    }
    let currency = CurrencyCode::from(code.to_owned());
    match currency.as_str() {
        "BIF" | "CLP" | "DJF" | "GNF" | "JPY" | "KMF" | "KRW" | "MGA" | "PYG" | "RWF" | "VND"
        | "VUV" | "XAF" | "XOF" | "XPF" | "ISK" | "UGX" => Ok(0),
        "BHD" | "JOD" | "KWD" | "OMR" | "TND" => Ok(3),
        "AED" | "AFN" | "ALL" | "AMD" | "ANG" | "AOA" | "ARS" | "AUD" | "AWG" | "AZN" | "BAM"
        | "BBD" | "BDT" | "BGN" | "BMD" | "BND" | "BOB" | "BRL" | "BSD" | "BWP" | "BYN" | "BZD"
        | "CAD" | "CDF" | "CHF" | "CNY" | "COP" | "CRC" | "CVE" | "CZK" | "DKK" | "DOP" | "DZD"
        | "EGP" | "ETB" | "EUR" | "FJD" | "FKP" | "GBP" | "GEL" | "GIP" | "GMD" | "GTQ" | "GYD"
        | "HKD" | "HNL" | "HTG" | "HUF" | "IDR" | "ILS" | "INR" | "JMD" | "KES" | "KGS" | "KHR"
        | "KYD" | "KZT" | "LAK" | "LBP" | "LKR" | "LRD" | "LSL" | "MAD" | "MDL" | "MKD" | "MMK"
        | "MNT" | "MOP" | "MUR" | "MVR" | "MWK" | "MXN" | "MYR" | "MZN" | "NAD" | "NGN" | "NIO"
        | "NOK" | "NPR" | "NZD" | "PAB" | "PEN" | "PGK" | "PHP" | "PKR" | "PLN" | "QAR" | "RON"
        | "RSD" | "RUB" | "SAR" | "SBD" | "SCR" | "SEK" | "SGD" | "SHP" | "SLE" | "SOS" | "SRD"
        | "STD" | "SZL" | "THB" | "TJS" | "TOP" | "TRY" | "TTD" | "TWD" | "TZS" | "UAH" | "USD"
        | "UYU" | "UZS" | "WST" | "XCD" | "YER" | "ZAR" | "ZMW" => Ok(2),
        _ => Err(InvalidMoney),
    }
}
pub(crate) fn methods(code: &str, methods: &[ConventionPaymentMethod]) -> Result<(), InvalidMoney> {
    precision(code)?;
    if methods.is_empty()
        || methods
            .iter()
            .enumerate()
            .any(|(i, m)| methods[..i].contains(m))
    {
        return Err(InvalidMoney);
    }
    for method in methods {
        match method {
            ConventionPaymentMethod::Paypal
                if !PAYPAL_CURRENCIES
                    .split_whitespace()
                    .any(|currency| currency == code) =>
            {
                return Err(InvalidMoney);
            }
            ConventionPaymentMethod::Promptpay if code != "THB" => return Err(InvalidMoney),
            _ => {}
        }
    }
    Ok(())
}
pub(crate) fn canonical(value: &str, code: &str) -> Result<String, InvalidMoney> {
    let precision = precision(code)?;
    let (whole, fraction) = value.split_once('.').map_or((value, ""), |(w, f)| (w, f));
    if whole.is_empty()
        || !whole.bytes().all(|c| c.is_ascii_digit())
        || value.len() > 24
        || fraction.len() > precision
        || (value.contains('.') && fraction.is_empty())
        || !fraction.bytes().all(|c| c.is_ascii_digit())
    {
        return Err(InvalidMoney);
    }
    let mut minor = 0_u64;
    for c in whole
        .bytes()
        .chain(fraction.bytes())
        .chain(std::iter::repeat_n(b'0', precision - fraction.len()))
    {
        minor = minor
            .checked_mul(10)
            .and_then(|n| n.checked_add(u64::from(c - b'0')))
            .ok_or(InvalidMoney)?;
    }
    if minor == 0 || minor > i64::MAX.cast_unsigned() {
        return Err(InvalidMoney);
    }
    let whole = whole.trim_start_matches('0');
    let whole = if whole.is_empty() { "0" } else { whole };
    if precision == 0 {
        Ok(whole.to_owned())
    } else {
        Ok(format!(
            "{whole}.{fraction}{}",
            "0".repeat(precision - fraction.len())
        ))
    }
}
pub(crate) fn validate(
    value: &str,
    code: &str,
    payment_methods: &[ConventionPaymentMethod],
) -> Result<String, InvalidMoney> {
    methods(code, payment_methods)?;
    let canonical = canonical(value, code)?;
    if matches!(code, "HUF" | "TWD")
        && payment_methods.contains(&ConventionPaymentMethod::Paypal)
        && canonical
            .split_once('.')
            .is_some_and(|(_, f)| f.bytes().any(|b| b != b'0'))
    {
        return Err(InvalidMoney);
    }
    Ok(canonical)
}
const PAYPAL_CURRENCIES: &str = "AUD BRL CAD CNY CZK DKK EUR GBP HKD HUF ILS JPY MYR MXN NOK NZD PHP PLN SEK SGD CHF THB TWD USD";

#[cfg(test)]
#[path = "../../tests/services/conventions/money.rs"]
mod tests;
