use crate::models::CurrencyCode;
use domain::dto::ConventionPaymentMethod;
#[derive(Debug, thiserror::Error)]
#[error("Invalid convention amount or currency/payment method combination")]
pub(crate) struct InvalidMoney;
#[expect(
    clippy::too_many_lines,
    reason = "Enumerates the exact accepted currency precision policy"
)]
pub(crate) fn precision(code: &str) -> Result<usize, InvalidMoney> {
    if code.len() != 3 || !code.bytes().all(|c| c.is_ascii_uppercase()) {
        return Err(InvalidMoney);
    }
    let currency = CurrencyCode::from(code.to_owned());
    match currency {
        CurrencyCode::BIF
        | CurrencyCode::CLP
        | CurrencyCode::DJF
        | CurrencyCode::GNF
        | CurrencyCode::JPY
        | CurrencyCode::KMF
        | CurrencyCode::KRW
        | CurrencyCode::MGA
        | CurrencyCode::PYG
        | CurrencyCode::RWF
        | CurrencyCode::VND
        | CurrencyCode::VUV
        | CurrencyCode::XAF
        | CurrencyCode::XOF
        | CurrencyCode::XPF
        | CurrencyCode::ISK
        | CurrencyCode::UGX => Ok(0),
        CurrencyCode::BHD
        | CurrencyCode::JOD
        | CurrencyCode::KWD
        | CurrencyCode::OMR
        | CurrencyCode::TND => Ok(3),
        CurrencyCode::AED
        | CurrencyCode::AFN
        | CurrencyCode::ALL
        | CurrencyCode::AMD
        | CurrencyCode::ANG
        | CurrencyCode::AOA
        | CurrencyCode::ARS
        | CurrencyCode::AUD
        | CurrencyCode::AWG
        | CurrencyCode::AZN
        | CurrencyCode::BAM
        | CurrencyCode::BBD
        | CurrencyCode::BDT
        | CurrencyCode::BGN
        | CurrencyCode::BMD
        | CurrencyCode::BND
        | CurrencyCode::BOB
        | CurrencyCode::BRL
        | CurrencyCode::BSD
        | CurrencyCode::BWP
        | CurrencyCode::BYN
        | CurrencyCode::BZD
        | CurrencyCode::CAD
        | CurrencyCode::CDF
        | CurrencyCode::CHF
        | CurrencyCode::CNY
        | CurrencyCode::COP
        | CurrencyCode::CRC
        | CurrencyCode::CVE
        | CurrencyCode::CZK
        | CurrencyCode::DKK
        | CurrencyCode::DOP
        | CurrencyCode::DZD
        | CurrencyCode::EGP
        | CurrencyCode::ETB
        | CurrencyCode::EUR
        | CurrencyCode::FJD
        | CurrencyCode::FKP
        | CurrencyCode::GBP
        | CurrencyCode::GEL
        | CurrencyCode::GIP
        | CurrencyCode::GMD
        | CurrencyCode::GTQ
        | CurrencyCode::GYD
        | CurrencyCode::HKD
        | CurrencyCode::HNL
        | CurrencyCode::HTG
        | CurrencyCode::HUF
        | CurrencyCode::IDR
        | CurrencyCode::ILS
        | CurrencyCode::INR
        | CurrencyCode::JMD
        | CurrencyCode::KES
        | CurrencyCode::KGS
        | CurrencyCode::KHR
        | CurrencyCode::KYD
        | CurrencyCode::KZT
        | CurrencyCode::LAK
        | CurrencyCode::LBP
        | CurrencyCode::LKR
        | CurrencyCode::LRD
        | CurrencyCode::LSL
        | CurrencyCode::MAD
        | CurrencyCode::MDL
        | CurrencyCode::MKD
        | CurrencyCode::MMK
        | CurrencyCode::MNT
        | CurrencyCode::MOP
        | CurrencyCode::MUR
        | CurrencyCode::MVR
        | CurrencyCode::MWK
        | CurrencyCode::MXN
        | CurrencyCode::MYR
        | CurrencyCode::MZN
        | CurrencyCode::NAD
        | CurrencyCode::NGN
        | CurrencyCode::NIO
        | CurrencyCode::NOK
        | CurrencyCode::NPR
        | CurrencyCode::NZD
        | CurrencyCode::PAB
        | CurrencyCode::PEN
        | CurrencyCode::PGK
        | CurrencyCode::PHP
        | CurrencyCode::PKR
        | CurrencyCode::PLN
        | CurrencyCode::QAR
        | CurrencyCode::RON
        | CurrencyCode::RSD
        | CurrencyCode::RUB
        | CurrencyCode::SAR
        | CurrencyCode::SBD
        | CurrencyCode::SCR
        | CurrencyCode::SEK
        | CurrencyCode::SGD
        | CurrencyCode::SHP
        | CurrencyCode::SLE
        | CurrencyCode::SOS
        | CurrencyCode::SRD
        | CurrencyCode::STD
        | CurrencyCode::SZL
        | CurrencyCode::THB
        | CurrencyCode::TJS
        | CurrencyCode::TOP
        | CurrencyCode::TRY
        | CurrencyCode::TTD
        | CurrencyCode::TWD
        | CurrencyCode::TZS
        | CurrencyCode::UAH
        | CurrencyCode::USD
        | CurrencyCode::UYU
        | CurrencyCode::UZS
        | CurrencyCode::WST
        | CurrencyCode::XCD
        | CurrencyCode::YER
        | CurrencyCode::ZAR
        | CurrencyCode::ZMW => Ok(2),
        _ => Err(InvalidMoney),
    }
}
pub(crate) fn methods(code: &str, methods: &[ConventionPaymentMethod]) -> Result<(), InvalidMoney> {
    precision(code)?;
    let currency = CurrencyCode::from(code.to_owned());
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
                if !matches!(
                    currency,
                    CurrencyCode::AUD
                        | CurrencyCode::BRL
                        | CurrencyCode::CAD
                        | CurrencyCode::CNY
                        | CurrencyCode::CZK
                        | CurrencyCode::DKK
                        | CurrencyCode::EUR
                        | CurrencyCode::GBP
                        | CurrencyCode::HKD
                        | CurrencyCode::HUF
                        | CurrencyCode::ILS
                        | CurrencyCode::JPY
                        | CurrencyCode::MYR
                        | CurrencyCode::MXN
                        | CurrencyCode::NOK
                        | CurrencyCode::NZD
                        | CurrencyCode::PHP
                        | CurrencyCode::PLN
                        | CurrencyCode::SEK
                        | CurrencyCode::SGD
                        | CurrencyCode::CHF
                        | CurrencyCode::THB
                        | CurrencyCode::TWD
                        | CurrencyCode::USD
                ) =>
            {
                return Err(InvalidMoney);
            }
            ConventionPaymentMethod::Promptpay if currency != CurrencyCode::THB => {
                return Err(InvalidMoney);
            }
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
    if matches!(
        CurrencyCode::from(code.to_owned()),
        CurrencyCode::HUF | CurrencyCode::TWD
    ) && payment_methods.contains(&ConventionPaymentMethod::Paypal)
        && canonical
            .split_once('.')
            .is_some_and(|(_, f)| f.bytes().any(|b| b != b'0'))
    {
        return Err(InvalidMoney);
    }
    Ok(canonical)
}

pub(crate) fn minor(value: &str, code: &str) -> Result<u64, InvalidMoney> {
    canonical(value, code)?
        .bytes()
        .filter(|byte| *byte != b'.')
        .try_fold(0_u64, |amount, byte| {
            amount
                .checked_mul(10)
                .and_then(|amount| amount.checked_add(u64::from(byte - b'0')))
                .ok_or(InvalidMoney)
        })
}

pub(crate) fn amount(minor: u64, code: &str) -> Result<String, InvalidMoney> {
    let precision = precision(code)?;
    if minor > i64::MAX.cast_unsigned() {
        return Err(InvalidMoney);
    }
    if precision == 0 {
        return Ok(minor.to_string());
    }
    let scale = 10_u64.pow(u32::try_from(precision).map_err(|_| InvalidMoney)?);
    Ok(format!("{}.{:0precision$}", minor / scale, minor % scale))
}

#[cfg(test)]
#[path = "../../tests/services/conventions/money.rs"]
mod tests;
