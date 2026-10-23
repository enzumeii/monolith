#![allow(clippy::unwrap_used)]
use super::*;
use axum::response::IntoResponse;
use serde_json::json;

fn request(country: &str) -> CreateConvention {
    serde_json::from_value(json!({"slug":"country-test","name":"Convention","country_code":country,"event_starts_on":"2028-02-29","event_ends_on":"2028-02-29","description_markdown":"Event","currency_code":"THB","payment_methods":["stripe"]})).unwrap()
}

#[test]
fn create_and_patch_preserve_country_normalization_and_reject_unknown_codes() {
    for (raw, expected) in [("th", "TH"), ("uS", "US"), ("AX", "AX")] {
        assert_eq!(create(request(raw)).unwrap().country_code, expected);
        let patch = serde_json::from_value(json!({"country_code":raw})).unwrap();
        assert!(patch_shape(&patch).is_ok());
    }
    for raw in ["ZZ", "XX", "", " TH", "TH ", "THA", "ก", "1A"] {
        assert_eq!(
            create(request(raw)).err().unwrap().into_response().status(),
            StatusCode::BAD_REQUEST
        );
        let patch = serde_json::from_value(json!({"country_code":raw})).unwrap();
        assert_eq!(
            patch_shape(&patch).unwrap_err().into_response().status(),
            StatusCode::BAD_REQUEST
        );
    }
}

#[test]
fn patch_preserves_omitted_legacy_logos_and_validates_supplied_locations() {
    let input = create(request("TH")).unwrap();
    let legacy_url = "https://legacy.example.test/convention.png";
    let current = Convention {
        id: uuid::Uuid::new_v4(),
        slug: input.slug,
        name: input.name,
        logo_location: Some(legacy_url.into()),
        country_code: input.country_code,
        event_starts_on: input.event_starts_on,
        event_ends_on: input.event_ends_on,
        description_markdown: input.description_markdown,
        preorder_enabled: false,
        currency_code: input.currency_code,
        payment_methods: vec!["stripe".into()],
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    let patch = serde_json::from_value(json!({"name":"Updated convention"})).unwrap();
    let merged = merge(patch, &current).unwrap();
    assert_eq!(merged.name, "Updated convention");
    assert_eq!(merged.logo_location.as_deref(), Some(legacy_url));
    let patch = serde_json::from_value(json!({"logo_location":null})).unwrap();
    assert!(merge(patch, &current).unwrap().logo_location.is_none());
    let location = "conventions/country-test/550e8400-e29b-41d4-a716-446655440000.png";
    let patch = serde_json::from_value(json!({"logo_location":location})).unwrap();
    assert_eq!(
        merge(patch, &current).unwrap().logo_location.as_deref(),
        Some(location)
    );
    let patch = serde_json::from_value(json!({"logo_location":legacy_url})).unwrap();
    assert!(patch_shape(&patch).is_err());
    assert!(merge(patch, &current).is_err());
    let mut new_convention = request("TH");
    new_convention.logo_location = Some(legacy_url.into());
    assert!(create(new_convention).is_err());
}
