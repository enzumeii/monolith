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
        let sku: crate::models::shopify::Sku = serde_json::from_value(json!(raw)).unwrap();
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
    assert!(level.on_hand().is_err());
}
