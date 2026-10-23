use super::*;

#[test]
fn exact_money_preserves_integer_precision_and_supported_currency_scales() {
    for (amount, currency, minor) in [
        ("1", "USD", 100),
        ("1.2", "THB", 120),
        ("90071992547409.91", "USD", 9_007_199_254_740_991),
        ("1", "JPY", 1),
        ("42", "HUF", 42),
        ("7", "TWD", 7),
        ("92233720368547758.07", "USD", i64::MAX),
    ] {
        assert_eq!(exact_money(amount, currency).expect("exact amount"), minor);
    }
}

#[test]
fn money_rejects_ambiguous_zero_fractional_overflow_and_unsupported_input() {
    for value in [
        "",
        "0",
        "0.00",
        "-1",
        "+1",
        " 1",
        "1 ",
        "1e2",
        ".5",
        "1.",
        "01.00",
        "1.001",
        "NaN",
        "1,000",
        "92233720368547758.08",
        "92233720368547759",
        "99999999999999999999999999999999999999",
    ] {
        assert!(exact_money(value, "USD").is_err(), "{value}");
    }
    for currency in ["JPY", "TWD", "HUF"] {
        assert!(exact_money("1.00", currency).is_err());
        assert!(exact_money("1.01", currency).is_err());
        assert!(exact_money("9223372036854775808", currency).is_err());
    }
    for currency in ["KWD", "BHD", "XYZ", "usd", " USD"] {
        assert!(exact_money("1", currency).is_err());
    }
}
