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

#[test]
fn currency_enum_policy_preserves_every_supported_precision() {
    for currency in ["HUF", "JPY", "TWD"] {
        assert_eq!(currency_scale(currency), Some(0), "{currency}");
        assert_eq!(exact_money("1", currency).expect("whole amount"), 1);
    }
    for currency in [
        "AUD", "BRL", "CAD", "CNY", "CZK", "DKK", "EUR", "HKD", "ILS", "MYR", "MXN", "NZD", "NOK",
        "PHP", "PLN", "GBP", "SGD", "SEK", "CHF", "THB", "USD",
    ] {
        assert_eq!(currency_scale(currency), Some(2), "{currency}");
        assert_eq!(
            exact_money("1.01", currency).expect("fractional amount"),
            101
        );
    }
}

#[test]
fn unsupported_unknown_and_noncanonical_currencies_remain_protocol_errors() {
    for currency in [
        "BHD", "IQD", "JOD", "KWD", "LYD", "OMR", "TND", "XYZ", "ABC", "usd", "uSd", "jpy", "JpY",
        " USD", "USD ", "USD\n", "", "€",
    ] {
        assert_eq!(currency_scale(currency), None, "{currency:?}");
        let error = exact_money("1", currency).expect_err("unsupported currency");
        assert!(matches!(
            error.kind(),
            ClientErrorKind::Protocol("Invalid payment amount or currency")
        ));
        assert!(
            error
                .location()
                .file()
                .ends_with("client/paypal/payment/money.rs")
        );
    }
}
