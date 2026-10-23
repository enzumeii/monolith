#![allow(clippy::unwrap_used)]
use crate::models::stripe::{BalanceTransactionKind, FeeKind};
use serde_json::json;

#[test]
fn stripe_kinds_keep_exact_known_and_unknown_wire_values() {
    for (wire, expected) in [
        ("charge", BalanceTransactionKind::Charge),
        ("payment", BalanceTransactionKind::Payment),
        ("refund", BalanceTransactionKind::Refund),
        ("payment_refund", BalanceTransactionKind::PaymentRefund),
        (
            "future_type",
            BalanceTransactionKind::Other("future_type".into()),
        ),
    ] {
        let kind: BalanceTransactionKind = serde_json::from_value(json!(wire)).unwrap();
        assert_eq!(kind, expected);
        assert_eq!(serde_json::to_value(kind).unwrap(), json!(wire));
    }
    for (wire, expected) in [
        ("stripe_fee", FeeKind::StripeFee),
        ("tax", FeeKind::Tax),
        ("future_fee", FeeKind::Other("future_fee".into())),
    ] {
        let kind: FeeKind = serde_json::from_value(json!(wire)).unwrap();
        assert_eq!(kind, expected);
        assert_eq!(serde_json::to_value(kind).unwrap(), json!(wire));
    }
}

#[test]
fn checkout_protocols_keep_unknown_strings_and_decode_real_references() {
    use crate::models::stripe::{
        CheckoutMode, CheckoutPaymentStatus, CheckoutStatus, PaymentIntent, PaymentIntentStatus,
        StripeReference,
    };
    for raw in ["payment", "subscription", "setup", "future_mode"] {
        let value: CheckoutMode = serde_json::from_value(json!(raw)).unwrap();
        assert_eq!(serde_json::to_value(value).unwrap(), json!(raw));
    }
    for raw in ["open", "complete", "expired", "future_status"] {
        let value: CheckoutStatus = serde_json::from_value(json!(raw)).unwrap();
        assert_eq!(serde_json::to_value(value).unwrap(), json!(raw));
    }
    for raw in ["paid", "unpaid", "no_payment_required", "future_status"] {
        let value: CheckoutPaymentStatus = serde_json::from_value(json!(raw)).unwrap();
        assert_eq!(serde_json::to_value(value).unwrap(), json!(raw));
    }
    for raw in [
        "succeeded",
        "processing",
        "requires_capture",
        "requires_action",
        "requires_confirmation",
        "requires_payment_method",
        "canceled",
        "future_status",
    ] {
        let value: PaymentIntentStatus = serde_json::from_value(json!(raw)).unwrap();
        assert_eq!(serde_json::to_value(value).unwrap(), json!(raw));
    }
    let id: StripeReference<PaymentIntent> = serde_json::from_value(json!("pi_actual")).unwrap();
    assert_eq!(id.id(), "pi_actual");
    let expanded: StripeReference<PaymentIntent>=serde_json::from_value(json!({"id":"pi_actual","object":"payment_intent","status":"succeeded","amount":1200,"amount_received":1200,"currency":"thb","livemode":false,"metadata":{},"latest_charge":null})).unwrap();
    assert_eq!(expanded.id(), "pi_actual");
    assert!(
        serde_json::from_value::<StripeReference<PaymentIntent>>(json!({"id":"pi_incomplete"}))
            .is_err()
    );
    assert!(serde_json::from_value::<StripeReference<PaymentIntent>>(json!(42)).is_err());
}
#[test]
fn checkout_response_requires_verification_fields_and_exact_integer_money() {
    use crate::models::stripe::CheckoutSession;
    let complete = json!({"id":"cs_actual","object":"checkout.session","mode":"payment","status":"complete","payment_status":"paid","livemode":false,"amount_total":1200,"currency":"thb","metadata":{},"payment_intent":"pi_actual","url":null});
    assert!(serde_json::from_value::<CheckoutSession>(complete.clone()).is_ok());
    for field in [
        "id",
        "object",
        "mode",
        "status",
        "payment_status",
        "livemode",
        "metadata",
    ] {
        let mut invalid = complete.clone();
        invalid.as_object_mut().unwrap().remove(field);
        assert!(
            serde_json::from_value::<CheckoutSession>(invalid).is_err(),
            "{field}"
        );
    }
    for amount in [json!(1.5), json!("1200"), json!(true)] {
        let mut invalid = complete.clone();
        invalid["amount_total"] = amount;
        assert!(serde_json::from_value::<CheckoutSession>(invalid).is_err());
    }
}
