#![allow(clippy::unwrap_used)]
use super::*;
#[test]
fn money_is_exact_and_rejects_lossy_or_overflow_input() {
    let amount = ExactMoney::new(12345, "THB", 2).unwrap();
    assert_eq!(amount.parse_amount("123.4500").unwrap(), 12345);
    for bad in [
        "123.451",
        "NaN",
        "1e2",
        "-1",
        "+1",
        " 1",
        "92233720368547758.08",
    ] {
        assert!(amount.parse_amount(bad).is_err(), "{bad}");
    }
    assert!(ExactMoney::new(4000, "USD", 3).is_err());
    assert!(ExactMoney::new(1, "ZZZ", 2).is_err());
    assert!(ExactMoney::new(-1, "THB", 2).is_err());
    assert!(ExactMoney::new(1, "thb", 2).is_err());
    assert!(ExactMoney::new(1, "THB", 9).is_err());
}
#[test]
fn zero_and_three_decimal_currencies_preserve_exact_minor_units() {
    let yen = ExactMoney::new(123, "JPY", 0).unwrap();
    assert_eq!(yen.parse_amount("123.000").unwrap(), 123);
    assert!(yen.parse_amount("123.1").is_err());
    let dinar = ExactMoney::new(1234, "KWD", 3).unwrap();
    assert_eq!(dinar.parse_amount("1.2340").unwrap(), 1234);
    assert_eq!(dinar.parse_amount("1.2").unwrap(), 1200);
    assert!(dinar.parse_amount("1.2341").is_err());
}

#[test]
fn newly_named_payment_currencies_remain_unsupported() {
    let mut accepted = Vec::new();
    for currency in ["KRW", "VND", "AED", "INR"] {
        for precision in [0, 2] {
            if ExactMoney::new(123, currency, precision).is_ok() {
                accepted.push((currency, precision));
            }
        }
    }
    assert!(
        accepted.is_empty(),
        "Unsupported payment currencies accepted: {accepted:?}"
    );
}

#[test]
fn supported_payment_currencies_preserve_exact_precision() {
    for (currency, precision, value) in [
        ("HUF", 2, "1.23"),
        ("TWD", 2, "1.23"),
        ("AUD", 2, "1.23"),
        ("BRL", 2, "1.23"),
        ("CAD", 2, "1.23"),
        ("CHF", 2, "1.23"),
        ("CNY", 2, "1.23"),
        ("CZK", 2, "1.23"),
        ("DKK", 2, "1.23"),
        ("EUR", 2, "1.23"),
        ("GBP", 2, "1.23"),
        ("HKD", 2, "1.23"),
        ("ILS", 2, "1.23"),
        ("MYR", 2, "1.23"),
        ("MXN", 2, "1.23"),
        ("NOK", 2, "1.23"),
        ("NZD", 2, "1.23"),
        ("PHP", 2, "1.23"),
        ("PLN", 2, "1.23"),
        ("SEK", 2, "1.23"),
        ("SGD", 2, "1.23"),
        ("THB", 2, "1.23"),
        ("USD", 2, "1.23"),
        ("JPY", 0, "123"),
        ("BHD", 3, "0.123"),
        ("IQD", 3, "0.123"),
        ("JOD", 3, "0.123"),
        ("KWD", 3, "0.123"),
        ("LYD", 3, "0.123"),
        ("OMR", 3, "0.123"),
        ("TND", 3, "0.123"),
    ] {
        let money = ExactMoney::new(123, currency, precision).unwrap();
        assert_eq!(money.parse_amount(value).unwrap(), 123, "{currency}");
    }
}
