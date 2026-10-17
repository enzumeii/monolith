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

#[test]
fn requests_own_validation_and_preserve_literal_values() {
    let location = ShopifyId::new("gid://shopify/Location/123").unwrap();
    let order = ShopifyId::new("gid://shopify/Order/456").unwrap();
    let request = ListProductsRequest {
        location_id: location.clone(),
        statuses: vec![ProductStatus::Active, ProductStatus::Draft],
    };
    let variables = request.variables(Some("next")).unwrap();
    assert_eq!(variables["search"], "status:active,draft");
    assert_eq!(variables["after"], "next");
    assert!(
        ListProductsRequest {
            location_id: order.clone(),
            statuses: vec![]
        }
        .variables(None)
        .is_err()
    );
    assert!(
        InventoryDiscoveryRequest {
            location_id: order.clone()
        }
        .variables(None)
        .is_err()
    );
    assert!(
        ReplaceOrderTagsRequest {
            order_id: location.clone(),
            tags: vec![]
        }
        .variables()
        .is_err()
    );
    assert_eq!(
        ReplaceOrderTagsRequest {
            order_id: order.clone(),
            tags: vec![" shipping ".into()]
        }
        .variables()
        .unwrap()["input"]["tags"],
        json!([" shipping "])
    );
    assert_eq!(
        OrdersByTagRequest {
            tag: " shipping ".into()
        }
        .variables(None)
        .unwrap()["search"],
        "tag:\" shipping \""
    );
    assert_eq!(
        OrdersByPaymentRequest {
            payment_id: " a:b ".into()
        }
        .variables(None)
        .unwrap()["search"],
        "payment_id:\" a\\:b \""
    );
    for blank in ["", " \t "] {
        assert!(
            OrdersByTagRequest { tag: blank.into() }
                .variables(None)
                .is_err()
        );
        assert!(
            OrdersByPaymentRequest {
                payment_id: blank.into()
            }
            .variables(None)
            .is_err()
        );
    }
    let time = DateTime::parse_from_rfc3339("2026-01-01T00:00:00+07:00").unwrap();
    assert!(
        SalesRangeRequest {
            start: time,
            end: time
        }
        .variables(None)
        .is_err()
    );
}

#[test]
fn activation_request_validates_resource_kinds_and_never_sets_quantities() {
    let location = ShopifyId::new("gid://shopify/Location/123").unwrap();
    let order = ShopifyId::new("gid://shopify/Order/456").unwrap();
    let item = ShopifyId::new("gid://shopify/InventoryItem/789").unwrap();
    let key = Uuid::new_v4();
    assert!(
        ActivateInventoryRequest {
            inventory_item_id: order.clone(),
            location_id: location.clone(),
            idempotency_key: key
        }
        .variables()
        .is_err()
    );
    assert!(
        ActivateInventoryRequest {
            inventory_item_id: item.clone(),
            location_id: order,
            idempotency_key: key
        }
        .variables()
        .is_err()
    );
    let variables = ActivateInventoryRequest {
        inventory_item_id: item,
        location_id: location,
        idempotency_key: key,
    }
    .variables()
    .unwrap();
    assert_eq!(variables["idempotencyKey"], key.to_string());
    assert!(variables.get("available").is_none());
    assert!(variables.get("onHand").is_none());
}
