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

#[test]
fn verification_status_enums_preserve_known_and_distinct_unknown_values() {
    use crate::models::stripe::{
        CheckoutMode, CheckoutPaymentStatus, CheckoutStatus, PaymentIntentStatus,
    };
    fn roundtrip<
        T: serde::de::DeserializeOwned + serde::Serialize + std::fmt::Debug + PartialEq,
    >(
        values: &[&str],
    ) {
        for value in values {
            let parsed: T = serde_json::from_value(json!(value)).unwrap();
            assert_eq!(serde_json::to_value(parsed).unwrap(), json!(value));
        }
        let a: T = serde_json::from_value(json!("future_a")).unwrap();
        let b: T = serde_json::from_value(json!("future_b")).unwrap();
        assert_ne!(a, b);
    }
    roundtrip::<CheckoutMode>(&["payment", "setup", "subscription", "future"]);
    roundtrip::<CheckoutStatus>(&["open", "complete", "expired", "future"]);
    roundtrip::<CheckoutPaymentStatus>(&["paid", "unpaid", "no_payment_required", "future"]);
    roundtrip::<PaymentIntentStatus>(&[
        "requires_payment_method",
        "requires_confirmation",
        "requires_action",
        "processing",
        "requires_capture",
        "canceled",
        "succeeded",
        "future",
    ]);
    assert_eq!(
        serde_json::from_value::<PaymentIntentStatus>(json!("succeeded")).unwrap(),
        PaymentIntentStatus::Succeeded
    );
}

#[test]
fn checkout_reads_allow_pending_nulls_but_require_authoritative_status_and_environment() {
    use crate::models::stripe::{CheckoutSession, PaymentIntent};
    let value = json!({"id":"cs_test_pending","object":"checkout.session","mode":"payment","status":"open","payment_status":"unpaid","livemode":false,"amount_total":null,"currency":null,"client_reference_id":null,"metadata":{},"payment_intent":null});
    let session: CheckoutSession = serde_json::from_value(value.clone()).unwrap();
    assert!(session.payment_intent.is_none());
    assert!(session.amount_total.is_none());
    for field in [
        "id",
        "mode",
        "status",
        "payment_status",
        "livemode",
        "metadata",
    ] {
        let mut missing = value.clone();
        missing.as_object_mut().unwrap().remove(field);
        assert!(
            serde_json::from_value::<CheckoutSession>(missing).is_err(),
            "{field}"
        );
    }
    let intent = json!({"id":"pi_one","object":"payment_intent","status":"succeeded","amount":1,"amount_received":1,"currency":"thb","livemode":false,"latest_charge":"ch_one","metadata":{}});
    for field in [
        "id",
        "status",
        "amount",
        "amount_received",
        "currency",
        "livemode",
        "metadata",
    ] {
        let mut missing = intent.clone();
        missing.as_object_mut().unwrap().remove(field);
        assert!(
            serde_json::from_value::<PaymentIntent>(missing).is_err(),
            "{field}"
        );
    }
    let mut expanded = value;
    expanded["payment_intent"] = json!({"id":"pi_one"});
    assert!(serde_json::from_value::<CheckoutSession>(expanded).is_err());
}
