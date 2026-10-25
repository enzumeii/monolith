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
        assert_eq!(create(request(raw)).unwrap().request.country_code, expected);
        let patch = serde_json::from_value(json!({"country_code":raw})).unwrap();
        assert!(crate::validations::conventions::patch(patch).is_ok());
    }
    for raw in ["ZZ", "XX", "", " TH", "TH ", "THA", "ก", "1A"] {
        assert_eq!(
            create(request(raw)).err().unwrap().into_response().status(),
            StatusCode::BAD_REQUEST
        );
        let patch = serde_json::from_value(json!({"country_code":raw})).unwrap();
        assert_eq!(
            crate::validations::conventions::patch(patch)
                .unwrap_err()
                .into_response()
                .status(),
            StatusCode::BAD_REQUEST
        );
    }
}

#[test]
fn logo_locations_are_canonical() {
    let key = "conventions/fair/550e8400-e29b-41d4-a716-446655440000.png";
    assert_eq!(logo_location(key).unwrap(), "image/png");
    for bad in [
        "reports/fair/a.png",
        "conventions/fair/../a.png",
        "conventions/fair/a.png",
        "https://storage.test/a.png",
        "conventions/fair/550e8400-e29b-41d4-a716-446655440000.svg",
        "conventions/fair/550E8400-E29B-41D4-A716-446655440000.png",
        "conventions/fair/550e8400-e29b-41d4-0716-446655440000.png",
    ] {
        assert!(logo_location(bad).is_err(), "{bad}");
    }
}
