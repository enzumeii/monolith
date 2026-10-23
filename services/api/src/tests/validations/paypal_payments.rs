use super::*;

#[test]
fn exact_money_preserves_large_values_and_currency_scales() {
    for (amount, currency, canonical, minor) in [
        ("1", "USD", "1.00", 100),
        ("1.2", "THB", "1.20", 120),
        (
            "90071992547409.91",
            "USD",
            "90071992547409.91",
            9_007_199_254_740_991,
        ),
        ("1", "JPY", "1", 1),
        ("42", "HUF", "42", 42),
        ("7", "TWD", "7", 7),
    ] {
        let money = exact_money(amount, currency).expect("exact amount");
        assert_eq!(money.amount, canonical);
        assert_eq!(money.currency, currency);
        assert_eq!(money.minor_units, minor);
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
        "99999999999999999999999999999999999999",
    ] {
        assert!(exact_money(value, "USD").is_err(), "{value}");
    }
    for currency in ["JPY", "TWD", "HUF"] {
        assert!(exact_money("1.00", currency).is_err());
        assert!(exact_money("1.01", currency).is_err());
    }
    for currency in ["KWD", "BHD", "XYZ", "usd", " USD"] {
        assert!(exact_money("1", currency).is_err());
    }
}

#[test]
fn money_checks_i64_minor_unit_arithmetic_boundaries() {
    let largest = exact_money("92233720368547758.07", "USD").expect("maximum minor units");
    assert_eq!(largest.amount, "92233720368547758.07");
    assert_eq!(largest.minor_units, i64::MAX);
    for (amount, currency) in [
        ("92233720368547758.08", "USD"),
        ("92233720368547759", "USD"),
        ("9223372036854775808", "JPY"),
    ] {
        assert!(
            exact_money(amount, currency).is_err(),
            "{amount} {currency}"
        );
    }
}

#[test]
fn create_requires_uuid_bounded_nonblank_reference_and_configured_currency() {
    let mut request = CreatePaypalPaymentRequest {
        idempotency_key: "a6af1d7a-b8e2-423b-93f7-30b1c69f030d".into(),
        reference: "opaque".into(),
        amount: "1.2".into(),
        currency: "USD".into(),
    };
    let allowed = vec!["USD".into()];
    let input = validate_create(&request, &allowed).expect("valid create");
    assert_eq!(input.amount, "1.20");
    assert_eq!(input.minor_units, 120);
    assert!(validate_create(&request, &[]).is_err());
    for reference in [
        String::new(),
        " ".into(),
        "x".repeat(128),
        "é".repeat(64),
        "a\nb".into(),
    ] {
        request.reference = reference;
        assert!(validate_create(&request, &allowed).is_err());
    }
    request.reference = "é".repeat(63);
    assert!(validate_create(&request, &allowed).is_ok());
    request.idempotency_key = "not-uuid".into();
    assert!(validate_create(&request, &allowed).is_err());
}

#[test]
fn capability_is_canonical_hashed_and_compared_as_fixed_size_digest() {
    let value = "A".repeat(43);
    let digest = capability_digest(&value).expect("32 zero byte canonical capability");
    assert_eq!(
        hex::encode(digest),
        "0f007385b6f9d4b7eeb2748605afe1a984a0a3bfa3f014d09e2a784ce9e5cd1a"
    );
    assert!(capability_matches(&digest, &digest));
    let mut changed = digest;
    changed[31] ^= 1;
    assert!(!capability_matches(&digest, &changed));
    assert!(!capability_matches(&digest[..31], &digest));
    for invalid in [
        "A".repeat(42),
        "A".repeat(44),
        format!("{}B", "A".repeat(42)),
        format!("{}=", "A".repeat(42)),
        format!("{}+", "A".repeat(42)),
        format!("{}/", "A".repeat(42)),
    ] {
        let error = capability_digest(&invalid).expect_err("invalid capability");
        assert!(!format!("{error:?} {error}").contains(&invalid));
    }
}
