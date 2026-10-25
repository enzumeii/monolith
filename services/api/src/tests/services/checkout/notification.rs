#![allow(clippy::unwrap_used)]
use super::*;
use serde_json::json;

fn settings() -> CheckoutSettings {
    serde_json::from_value(json!({
        "frontend_origin":"https://example.test",
        "return_path_template":"/{convention_slug}/{checkout_id}",
        "cancel_path_template":"/{convention_slug}/{checkout_id}",
        "shopify_location_id":"1",
        "reservation_secs":3600,"guest_recovery_secs":604_800,"stripe_livemode":false
    }))
    .unwrap()
}

#[test]
fn authenticated_irrelevant_events_require_expected_environment_and_account() {
    let config = settings();
    let event = || stripe::VerifiedEvent {
        event_type: stripe::EventType::Other("unrelated.event".into()),
        livemode: false,
        account: Some("acct_test".into()),
        object_id: "obj_fixture".into(),
        checkout_reference: None,
    };
    assert!(
        stripe(&config, event(), Some("acct_test"))
            .unwrap()
            .is_none()
    );
    let mut wrong = event();
    wrong.account = Some("acct_other".into());
    assert!(stripe(&config, wrong, Some("acct_test")).is_err());
    let mut wrong = event();
    wrong.livemode = true;
    assert!(stripe(&config, wrong, Some("acct_test")).is_err());
}

#[test]
fn selected_events_carry_checkout_identity_without_binding_payment_intent_as_session() {
    let id = Uuid::new_v4();
    let config = settings();
    for (kind, object_id, expected_resource) in [
        (
            stripe::EventType::CheckoutSessionCompleted,
            "cs_fixture",
            Some("cs_fixture"),
        ),
        (
            stripe::EventType::PaymentIntentSucceeded,
            "pi_fixture",
            None,
        ),
    ] {
        let notification = stripe(
            &config,
            stripe::VerifiedEvent {
                event_type: kind,
                livemode: false,
                account: Some("acct_test".into()),
                object_id: object_id.into(),
                checkout_reference: Some(id.to_string()),
            },
            Some("acct_test"),
        )
        .unwrap()
        .unwrap();
        assert_eq!(notification.provider, CheckoutProvider::Stripe);
        assert_eq!(notification.resource_id.as_deref(), expected_resource);
        assert_eq!(notification.correlation_id, Some(id));
    }
}

#[test]
fn paypal_selects_typed_notifications_and_parses_optional_correlation() {
    let id = Uuid::new_v4();
    for (reference, expected) in [
        (Some(id.to_string()), Some(id)),
        (Some("invalid".into()), None),
        (None, None),
    ] {
        let event = paypal::VerifiedEvent {
            event_type: WebhookEventType::CaptureCompleted,
            order_id: Some("ORDER123".into()),
            checkout_reference: reference,
        };
        let notification = paypal(event).unwrap();
        assert_eq!(notification.provider, CheckoutProvider::Paypal);
        assert_eq!(notification.resource_id.as_deref(), Some("ORDER123"));
        assert_eq!(notification.correlation_id, expected);
    }
    assert!(
        paypal(paypal::VerifiedEvent {
            event_type: WebhookEventType::Other("unrelated.event".into()),
            order_id: None,
            checkout_reference: None,
        })
        .is_none()
    );
}
