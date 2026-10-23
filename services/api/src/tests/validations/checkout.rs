#![allow(clippy::unwrap_used)]
use super::*;
use axum::http::header::AUTHORIZATION;
use domain::dto::checkout::{CheckoutItemInput, CheckoutProvider};

fn request() -> CreateCheckoutRequest {
    CreateCheckoutRequest {
        operation_id: Uuid::new_v4().to_string(),
        convention_id: Uuid::new_v4().to_string(),
        email: "buyer@example.test".into(),
        provider: CheckoutProvider::Stripe,
        items: vec![CheckoutItemInput {
            variant_id: Uuid::new_v4().to_string(),
            quantity: 1,
            expected_unit_price: 2000,
        }],
        expected_currency: "SGD".into(),
    }
}

#[test]
fn guest_credential_hashes_decoded_secret_and_rejects_other_authentication() {
    let mut headers = HeaderMap::new();
    assert!(credential(&headers).is_err());
    for invalid in [
        "Basic secret".to_owned(),
        "Bearer short".to_owned(),
        format!("Bearer {}", "x".repeat(64)),
    ] {
        headers.insert(AUTHORIZATION, invalid.parse().unwrap());
        assert!(credential(&headers).is_err());
    }
    headers.insert(
        AUTHORIZATION,
        format!("Bearer {}", "ab".repeat(32)).parse().unwrap(),
    );
    let lowercase = credential(&headers).unwrap();
    headers.insert(
        AUTHORIZATION,
        format!("Bearer {}", "AB".repeat(32)).parse().unwrap(),
    );
    assert_eq!(credential(&headers).unwrap(), lowercase);
    assert_ne!(lowercase, vec![0xab; 32]);
}

#[test]
fn checkout_accepts_bounds_and_rejects_duplicate_variants_and_invalid_money() {
    let mut valid = request();
    valid.items[0].quantity = 999;
    assert!(create(&valid).is_ok());
    let mut invalid = valid.clone();
    invalid.items.push(invalid.items[0].clone());
    assert!(create(&invalid).is_err());
    invalid = valid.clone();
    invalid.items[0].quantity = 0;
    assert!(create(&invalid).is_err());
    invalid = valid.clone();
    invalid.items[0].quantity = 1000;
    assert!(create(&invalid).is_err());
    invalid = valid.clone();
    invalid.items[0].expected_unit_price = 0;
    assert!(create(&invalid).is_err());
    invalid = valid.clone();
    invalid.items.clear();
    assert!(create(&invalid).is_err());
    invalid = valid.clone();
    invalid.expected_currency = "sgd".into();
    assert!(create(&invalid).is_err());
    invalid = valid.clone();
    invalid.email = "bad-address".into();
    assert!(create(&invalid).is_err());
    invalid = valid.clone();
    invalid.email = " buyer@example.test".into();
    assert!(create(&invalid).is_err());
    invalid = valid;
    invalid.operation_id = Uuid::nil().to_string();
    assert!(create(&invalid).is_err());
}
