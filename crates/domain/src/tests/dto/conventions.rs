#![allow(clippy::unwrap_used)]
use crate::dto::{PatchConvention, PatchCronConfiguration, PatchField};
use serde_json::json;
#[test]
fn common_patch_field_retains_missing_set_and_null_on_both_contracts() {
    let value: PatchConvention =
        serde_json::from_value(json!({"logo_url":null,"name":"Updated","preorder_enabled":false}))
            .unwrap();
    assert_eq!(value.logo_url, PatchField::Clear);
    assert_eq!(value.name, PatchField::Set("Updated".into()));
    assert_eq!(value.country_code, PatchField::Missing);
    assert_eq!(value.preorder_enabled, PatchField::Set(false));
    assert_eq!(
        serde_json::to_value(value).unwrap(),
        json!({"logo_url":null,"name":"Updated","preorder_enabled":false})
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
