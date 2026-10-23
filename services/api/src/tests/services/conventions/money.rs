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
