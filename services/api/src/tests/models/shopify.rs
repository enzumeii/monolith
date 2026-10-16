#![allow(clippy::unwrap_used)]
use super::*;
#[test]
fn money_keeps_decimal_precision_and_currency_together() {
    let money: Money =
        serde_json::from_str(r#"{"amount":"1234567890.0123456789","currencyCode":"THB"}"#).unwrap();
    assert_eq!(money.amount, "1234567890.0123456789");
    assert_eq!(money.currency_code, "THB");
}
#[test]
fn resource_ids_reject_wrong_kind_and_malformed_values() {
    let id = ShopifyId::new("gid://shopify/Order/123").unwrap();
    assert!(id.require_kind("Location").is_err());
    for raw in ["123", "gid://shopify/Order/", "gid://shopify/Order/123:bad"] {
        assert!(ShopifyId::new(raw).is_err());
    }
}
