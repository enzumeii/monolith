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
        note: None,
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

#[test]
fn webhook_shape_checks_preserve_mixed_authentication_and_configuration_order() {
    let mut config: CheckoutSettings = serde_json::from_value(serde_json::json!({
        "frontend_origin":"https://example.test",
        "return_path_template":"/{convention_slug}/{checkout_id}",
        "cancel_path_template":"/{convention_slug}/{checkout_id}",
        "shopify_location_id":"1",
        "reservation_secs":3600,"guest_recovery_secs":604_800,"stripe_livemode":false
    }))
    .unwrap();
    let mut headers = HeaderMap::new();
    assert_eq!(
        stripe_authentication(&config, &headers)
            .err()
            .unwrap()
            .to_string(),
        "Stripe webhook configuration unavailable"
    );
    assert_eq!(
        paypal_authentication(&config, &headers, b"malformed")
            .err()
            .unwrap()
            .to_string(),
        "PayPal webhook configuration unavailable"
    );
    headers.insert("paypal-transmission-id", "fixture".parse().unwrap());
    assert_eq!(
        stripe_authentication(&config, &headers)
            .err()
            .unwrap()
            .to_string(),
        "Ambiguous webhook authentication"
    );
    headers.insert("stripe-signature", "fixture".parse().unwrap());
    assert_eq!(
        paypal_authentication(&config, &headers, b"malformed")
            .err()
            .unwrap()
            .to_string(),
        "Ambiguous webhook authentication"
    );
    headers.clear();
    assert_eq!(
        paypal_authentication(&config, &headers, &vec![0; 262_145])
            .err()
            .unwrap()
            .to_string(),
        "PayPal webhook payload exceeds supported limit"
    );
    config.paypal_webhook_id = Some("registered_fixture".into());
    assert_eq!(
        paypal_authentication(&config, &headers, b"malformed")
            .err()
            .unwrap()
            .to_string(),
        "Invalid PayPal webhook event"
    );
    assert_eq!(
        paypal_authentication(&config, &headers, b"{}")
            .err()
            .unwrap()
            .to_string(),
        "Missing or invalid webhook authentication header"
    );
    for (header, value) in [
        ("paypal-auth-algo", "SHA256withRSA"),
        ("paypal-cert-url", "https://api.paypal.com/cert/fixture"),
        ("paypal-transmission-id", "transmission_fixture"),
        ("paypal-transmission-sig", "signature_fixture"),
        ("paypal-transmission-time", "2026-10-24T00:00:00Z"),
    ] {
        headers.insert(header, value.parse().unwrap());
    }
    let raw = br#"{"resource" : {"id":"ORDER123"}, "event_type":"CHECKOUT.ORDER.APPROVED", "id":"EVENT\u0031"}"#;
    let payload = paypal_authentication(&config, &headers, raw).unwrap();
    let crate::models::checkout::WebhookInput::Paypal(payload) =
        crate::transformers::checkout::paypal_webhook(payload)
    else {
        panic!("PayPal authentication must map to PayPal input")
    };
    assert_eq!(payload.webhook_event.get().as_bytes(), raw);
    assert_eq!(payload.transmission_sig, "signature_fixture");
    assert_eq!(payload.webhook_id, "registered_fixture");
    headers.clear();
    config.stripe_webhook_secret = Some("whsec_fixture".into());
    assert_eq!(
        stripe_authentication(&config, &headers)
            .err()
            .unwrap()
            .to_string(),
        "Missing or invalid webhook authentication header"
    );
}

#[test]
fn checkout_accepts_public_variant_gid_and_rejects_malformed_or_other_resource_ids() {
    let mut input = request();
    input.items[0].variant_id = "gid://shopify/ProductVariant/2".into();
    assert!(create(&input).is_ok());
    for invalid in [
        "gid://shopify/Product/2",
        "gid://shopify/ProductVariant/0",
        "gid://shopify/ProductVariant/02",
        "gid://shopify/ProductVariant/-2",
        "gid://shopify/ProductVariant/2?foo=bar",
        "not-a-variant",
    ] {
        input.items[0].variant_id = invalid.into();
        assert!(create(&input).is_err(), "{invalid}");
    }
}

#[test]
fn checkout_note_accepts_unicode_and_layout_controls_but_rejects_oversize_and_other_controls() {
    let mut input = request();
    for note in [
        String::new(),
        " \t\r\n".to_owned(),
        "  Please sign\nありがとう 🐶\t<gift>  ".to_owned(),
        "🐶".repeat(2000),
    ] {
        input.note = Some(note);
        assert!(create(&input).is_ok());
    }
    for note in [
        "🐶".repeat(2001),
        "private\0note".into(),
        "private\u{7f}note".into(),
        "private\u{85}note".into(),
    ] {
        input.note = Some(note);
        let error = create(&input).expect_err("invalid note");
        assert!(!error.to_string().contains("private"));
    }
}
