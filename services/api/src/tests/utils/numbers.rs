#![allow(clippy::unwrap_used)]
use super::*;

#[test]
fn numbers_parse_prices_and_weights() {
    for (raw, expected) in [("19.99", 19.99), ("0.01", 0.01), ("1.25E+2", 125.0)] {
        assert_eq!(parse_number(raw).unwrap().as_f64(), Some(expected));
    }
    for raw in ["invalid-price", "NaN", "inf", "-inf"] {
        assert!(parse_number(raw).is_err(), "{raw}");
    }
}

#[test]
fn totals_use_two_decimal_places_for_inventory_quantities() {
    for (raw, quantity, expected) in [
        ("19.99", 1000, 19990.0),
        ("0.29", 3, 0.87),
        ("0.01", -5, -0.05),
        ("2.55", 0, 0.0),
        ("0", 1000, 0.0),
    ] {
        let result = multiply_number(&parse_number(raw).unwrap(), quantity).unwrap();
        assert_eq!(result.as_f64(), Some(expected));
    }
}

#[test]
fn invalid_numbers_keep_the_utility_origin() {
    let error = parse_number("invalid-price").unwrap_err();
    assert!(matches!(error.kind(), ClientErrorKind::Protocol(_)));
    assert_eq!(error.location().file(), "services/api/src/utils/numbers.rs");
    assert!(error.location().line() > 0);
}

#[test]
fn reusable_float_conversions_preserve_units() {
    assert_eq!(super::integer_as_f64(-125), -125.0);
    assert_eq!(super::minor_units(-125), -1.25);
    assert_eq!(super::parse_f64("1.25").unwrap(), 1.25);
    assert!(super::parse_f64("NaN").is_err());
    assert!(super::parse_f64("1e999").is_err());
    assert!(super::integer_as_f64(i64::MAX).is_finite());
}
