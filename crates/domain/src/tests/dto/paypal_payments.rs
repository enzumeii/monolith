use super::*;
use serde_json::json;

#[test]
fn create_preserves_exact_decimal_strings_and_rejects_injected_provider_fields() {
    let body = json!({"idempotency_key":"a6af1d7a-b8e2-423b-93f7-30b1c69f030d","reference":"opaque","amount":"90071992547409.91","currency":"USD"});
    let request: CreatePaypalPaymentRequest =
        serde_json::from_value(body.clone()).expect("request");
    assert_eq!(request.amount, "90071992547409.91");
    assert_eq!(serde_json::to_value(request).expect("JSON"), body);
    for injected in ["paypal_order_id", "capture_id", "return_url", "status"] {
        let mut body = body.clone();
        body[injected] = json!("untrusted");
        assert!(serde_json::from_value::<CreatePaypalPaymentRequest>(body).is_err());
    }
    let body = json!({"idempotency_key":"id","reference":"opaque","amount":1.01,"currency":"USD"});
    assert!(serde_json::from_value::<CreatePaypalPaymentRequest>(body).is_err());
}

#[test]
fn response_carries_safe_status_and_optional_evidence_without_capability() {
    let body = json!({"id":"id","status":"PENDING","approval_url":null,"paypal_order_id":"order","reference":"opaque","amount":"1.01","currency":"USD","capture_id":null,"exception":null});
    let response: PaypalPaymentResponse = serde_json::from_value(body.clone()).expect("response");
    assert_eq!(response.status, PaypalPaymentStatus::Pending);
    assert_eq!(serde_json::to_value(response).expect("JSON"), body);
    assert!(serde_json::from_str::<PaypalPaymentStatus>("\"APPROVED\"").is_err());
}
