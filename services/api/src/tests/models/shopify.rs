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

#[test]
fn tagged_order_decodes_only_requested_fields_and_keeps_shop_currency() {
    let value = serde_json::json!({
        "id": "gid://shopify/Order/123", "number": 1001, "tags": ["shipping"], "email": null, "phone": null,
        "currentTotalPriceSet": {"shopMoney": {"amount": "1234567890.0123456789", "currencyCode": "THB"}},
        "lineItems": {"nodes": [{
            "title": "Shirt", "variantTitle": null, "sku": null, "quantity": 2,
            "originalUnitPriceSet": {"shopMoney": {"amount": "45.6700", "currencyCode": "THB"}}
        }]}
    });
    let order: TaggedOrder = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(order.number, 1001);
    assert!(order.email.is_none());
    assert!(order.phone.is_none());
    assert_eq!(
        order.current_total_price_set.shop_money.amount,
        "1234567890.0123456789"
    );
    assert_eq!(
        order.current_total_price_set.shop_money.currency_code,
        "THB"
    );
    let line = &order.line_items.nodes[0];
    assert!(line.variant_title.is_none());
    assert!(line.sku.is_none());
    assert_eq!(line.quantity, 2);
    assert_eq!(line.original_unit_price_set.shop_money.amount, "45.6700");
    assert_eq!(line.original_unit_price_set.shop_money.currency_code, "THB");
    assert_eq!(serde_json::to_value(&order).unwrap(), value);

    let mut empty = value;
    empty["lineItems"]["nodes"] = serde_json::json!([]);
    assert!(
        serde_json::from_value::<TaggedOrder>(empty)
            .unwrap()
            .line_items
            .nodes
            .is_empty()
    );
}
