#![allow(clippy::unwrap_used)]
use super::*;
use serde_json::json;

fn variant() -> serde_json::Value {
    json!({"id":"gid://shopify/ProductVariant/2","title":"Default","sku":null,"image":null,"selectedOptions":[],"inventoryPolicy":"DENY","contextualPricing":{"price":{"amount":"0.00010","currencyCode":"USD"}},"inventoryItem":{"id":"gid://shopify/InventoryItem/3","tracked":true,"inventoryLevel":{"id":"level","isActive":true,"quantities":[{"name":"available","quantity":0}]}}})
}

#[test]
fn available_quantity_preserves_zero_and_distinguishes_absent_inventory() {
    let mut value = variant();
    let model: SelectionVariant = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(model.available_quantity().unwrap(), Some(0));
    let dto = crate::transformers::catalogue::variant(model, Some(0), true);
    assert_eq!(dto.price.amount, "0.00010");
    assert!(dto.eligible);
    value["inventoryItem"]["inventoryLevel"] = serde_json::Value::Null;
    let model: SelectionVariant = serde_json::from_value(value).unwrap();
    let quantity = model.available_quantity().unwrap();
    assert_eq!(quantity, None);
    assert!(!crate::transformers::catalogue::variant(model, quantity, quantity.is_some()).eligible);
}

#[test]
fn active_level_missing_available_fails_instead_of_becoming_zero() {
    let mut value = variant();
    value["inventoryItem"]["inventoryLevel"]["quantities"] = json!([]);
    let model: SelectionVariant = serde_json::from_value(value.clone()).unwrap();
    assert!(model.available_quantity().is_err());
    value["inventoryItem"]["inventoryLevel"]["isActive"] = json!(false);
    let model: SelectionVariant = serde_json::from_value(value).unwrap();
    let quantity = model.available_quantity().unwrap();
    assert_eq!(quantity, None);
    assert!(!crate::transformers::catalogue::variant(model, quantity, quantity.is_some()).eligible);
}
