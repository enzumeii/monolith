#![allow(clippy::unwrap_used)]
use super::*;

#[test]
fn decimal_parsing_preserves_scientific_notation_and_rejects_precision_loss() {
    for (raw, expected) in [
        ("1e-7", "0.0000001"),
        ("1e7", "10000000"),
        ("1.25E+2", "125"),
        ("1.00", "1"),
    ] {
        assert_eq!(parse_decimal(raw).unwrap().to_string(), expected);
    }
    for raw in [
        "NaN",
        "1e-29",
        "0.00000000000000000000000000001",
        "0.00000000116415321826934814453125",
    ] {
        assert!(parse_decimal(raw).is_err(), "{raw}");
    }
}

#[test]
fn multiplication_preserves_exact_scaled_totals_and_rejects_overflow() {
    for (raw, quantity, expected) in [
        ("0.1", 3, "0.3"),
        ("0.1", -2, "-0.2"),
        ("0.1", 0, "0"),
        ("0", i64::MAX, "0"),
        (
            "7922816251426433759354395033.5",
            10,
            "79228162514264337593543950335",
        ),
        (
            "0.0000000037252902984619140625",
            4_611_686_018_427_387_904,
            "17179869184",
        ),
        (
            "1234567890123456789012345678.1",
            30,
            "37037036703703703670370370343",
        ),
    ] {
        let result = multiply_decimal(&parse_decimal(raw).unwrap(), quantity).unwrap();
        assert_eq!(result.to_string(), expected);
    }
    let maximum = parse_decimal("79228162514264337593543950335").unwrap();
    assert!(multiply_decimal(&maximum, i64::MAX).is_err());
}

#[test]
fn invalid_decimals_keep_the_utility_origin() {
    let error = parse_decimal("invalid-price").unwrap_err();
    assert!(matches!(error.kind(), ClientErrorKind::Protocol(_)));
    assert_eq!(error.location().file(), "services/api/src/utils/numbers.rs");
    assert!(error.location().line() > 0);
}
