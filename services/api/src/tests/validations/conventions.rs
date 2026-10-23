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
