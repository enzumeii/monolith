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
