#![allow(clippy::unwrap_used)]
use super::*;
use serde_json::json;

#[test]
fn checkout_request_rejects_payment_assertions_and_unsupported_provider() {
    let request = json!({"operation_id":"op","convention_id":"convention","email":"buyer@example.test","provider":"stripe","items":[{"variant_id":"variant","quantity":2,"expected_unit_price":2000}],"expected_currency":"SGD"});
    let parsed: CreateCheckoutRequest = serde_json::from_value(request.clone()).unwrap();
    assert_eq!(parsed.items[0].expected_unit_price, 2000);
    let mut asserted = request.clone();
    asserted["paid"] = json!(true);
    assert!(serde_json::from_value::<CreateCheckoutRequest>(asserted).is_err());
    let mut unsupported = request.clone();
    unsupported["provider"] = json!("promptpay");
    assert!(serde_json::from_value::<CreateCheckoutRequest>(unsupported).is_err());
    let mut fractional = request;
    fractional["items"][0]["expected_unit_price"] = json!(20.5);
    assert!(serde_json::from_value::<CreateCheckoutRequest>(fractional).is_err());
}

#[test]
fn checkout_envelope_has_exact_money_and_omits_unconfirmed_order() {
    let response = CheckoutResponse {
        checkout_id: "checkout".into(),
        status: CheckoutStatus::RequiresAction,
        money: CheckoutMoney {
            minor: 4000,
            currency: "SGD".into(),
        },
        reservation_expires_at: Some("2026-10-24T01:00:00Z".into()),
        next_action: Some(CheckoutNextAction::Redirect {
            url: "https://payments.example.test/checkout".into(),
        }),
        order: None,
    };
    let wire = serde_json::to_value(&response).unwrap();
    assert_eq!(wire["status"], "requires_action");
    assert_eq!(wire["money"], json!({"minor":4000,"currency":"SGD"}));
    assert_eq!(wire["next_action"]["type"], "redirect");
    assert!(wire.get("order").is_none());
    assert_eq!(
        serde_json::from_value::<CheckoutResponse>(wire).unwrap(),
        response
    );
}

#[test]
fn checkout_optional_note_preserves_legacy_wire_and_exact_customer_text() {
    let legacy = r#"{"operation_id":"op","convention_id":"convention","email":"buyer@example.test","provider":"stripe","items":[],"expected_currency":"SGD"}"#;
    let parsed: CreateCheckoutRequest = serde_json::from_str(legacy).unwrap();
    assert_eq!(serde_json::to_string(&parsed).unwrap(), legacy);
    let mut noted: serde_json::Value = serde_json::from_str(legacy).unwrap();
    noted["note"] = serde_json::Value::Null;
    let parsed: CreateCheckoutRequest = serde_json::from_value(noted.clone()).unwrap();
    assert_eq!(serde_json::to_string(&parsed).unwrap(), legacy);
    noted["note"] = json!("  Please sign\nありがとう 🐶\t<gift>  ");
    let parsed: CreateCheckoutRequest = serde_json::from_value(noted.clone()).unwrap();
    assert_eq!(serde_json::to_value(parsed).unwrap(), noted);
    noted["note"] = json!(123);
    assert!(serde_json::from_value::<CreateCheckoutRequest>(noted).is_err());
}
