#![allow(clippy::unwrap_used)]
use super::*;
use serde_json::{Value, json};

fn variant() -> Value {
    json!({"id":"gid://shopify/ProductVariant/2","title":"Blue","sku":null,"image":{"url":"https://example.test/variant.png"},"product":{"id":"gid://shopify/Product/1","title":"Product","featuredImage":{"url":"https://example.test/product.png"},"status":"ACTIVE"},"inventoryPolicy":"DENY","inventoryItem":{"tracked":true}})
}

#[test]
fn selection_images_require_urls_without_requesting_alt_text() {
    let decoded: Variant = serde_json::from_value(variant()).unwrap();
    assert_eq!(decoded.image.unwrap().alt_text, None);
    assert_eq!(decoded.product.featured_image.unwrap().alt_text, None);
    for pointer in ["/image", "/product/featuredImage"] {
        for invalid in [Value::Null, json!({}), json!({"url":17})] {
            let mut value = variant();
            *value.pointer_mut(pointer).unwrap() = invalid.clone();
            assert_eq!(
                serde_json::from_value::<Variant>(value).is_ok(),
                invalid.is_null()
            );
        }
    }
    for (pointer, field) in [("", "image"), ("/product", "featuredImage")] {
        let mut value = variant();
        value
            .pointer_mut(pointer)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .remove(field);
        assert!(serde_json::from_value::<Variant>(value).is_err());
    }
}

#[test]
fn selection_inventory_policy_is_required_and_strict() {
    for invalid in [Value::Null, json!("UNKNOWN"), json!(17)] {
        let mut value = variant();
        value["inventoryPolicy"] = invalid;
        assert!(serde_json::from_value::<Variant>(value).is_err());
    }
    let mut value = variant();
    value.as_object_mut().unwrap().remove("inventoryPolicy");
    assert!(serde_json::from_value::<Variant>(value).is_err());
}
