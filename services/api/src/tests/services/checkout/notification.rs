#![allow(clippy::unwrap_used)]
use super::*;
use hmac::{Hmac, KeyInit, Mac};
use serde_json::json;
use sha2::Sha256;

fn settings() -> CheckoutSettings {
    serde_json::from_value(json!({
        "frontend_origin":"https://example.test",
        "return_path_template":"/{convention_slug}/{checkout_id}",
        "cancel_path_template":"/{convention_slug}/{checkout_id}",
        "shopify_location_id":"gid://shopify/Location/1",
        "reservation_secs":3600,"guest_recovery_secs":604_800,
        "stripe_expected_account":"acct_test","stripe_livemode":false,
        "paypal_api_origin":"https://api-m.sandbox.paypal.com"
    }))
    .unwrap()
}

fn authenticated_event(
    config: &CheckoutSettings,
    value: &Value,
) -> Result<Option<Notification>, AppError> {
    let raw = serde_json::to_vec(value).unwrap();
    let timestamp = chrono::Utc::now().timestamp();
    let mut mac = <Hmac<Sha256> as KeyInit>::new_from_slice(b"whsec_fixture").unwrap();
    mac.update(format!("{timestamp}.").as_bytes());
    mac.update(&raw);
    let signature = format!(
        "t={timestamp},v1={}",
        hex::encode(mac.finalize().into_bytes())
    );
    stripe(
        config,
        StripeAuthentication {
            signature: &signature,
            secret: "whsec_fixture",
        },
        &raw,
    )
}

#[test]
fn authenticated_irrelevant_events_require_expected_environment_and_account() {
    let config = settings();
    let event = json!({"id":"evt_fixture","type":"unrelated.event","livemode":false,"account":"acct_test", "data":{"object":{"id":"obj_fixture","object":"unrelated"}}});
    assert!(authenticated_event(&config, &event).unwrap().is_none());
    let mut wrong = event.clone();
    wrong["account"] = json!("acct_other");
    assert!(authenticated_event(&config, &wrong).is_err());
    let mut wrong = event;
    wrong["livemode"] = json!(true);
    assert!(authenticated_event(&config, &wrong).is_err());
}

#[test]
fn selected_events_carry_checkout_identity_without_binding_payment_intent_as_session() {
    let id = Uuid::new_v4();
    let config = settings();
    for (kind, object_id, object_type, expected_resource) in [
        (
            "checkout.session.completed",
            "cs_fixture",
            "checkout.session",
            Some("cs_fixture"),
        ),
        (
            "payment_intent.succeeded",
            "pi_fixture",
            "payment_intent",
            None,
        ),
    ] {
        let notification = authenticated_event(&config, &json!({
            "id":"evt_fixture","type":kind,"livemode":false,"account":"acct_test",
            "data":{"object":{"id":object_id,"object":object_type,"metadata":{"checkout_id":id.to_string()}}}
        })).unwrap().unwrap();
        assert_eq!(notification.provider, CheckoutProvider::Stripe);
        assert_eq!(notification.event_id, "evt_fixture");
        assert_eq!(notification.resource_id.as_deref(), expected_resource);
        assert_eq!(notification.correlation_id, Some(id));
    }
}
