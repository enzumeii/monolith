#![allow(clippy::unwrap_used)]
use super::*;

#[test]
fn stored_inventory_policy_decodes_known_values_null_and_rejects_corruption() {
    let mut row = ConventionVariant {
        product_title: None,
        variant_title: None,
        sku: None,
        product_image_url: Some("https://example.test/product.png".into()),
        variant_image_url: None,
        inventory_policy: None,
        id: uuid::Uuid::new_v4(),
        convention_id: uuid::Uuid::new_v4(),
        shopify_product_id: "gid://shopify/Product/1".into(),
        shopify_variant_id: "gid://shopify/ProductVariant/2".into(),
        final_price: "12.99".into(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    assert_eq!(row.display().unwrap().inventory_policy, None);
    assert_eq!(
        row.display().unwrap().product_image.unwrap().url,
        "https://example.test/product.png"
    );
    for (wire, policy) in [
        ("DENY", domain::dto::CatalogueInventoryPolicy::Deny),
        ("CONTINUE", domain::dto::CatalogueInventoryPolicy::Continue),
    ] {
        row.inventory_policy = Some(wire.into());
        assert_eq!(
            row.clone().into_dto("THB").unwrap().inventory_policy,
            Some(policy)
        );
    }
    row.inventory_policy = Some("UNKNOWN".into());
    assert!(row.display().is_err());
    assert!(row.into_dto("THB").is_err());
}
