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
fn unknown_payment_protocols_fail_instead_of_defaulting() {
    assert!(serde_json::from_str::<Provider>("\"OTHER\"").is_err());
    assert!(serde_json::from_str::<PaymentEnvironment>("\"sandbox\"").is_err());
    assert_eq!(
        serde_json::to_string(&PaymentEnvironment::Test).unwrap(),
        "\"TEST\""
    );
}
