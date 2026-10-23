#![allow(clippy::unwrap_used)]
use super::*;
use serde_json::json;
use uuid::Uuid;

fn payment() -> PaypalPayment {
    PaypalPayment {
        id: Uuid::nil(),
        capability_digest: vec![0; 32],
        reference: "draft-ref".into(),
        amount: "10.00".into(),
        currency: "USD".into(),
        minor_units: 1000,
        merchant_id: "MERCHANT".into(),
        environment: "sandbox".into(),
        api_origin: "https://api-m.sandbox.paypal.com".into(),
        paypal_order_id: Some("ORDER".into()),
        approval_url: None,
        status: "PENDING".into(),
        create_request_id: Uuid::nil(),
        capture_request_id: Uuid::nil(),
        lease_token: None,
        lease_until: None,
        create_started_at: None,
        capture_started_at: None,
        verified_capture_id: None,
        verified_at: None,
        exception: None,
    }
}
fn capture() -> Capture {
    serde_json::from_value(json!({"id":"CAPTURE","status":"COMPLETED","amount":{"currency_code":"USD","value":"10.00"},"final_capture":true,"payee":{"merchant_id":"MERCHANT"},"custom_id":"draft-ref","supplementary_data":{"related_ids":{"order_id":"ORDER"}}})).unwrap()
}
fn order(capture: &Capture) -> Order {
    serde_json::from_value(json!({"id":"ORDER","status":"COMPLETED","purchase_units":[{"reference_id":Uuid::nil().to_string(),"custom_id":"draft-ref","amount":{"currency_code":"USD","value":"10.00"},"payee":{"merchant_id":"MERCHANT"},"payments":{"captures":[capture]}}]})).unwrap()
}
#[test]
fn exact_completed_evidence_is_returned() {
    let capture = capture();
    let evidence = verify(&payment(), &order(&capture), &capture).unwrap();
    assert_eq!(evidence.capture_id, "CAPTURE");
    assert_eq!(evidence.minor_units, 1000);
}
#[test]
fn missing_or_mismatched_trusted_capture_fields_never_verify() {
    let original = capture();
    let order = order(&original);
    for field in ["amount", "final_capture", "status", "id"] {
        let mut value = serde_json::to_value(&original).unwrap();
        value[field] = match field {
            "amount" => json!({"currency_code":"USD","value":"9.99"}),
            "status" => json!("PENDING"),
            "id" => json!("OTHER"),
            _ => serde_json::Value::Null,
        };
        let changed: Capture = serde_json::from_value(value).unwrap();
        assert!(verify(&payment(), &order, &changed).is_err(), "{field}");
    }
}
#[test]
fn multiple_partial_and_wrong_purchase_units_never_verify() {
    let capture = capture();
    for change in [
        "multiple",
        "partial",
        "merchant",
        "reference",
        "currency",
        "order",
    ] {
        let mut value = serde_json::to_value(order(&capture)).unwrap();
        match change {
            "multiple" => {
                value["purchase_units"][0]["payments"]["captures"] = json!([capture, capture]);
            }
            "partial" => {
                value["purchase_units"][0]["payments"]["captures"][0]["final_capture"] =
                    json!(false);
            }
            "merchant" => value["purchase_units"][0]["payee"]["merchant_id"] = json!("OTHER"),
            "reference" => value["purchase_units"][0]["custom_id"] = json!("other"),
            "currency" => value["purchase_units"][0]["amount"]["currency_code"] = json!("EUR"),
            _ => value["id"] = json!("OTHER"),
        }
        assert!(
            verify(
                &payment(),
                &serde_json::from_value(value).unwrap(),
                &capture
            )
            .is_err(),
            "{change}"
        );
    }
}

#[test]
fn optional_redundant_capture_identity_uses_authoritative_order_binding() {
    let mut capture = capture();
    let order = order(&capture);
    capture.payee = None;
    capture.custom_id = None;
    capture.supplementary_data = None;
    assert!(verify(&payment(), &order, &capture).is_ok());
    capture.supplementary_data = Some(crate::models::paypal::SupplementaryData {
        related_ids: crate::models::paypal::RelatedIds {
            order_id: Some("WRONG".into()),
            capture_id: None,
            authorization_id: None,
        },
    });
    assert!(verify(&payment(), &order, &capture).is_err());
    capture.supplementary_data = None;
    capture.custom_id = Some("wrong-ref".into());
    assert!(verify(&payment(), &order, &capture).is_err());
    capture.custom_id = None;
    capture.payee = Some(crate::models::paypal::Payee {
        merchant_id: "WRONG".into(),
    });
    assert!(verify(&payment(), &order, &capture).is_err());
}

fn rejects_embedded_identity(field: &str) {
    let original = capture();
    let mut fetched = original.clone();
    fetched.payee = None;
    fetched.custom_id = None;
    fetched.supplementary_data = None;
    let mut value = serde_json::to_value(order(&original)).unwrap();
    let embedded = &mut value["purchase_units"][0]["payments"]["captures"][0];
    match field {
        "payee" => embedded["payee"]["merchant_id"] = json!("WRONG"),
        "custom_id" => embedded["custom_id"] = json!("wrong-ref"),
        "order_id" => embedded["supplementary_data"]["related_ids"]["order_id"] = json!("WRONG"),
        "capture_id" => {
            embedded["supplementary_data"]["related_ids"]["capture_id"] = json!("WRONG");
        }
        _ => unreachable!(),
    }
    assert!(
        verify(
            &payment(),
            &serde_json::from_value(value).unwrap(),
            &fetched
        )
        .is_err(),
        "{field}"
    );
}

#[test]
fn embedded_payee_mismatch_cannot_hide_behind_absent_fetched_payee() {
    rejects_embedded_identity("payee");
}
#[test]
fn embedded_reference_mismatch_cannot_hide_behind_absent_fetched_reference() {
    rejects_embedded_identity("custom_id");
}
#[test]
fn embedded_order_mismatch_cannot_hide_behind_absent_fetched_linkage() {
    rejects_embedded_identity("order_id");
}
#[test]
fn embedded_capture_mismatch_cannot_hide_behind_absent_fetched_linkage() {
    rejects_embedded_identity("capture_id");
}
