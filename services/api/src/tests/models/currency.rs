#![allow(clippy::unwrap_used)]
use super::CurrencyCode;
use serde_json::json;

#[test]
fn known_and_unknown_currency_codes_preserve_wire_values() {
    let currency: CurrencyCode = serde_json::from_value(json!("AUD")).unwrap();
    assert_eq!(currency, CurrencyCode::AUD);
    let unknown: CurrencyCode = serde_json::from_value(json!("ZZZ")).unwrap();
    assert_eq!(unknown, CurrencyCode::Other("ZZZ".into()));
    assert_eq!(serde_json::to_value(unknown).unwrap(), json!("ZZZ"));
}

#[test]
fn known_currency_codes_round_trip() {
    for code in [
        "HUF", "JPY", "TWD", "BHD", "IQD", "JOD", "KWD", "LYD", "OMR", "TND", "AUD", "BRL", "CAD",
        "CHF", "CNY", "CZK", "DKK", "EUR", "GBP", "HKD", "ILS", "MYR", "MXN", "NOK", "NZD", "PHP",
        "PLN", "SEK", "SGD", "THB", "USD",
    ] {
        let currency: CurrencyCode = serde_json::from_value(json!(code)).unwrap();
        assert!(!matches!(currency, CurrencyCode::Other(_)));
        assert_eq!(currency.as_str(), code);
        assert_eq!(serde_json::to_value(currency).unwrap(), json!(code));
    }
}

#[test]
fn currency_codes_normalize_to_uppercase() {
    assert_eq!(CurrencyCode::from("aud".to_owned()), CurrencyCode::AUD);
    assert_eq!(
        serde_json::to_value(CurrencyCode::Other("zzz".into())).unwrap(),
        json!("ZZZ")
    );
    for (input, expected) in [("aud", "AUD"), ("uSd", "USD"), ("zzz", "ZZZ")] {
        let currency: CurrencyCode = serde_json::from_value(json!(input)).unwrap();
        assert_eq!(currency.as_str(), expected);
        assert_eq!(serde_json::to_value(currency).unwrap(), json!(expected));
    }
}
