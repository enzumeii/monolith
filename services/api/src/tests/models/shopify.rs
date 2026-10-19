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
        "lineItems": {"pageInfo":{"hasNextPage":false,"endCursor":null},"nodes": [{
            "title": "Shirt", "variantTitle": null, "sku": null, "quantity": 2, "product": null, "variant": null,
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
    let variables = request
        .variables(&PageRequest {
            first: 7,
            after: Some("next".into()),
        })
        .unwrap();
    assert_eq!(variables["search"], "status:active,draft");
    assert_eq!(variables["after"], "next");
    assert!(
        ListProductsRequest {
            location_id: order.clone(),
            statuses: vec![],
        }
        .variables(&PageRequest {
            first: 10,
            after: None
        })
        .is_err()
    );
    assert!(
        InventoryDiscoveryRequest {
            location_id: order.clone()
        }
        .variables(&PageRequest {
            first: 10,
            after: None
        })
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
}

#[test]
fn search_requests_preserve_literals_and_validate_ranges() {
    assert_eq!(
        OrdersByTagRequest {
            tags: vec![" shipping ".into()]
        }
        .variables(&PageRequest {
            first: 10,
            after: None
        })
        .unwrap()["search"],
        "tag:\" shipping \""
    );
    assert_eq!(
        OrdersByPaymentRequest {
            payment_id: " a:b ".into()
        }
        .variables(&PageRequest {
            first: 10,
            after: None
        })
        .unwrap()["search"],
        "payment_id:\" a\\:b \""
    );
    for blank in ["", " \t "] {
        assert!(
            OrdersByTagRequest {
                tags: vec![blank.into()]
            }
            .variables(&PageRequest {
                first: 10,
                after: None
            })
            .is_err()
        );
        assert!(
            OrdersByPaymentRequest {
                payment_id: blank.into()
            }
            .variables(&PageRequest {
                first: 10,
                after: None
            })
            .is_err()
        );
    }
    let time = DateTime::parse_from_rfc3339("2026-01-01T00:00:00+07:00").unwrap();
    assert!(
        SalesRangeRequest {
            start: time,
            end: time
        }
        .variables(&PageRequest {
            first: 10,
            after: None
        })
        .is_err()
    );
}

#[test]
fn activation_request_validates_resource_kinds_and_omits_quantities() {
    let location = ShopifyId::new("gid://shopify/Location/123").unwrap();
    let order = ShopifyId::new("gid://shopify/Order/456").unwrap();
    let item = ShopifyId::new("gid://shopify/InventoryItem/789").unwrap();
    let key = Uuid::new_v4();
    assert!(
        ActivateInventoryRequest {
            inventory_item_id: order.clone(),
            location_id: location.clone(),
            idempotency_key: key,
        }
        .variables()
        .is_err()
    );
    assert!(
        ActivateInventoryRequest {
            inventory_item_id: item.clone(),
            location_id: order,
            idempotency_key: key,
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

#[test]
fn page_sizes_and_continuation_metadata_are_validated() {
    for first in [0, 251] {
        assert!(PageRequest { first, after: None }.validate().is_err());
    }
    for first in [1, 250] {
        assert!(
            PageRequest {
                first,
                after: Some("next".into())
            }
            .validate()
            .is_ok()
        );
    }
    for (has_next_page, end_cursor, after, valid) in [
        (true, None, None, false),
        (true, Some(""), None, false),
        (true, Some("a"), Some("a"), false),
        (true, Some("b"), Some("a"), true),
        (false, None, None, true),
    ] {
        assert_eq!(
            Connection {
                nodes: Vec::<()>::new(),
                page_info: PageInfo {
                    has_next_page,
                    end_cursor: end_cursor.map(str::to_owned)
                }
            }
            .validate_after(after)
            .is_ok(),
            valid
        );
    }
}

#[test]
fn bounded_nested_connections_reject_incomplete_data() {
    let complete = Connection {
        nodes: vec![1; 250],
        page_info: PageInfo {
            has_next_page: false,
            end_cursor: None,
        },
    };
    assert_eq!(complete.into_complete_nodes().unwrap().len(), 250);
    let incomplete = Connection {
        nodes: vec![1; 250],
        page_info: PageInfo {
            has_next_page: true,
            end_cursor: Some("more".into()),
        },
    };
    assert!(incomplete.into_complete_nodes().is_err());
}

#[test]
fn activation_candidates_reject_zero_and_overflowed_resource_ids() {
    for (item, variant, product) in [
        ("0", "1", "42"),
        ("18446744073709551616", "1", "42"),
        ("1", "0", "42"),
        ("1", "18446744073709551616", "42"),
        ("1", "1", "0"),
        ("1", "1", "18446744073709551616"),
    ] {
        let candidate: InventoryActivationCandidate = serde_json::from_value(json!({
            "id":"level","isActive":false,
            "quantities":[{"name":"on_hand","quantity":1},{"name":"available","quantity":0}],
            "item":{"id":format!("gid://shopify/InventoryItem/{item}"),"sku":null,"tracked":true,
                "variant":{"id":format!("gid://shopify/ProductVariant/{variant}"),"title":"Variant",
                    "product":{"id":format!("gid://shopify/Product/{product}"),"title":"Product"}}}
        }))
        .unwrap();
        assert!(
            candidate.activation_item().is_err(),
            "item={item}, variant={variant}, product={product}"
        );
    }
}

#[test]
fn fulfillment_resource_kinds_share_checked_shopify_numeric_extraction() {
    for kind in [
        "Order",
        "FulfillmentOrder",
        "FulfillmentOrderLineItem",
        "Fulfillment",
    ] {
        assert_eq!(
            ShopifyId::new(format!("gid://shopify/{kind}/42"))
                .expect("valid ID")
                .number(kind)
                .expect("numeric ID"),
            42
        );
        for value in ["0", "18446744073709551616"] {
            assert!(
                ShopifyId::new(format!("gid://shopify/{kind}/{value}"))
                    .expect("valid lexical ID")
                    .number(kind)
                    .is_err()
            );
        }
    }
}

#[test]
fn shipping_weight_accepts_shopify_unsigned_integer_strings() {
    let order:ShippingOrder=serde_json::from_value(json!({"id":"gid://shopify/Order/1","email":null,"totalWeight":"2000","shippingAddress":null,"shippingLines":{"nodes":[]},"lineItems":{"nodes":[]}})).expect("valid fixture");
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

#[test]
fn shipping_query_keeps_required_money_fields_after_removing_order_currency() {
    let query = include_str!("../../client/shopify/shipping_orders.graphql")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    assert!(query.contains("shopMoney { amount currencyCode }"));
    let money: MoneyBag =
        serde_json::from_value(json!({"shopMoney":{"amount":"30","currencyCode":"USD"}}))
            .expect("query-selected price");
    assert_eq!(money.shop_money.currency_code, "USD");
}
