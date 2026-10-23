#![allow(clippy::unwrap_used)]
use super::*;

fn settings() -> CheckoutSettings {
    CheckoutSettings {
        frontend_origin: "http://localhost:8080".into(),
        return_path_template: "/{convention_slug}/checkout/return?checkout_id={checkout_id}".into(),
        cancel_path_template: "/{convention_slug}/checkout/cancel?checkout_id={checkout_id}".into(),
        shopify_location_id: "gid://shopify/Location/1".into(),
        reservation_secs: 3600,
        guest_recovery_secs: 7 * 86400,
        stripe_expected_account: "acct_test".into(),
        stripe_livemode: false,
        paypal_api_origin: "https://api-m.sandbox.paypal.com".into(),
        paypal_merchant_id: Some("private_merchant".into()),
        stripe_webhook_secret: Some("private_signature_secret".into()),
        paypal_webhook_id: Some("private_webhook_registration".into()),
    }
}

#[test]
fn checkout_requires_trusted_origins_and_bounded_recovery() {
    let valid = settings();
    assert!(valid.validate(true).is_ok());
    assert!(valid.validate(false).is_err());
    let mut invalid = valid.clone();
    invalid.reservation_secs = 1799;
    assert!(invalid.validate(true).is_err());
    invalid = valid.clone();
    invalid.guest_recovery_secs = 31 * 86400 + 1;
    assert!(invalid.validate(true).is_err());
    invalid = valid.clone();
    invalid.frontend_origin = "https://example.test/path".into();
    assert!(invalid.validate(true).is_err());
}

#[test]
fn checkout_returns_stay_under_a_trusted_convention_path() {
    for template in [
        "//evil.test/{convention_slug}/{checkout_id}",
        "/{convention_slug}/../{checkout_id}",
        "/{convention_slug}/return?token={guest_secret}&id={checkout_id}",
        "/checkout/return?checkout_id={checkout_id}",
        "/{convention_slug}/return#id={checkout_id}",
    ] {
        let mut invalid = settings();
        invalid.return_path_template = template.into();
        assert!(invalid.validate(true).is_err(), "{template}");
    }
}

#[test]
fn checkout_configuration_debug_redacts_private_identifiers_and_secrets() {
    let debug = format!("{:?}", settings());
    for secret in [
        "private_merchant",
        "private_signature_secret",
        "private_webhook_registration",
    ] {
        assert!(!debug.contains(secret));
    }
}
