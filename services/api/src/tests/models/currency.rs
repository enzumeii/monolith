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

#[test]
fn convention_currency_expansion_preserves_known_and_unknown_identity() {
    for code in [
        "AED", "AFN", "ALL", "AMD", "ANG", "AOA", "ARS", "AWG", "AZN", "BAM", "BBD", "BDT", "BGN",
        "BIF", "BMD", "BND", "BOB", "BSD", "BWP", "BYN", "BZD", "CDF", "CLP", "COP", "CRC", "CVE",
        "DJF", "DOP", "DZD", "EGP", "ETB", "FJD", "FKP", "GEL", "GIP", "GMD", "GNF", "GTQ", "GYD",
        "HNL", "HTG", "IDR", "INR", "ISK", "JMD", "KES", "KGS", "KHR", "KMF", "KRW", "KYD", "KZT",
        "LAK", "LBP", "LKR", "LRD", "LSL", "MAD", "MDL", "MGA", "MKD", "MMK", "MNT", "MOP", "MUR",
        "MVR", "MWK", "MZN", "NAD", "NGN", "NIO", "NPR", "PAB", "PEN", "PGK", "PKR", "PYG", "QAR",
        "RON", "RSD", "RUB", "RWF", "SAR", "SBD", "SCR", "SHP", "SLE", "SOS", "SRD", "STD", "SZL",
        "TJS", "TOP", "TRY", "TTD", "TZS", "UAH", "UGX", "UYU", "UZS", "VND", "VUV", "WST", "XAF",
        "XCD", "XOF", "XPF", "YER", "ZAR", "ZMW",
    ] {
        let currency = CurrencyCode::from(code.to_ascii_lowercase());
        assert!(!matches!(currency, CurrencyCode::Other(_)));
        assert_eq!(currency.as_str(), code);
        assert_eq!(serde_json::to_value(currency).unwrap(), json!(code));
    }
    assert_ne!(
        CurrencyCode::from("ZZZ".to_owned()),
        CurrencyCode::from("ZZY".to_owned())
    );
}
