use super::*;
use serde_json::json;

#[test]
fn shipping_weight_accepts_shopify_unsigned_integer_strings() {
    let order:ShippingOrder=serde_json::from_value(json!({"id":"gid://shopify/Order/1","email":null,"currencyCode":"THB","totalWeight":"2000","shippingAddress":null,"shippingLines":{"nodes":[]},"lineItems":{"nodes":[]}})).expect("valid fixture");
    assert_eq!(order.total_weight, Some(2000));
}
#[test]
fn resource_identity_rejects_wrong_kind_zero_and_overflow() {
    for id in [
        "gid://shopify/Order/0",
        "gid://shopify/Product/1",
        "gid://shopify/Order/-1",
        "gid://shopify/Order/18446744073709551616",
    ] {
        assert!(
            crate::models::shopify::ShopifyId::new(id)
                .and_then(|id| id.number("Order"))
                .is_err()
        );
    }
    assert_eq!(
        crate::models::shopify::ShopifyId::new("gid://shopify/Order/42")
            .and_then(|id| id.number("Order"))
            .expect("valid fixture"),
        42
    );
}
