#![allow(clippy::unwrap_used)]
use crate::dto::{PatchConvention, PatchCronConfiguration, PatchField};
use serde_json::json;
#[test]
fn common_patch_field_retains_missing_set_and_null_on_both_contracts() {
    let value: PatchConvention = serde_json::from_value(
        json!({"logo_location":null,"name":"Updated","preorder_enabled":false}),
    )
    .unwrap();
    assert_eq!(value.logo_location, PatchField::Clear);
    assert_eq!(value.name, PatchField::Set("Updated".into()));
    assert_eq!(value.country_code, PatchField::Missing);
    assert_eq!(value.preorder_enabled, PatchField::Set(false));
    assert_eq!(
        serde_json::to_value(value).unwrap(),
        json!({"logo_location":null,"name":"Updated","preorder_enabled":false})
    );
    let cron: PatchCronConfiguration =
        serde_json::from_value(json!({"name":null,"enabled":false})).unwrap();
    assert_eq!(cron.name, PatchField::Clear);
    assert_eq!(cron.enabled, PatchField::Set(false));
    assert_eq!(cron.parameter, PatchField::Missing);
}
#[test]
fn convention_dtos_reject_unknown_fields_and_json_number_prices() {
    assert!(serde_json::from_value::<PatchConvention>(json!({"slug":"immutable"})).is_err());
    assert!(serde_json::from_value::<crate::dto::ConventionVariantInput>(json!({"shopify_product_id":"gid://shopify/Product/1","shopify_variant_id":"gid://shopify/ProductVariant/2","final_price":12.99})).is_err());
}

#[test]
fn logo_attachment_accepts_location_but_rejects_render_url() {
    assert!(
        serde_json::from_value::<PatchConvention>(
            json!({"logo_location":"conventions/fair/550e8400-e29b-41d4-a716-446655440000.png"})
        )
        .is_ok()
    );
    assert!(
        serde_json::from_value::<PatchConvention>(
            json!({"logo_url":"https://storage.test/signed"})
        )
        .is_err()
    );
}

#[test]
fn selected_variant_serialization_preserves_display_snapshot_and_legacy_nulls() {
    let mut value = json!({"id":"id","convention_id":"parent","shopify_product_id":"gid://shopify/Product/1","shopify_variant_id":"gid://shopify/ProductVariant/2","final_price":"12.99","created_at":"2026-10-23T00:00:00Z","updated_at":"2026-10-23T00:00:00Z","inventory_policy":"DENY","product_title":"Product","variant_title":"Blue","sku":"SKU-2","product_image":{"url":"https://example.test/product.png"},"variant_image":null});
    let dto: crate::dto::ConventionVariantDTO = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(serde_json::to_value(dto).unwrap(), value);
    for key in [
        "inventory_policy",
        "product_title",
        "variant_title",
        "sku",
        "product_image",
        "variant_image",
    ] {
        value[key] = serde_json::Value::Null;
    }
    let dto: crate::dto::ConventionVariantDTO = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(serde_json::to_value(dto).unwrap(), value);
}

#[test]
fn inventory_policy_round_trips_both_wire_values_and_rejects_unknown() {
    use crate::dto::CatalogueInventoryPolicy;
    for (policy, wire) in [
        (CatalogueInventoryPolicy::Deny, "DENY"),
        (CatalogueInventoryPolicy::Continue, "CONTINUE"),
    ] {
        assert_eq!(serde_json::to_value(policy).unwrap(), json!(wire));
        assert_eq!(
            serde_json::from_value::<CatalogueInventoryPolicy>(json!(wire)).unwrap(),
            policy
        );
    }
    assert!(serde_json::from_value::<CatalogueInventoryPolicy>(json!("UNKNOWN")).is_err());
}

#[test]
fn catalogue_images_serialize_only_the_url() {
    let image: crate::dto::CatalogueImage =
        serde_json::from_value(json!({"url":"https://example.test/image.png"})).unwrap();
    assert_eq!(
        serde_json::to_value(image).unwrap(),
        json!({"url":"https://example.test/image.png"})
    );
}

#[test]
fn catalogue_variants_omit_unavailable_reason() {
    let variant: crate::dto::CatalogueVariant = serde_json::from_value(json!({
        "id":"gid://shopify/ProductVariant/2", "inventory_item_id":"gid://shopify/InventoryItem/3",
        "title":"Large", "sku":null, "image":null, "selected_options":[],
        "price":{"amount":"12.99","currency_code":"USD"}, "tracked":true,
        "inventory_policy":"CONTINUE", "available_quantity":0, "eligible":true
    }))
    .unwrap();
    let wire = serde_json::to_value(variant).unwrap();
    assert!(wire.get("unavailable_reason").is_none());
    assert_eq!(wire["available_quantity"], 0);
    assert_eq!(wire["eligible"], true);
}
