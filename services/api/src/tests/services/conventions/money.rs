#![allow(clippy::unwrap_used)]
use super::*;
use ConventionPaymentMethod::{Paypal, Promptpay, Stripe};
#[test]
fn exact_amounts_preserve_major_units_and_provider_precision() {
    assert_eq!(
        validate("12.99", "THB", &[Stripe, Paypal]).unwrap(),
        "12.99"
    );
    assert_eq!(validate("00012", "THB", &[Stripe]).unwrap(), "12.00");
    assert_eq!(validate("12.99", "HUF", &[Stripe]).unwrap(), "12.99");
    assert_eq!(validate("12", "HUF", &[Paypal]).unwrap(), "12.00");
    for code in ["HUF", "TWD"] {
        assert!(validate("12.99", code, &[Paypal]).is_err());
    }
    for code in ["JPY", "ISK", "UGX"] {
        assert_eq!(validate("12", code, &[Stripe]).unwrap(), "12");
        assert!(validate("12.00", code, &[Stripe]).is_err());
    }
    assert_eq!(validate("1.234", "KWD", &[Stripe]).unwrap(), "1.234");
    assert!(methods("SGD", &[Promptpay]).is_err());
    assert!(methods("KWD", &[Paypal]).is_err());
    for value in [
        "0",
        "-1",
        "+1",
        "1e2",
        "1.",
        ".1",
        " 1",
        "1 ",
        "NaN",
        "Infinity",
        "12.999",
        "18446744073709551616",
        "92233720368547758.08",
    ] {
        assert!(validate(value, "THB", &[Stripe]).is_err(), "{value}");
    }
    assert_eq!(
        validate("92233720368547758.07", "THB", &[Stripe]).unwrap(),
        "92233720368547758.07"
    );
}

#[test]
fn currency_enum_preserves_existing_precision_and_provider_rules() {
    for code in [
        "BIF", "CLP", "DJF", "GNF", "JPY", "KMF", "KRW", "MGA", "PYG", "RWF", "VND", "VUV", "XAF",
        "XOF", "XPF", "ISK", "UGX",
    ] {
        assert_eq!(precision(code).unwrap(), 0, "{code}");
    }
    for code in ["BHD", "JOD", "KWD", "OMR", "TND"] {
        assert_eq!(
            validate("12.345", code, &[Stripe]).unwrap(),
            "12.345",
            "{code}"
        );
    }
    for code in ["AED", "THB", "SGD", "HUF", "STD", "SLE", "ZMW"] {
        assert_eq!(
            validate("12.34", code, &[Stripe]).unwrap(),
            "12.34",
            "{code}"
        );
    }
    for code in [
        "AUD", "BRL", "CAD", "CNY", "CZK", "DKK", "EUR", "GBP", "HKD", "HUF", "ILS", "JPY", "MYR",
        "MXN", "NOK", "NZD", "PHP", "PLN", "SEK", "SGD", "CHF", "THB", "TWD", "USD",
    ] {
        assert!(methods(code, &[Paypal]).is_ok(), "{code}");
    }
    for code in ["IQD", "LYD", "ZZZ", "thb", ""] {
        assert!(methods(code, &[Stripe]).is_err(), "{code}");
    }
    assert!(methods("THB", &[]).is_err());
    assert!(methods("THB", &[Stripe, Stripe]).is_err());
    assert!(methods("THB", &[Promptpay]).is_ok());
    assert!(methods("AED", &[Paypal]).is_err());
}

#[test]
fn quote_minor_units_format_zero_and_preserve_all_supported_precision_classes() {
    for (code, input, units, canonical) in [
        ("JPY", "12", 12, "12"),
        ("THB", "12.99", 1299, "12.99"),
        ("KWD", "1.234", 1234, "1.234"),
        ("THB", "0001.2", 120, "1.20"),
    ] {
        assert_eq!(minor(input, code).unwrap(), units);
        assert_eq!(amount(units, code).unwrap(), canonical);
    }
    assert_eq!(amount(0, "JPY").unwrap(), "0");
    assert_eq!(amount(0, "THB").unwrap(), "0.00");
    assert_eq!(amount(0, "KWD").unwrap(), "0.000");
    assert!(amount(u64::MAX, "THB").is_err());
    assert!(minor("92233720368547758.08", "THB").is_err());
    assert!(amount(0, "XXX").is_err());
}
