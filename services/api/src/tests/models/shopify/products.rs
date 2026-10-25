#![allow(clippy::unwrap_used)]
use super::*;
use serde_json::json;
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
