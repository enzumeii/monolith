#![allow(clippy::unwrap_used)]
use super::*;
use serde_json::json;
#[test]
fn source_models_derive_collection_and_inventory_rows() {
    for (raw, want) in [
        ("GENSTSWOW01", Some("STSWOW")),
        ("กขคSTSWOW01", Some("STSWOW")),
        ("SHORT", None),
    ] {
        let sku: crate::models::Sku = serde_json::from_value(json!(raw)).unwrap();
        assert_eq!(sku.collection().as_deref(), want);
    }
    for (active, quantity) in [(true, 0), (true, -2), (false, 3)] {
        let value = json!({"id":"gid://shopify/ProductVariant/1","title":"Large","sku":"GENSTSWOW01","contextualPricing":{"price":{"amount":"0.1","currencyCode":"USD"}},"image":null,
            "inventoryItem":{"id":"gid://shopify/InventoryItem/1","tracked":true,"unitCost":null,"measurement":{"weight":null},"inventoryLevel":{"id":"level","isActive":active,"quantities":[{"name":"on_hand","quantity":quantity}]}}});
        let row = serde_json::from_value::<Variant>(value)
            .unwrap()
            .into_report_row("Parent", "KEYCHAIN")
            .unwrap();
        if active {
            let row = row.unwrap().into_dto();
            assert_eq!(row.quantity, quantity);
            assert_eq!(row.collection.as_deref(), Some("STSWOW"));
            assert_eq!(row.title, "Parent");
        } else {
            assert!(row.is_none());
        }
    }
}
#[test]
fn line_conversion_keeps_historical_values_and_missing_metadata() {
    let line:TaggedOrderLineItem=serde_json::from_value(json!({"title":"Deleted","variantTitle":null,"sku":null,"quantity":3,"originalUnitPriceSet":{"shopMoney":{"amount":"0.1","currencyCode":"THB"}},"product":null,"variant":null})).unwrap();
    let row = line.into_report_row().unwrap().into_dto();
    assert_eq!(row.total_selling_price.to_string(), "0.3");
    assert!(row.cost.is_none());
    assert!(row.collection.is_none());
    assert_eq!(row.selling_price_currency, "THB");
}
#[test]
fn missing_required_inventory_quantity_is_a_model_protocol_error() {
    let level: InventoryLevel = serde_json::from_value(
        json!({"id":"level","isActive":true,"quantities":[{"name":"available","quantity":3}]}),
    )
    .unwrap();
    let error = level.on_hand().unwrap_err();
    assert!(matches!(
        error.kind(),
        ClientErrorKind::Protocol("on_hand quantity is missing")
    ));
    assert_eq!(
        error.location().file(),
        "services/api/src/models/shopify/products.rs"
    );
    assert!(error.location().line() > 0);
    assert!(error.location().column() > 0);
}

#[test]
fn inventory_report_decodes_without_unused_catalog_fields() {
    let product: crate::models::shopify::Product = serde_json::from_value(json!({
        "title":"Parent", "productType":"KEYCHAIN",
        "variants":{"nodes":[{"title":"Large","sku":"GENSTSWOW01",
            "contextualPricing":{"price":{"amount":"2.5","currencyCode":"THB"}},
            "inventoryItem":{"unitCost":null,"measurement":{"weight":null},
                "inventoryLevel":{"id":"level","isActive":true,"quantities":[{"name":"on_hand","quantity":2}]}}
        }],"pageInfo":{"hasNextPage":false,"endCursor":null}}
    })).unwrap();
    let row = product
        .variants
        .nodes
        .into_iter()
        .next()
        .unwrap()
        .into_report_row(&product.title, &product.product_type)
        .unwrap()
        .unwrap()
        .into_dto();
    assert_eq!(row.quantity, 2);
    assert_eq!(row.total_selling_price.to_string(), "5");
}

#[test]
fn order_report_decodes_without_unused_order_header() {
    let order: crate::models::shopify::TaggedOrder = serde_json::from_value(json!({
        "lineItems":{"nodes":[{"title":"Item","variantTitle":null,"sku":null,"quantity":2,
            "originalUnitPriceSet":{"shopMoney":{"amount":"2.5","currencyCode":"THB"}},"product":null,"variant":null
        }],"pageInfo":{"hasNextPage":false,"endCursor":null}}
    })).unwrap();
    let row = order
        .line_items
        .nodes
        .into_iter()
        .next()
        .unwrap()
        .into_report_row()
        .unwrap()
        .into_dto();
    assert_eq!(row.quantity, 2);
    assert_eq!(row.total_selling_price.to_string(), "5");
}
