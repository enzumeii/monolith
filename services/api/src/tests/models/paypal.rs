#![allow(clippy::unwrap_used)]
use serde_json::json;

#[test]
fn transaction_models_preserve_decimal_precision_offsets_and_optional_sections() {
    let value = json!({"transaction_details":[{"transaction_info":{"transaction_id":"fixture-transaction","transaction_event_code":"T9999","transaction_status":"S","transaction_amount":{"currency_code":"THB","value":"-1234567890.0123456789"},"transaction_initiation_date":"2026-10-01T07:00:00+07:00","transaction_updated_date":"2026-10-01T00:01:00Z"},"shipping_info":{"address":{"country_code":"TH"}},"cart_info":{"item_details":[{"item_quantity":"2","item_unit_price":{"currency_code":"THB","value":"1.00"}}]}}],"page":2,"total_items":501,"total_pages":2});
    let response: crate::models::paypal::TransactionPage = serde_json::from_value(value).unwrap();
    let detail = &response.transaction_details[0];
    assert_eq!(
        detail.transaction.transaction_amount.value,
        "-1234567890.0123456789"
    );
    assert_eq!(
        detail.transaction.transaction_amount.currency_code.as_str(),
        "THB"
    );
    assert_eq!(
        detail.transaction.transaction_initiation_date.to_rfc3339(),
        "2026-10-01T07:00:00+07:00"
    );
    assert!(detail.payer.is_none());
    assert!(
        detail
            .shipping
            .as_ref()
            .unwrap()
            .address
            .as_ref()
            .unwrap()
            .line1
            .is_none()
    );
    assert_eq!(response.total_pages, 2);
}

#[test]
fn partial_cart_items_do_not_discard_transaction_reports() {
    let cart: crate::models::paypal::CartInfo = serde_json::from_value(json!({
        "item_details":[{"invoice_number":"fixture-invoice","item_name":"Shipping adjustment"}]
    }))
    .unwrap();
    let items = cart.item_details.unwrap();
    assert_eq!(items[0].invoice_number.as_deref(), Some("fixture-invoice"));
}

#[test]
fn paypal_protocol_enums_preserve_known_and_unknown_wire_codes() {
    use crate::models::paypal::TransactionEvent;
    let event: TransactionEvent = serde_json::from_value(json!("T0403")).unwrap();
    assert_eq!(event, TransactionEvent::ManualWithdrawal);
    for code in ["T9999", "T9998"] {
        let event: TransactionEvent = serde_json::from_value(json!(code)).unwrap();
        assert_eq!(event, TransactionEvent::Other(code.into()));
        assert_eq!(serde_json::to_value(event).unwrap(), json!(code));
    }
}

#[test]
fn paypal_known_event_codes_round_trip() {
    use crate::models::paypal::TransactionEvent;
    for code in [
        "T0400", "T0403", "T0201", "T0200", "T1107", "T2105", "T2106", "T1110", "T1111", "T1106",
        "T0114", "T0000", "T0011", "T0006",
    ] {
        let event: TransactionEvent = serde_json::from_value(json!(code)).unwrap();
        assert!(!matches!(event, TransactionEvent::Other(_)));
        assert_eq!(event.as_str(), code);
        assert_eq!(serde_json::to_value(event).unwrap(), json!(code));
    }
}

#[test]
fn paypal_experience_requires_shipping_and_encodes_pickup_policy() {
    use crate::models::paypal::ExperienceContext;
    assert!(
        serde_json::from_value::<ExperienceContext>(json!({
            "return_url":"https://example.test/return", "cancel_url":"https://example.test/cancel"
        }))
        .is_err()
    );
    let context = ExperienceContext {
        return_url: "https://example.test/return".into(),
        cancel_url: "https://example.test/cancel".into(),
        shipping_preference: "NO_SHIPPING".into(),
    };
    assert_eq!(
        serde_json::to_value(&context).unwrap()["shipping_preference"],
        "NO_SHIPPING"
    );
}

#[test]
fn paypal_empty_report_may_omit_transaction_details() {
    let response: crate::models::paypal::TransactionPage = serde_json::from_value(json!({
        "page":1, "total_items":0, "total_pages":0,
        "last_refreshed_datetime":"2026-10-25T06:00:00Z"
    }))
    .unwrap();
    assert!(response.transaction_details.is_empty());
    assert_eq!(response.page, 1);
    assert_eq!(response.total_items, 0);
    assert_eq!(response.total_pages, 0);
}

#[test]
fn paypal_missing_report_details_require_explicit_empty_first_page() {
    for value in [
        json!({"page":1,"total_items":1,"total_pages":1}),
        json!({"page":2,"total_items":0,"total_pages":0}),
        json!({"page":1,"total_items":0,"total_pages":1}),
        json!({"page":1,"total_items":1,"total_pages":0}),
        json!({"total_items":0,"total_pages":0}),
        json!({"page":1,"total_pages":0}),
        json!({"page":1,"total_items":0}),
        json!({"page":1,"total_items":0,"total_pages":null}),
        json!({"transaction_details":null,"page":1,"total_items":0,"total_pages":0}),
        json!({"transaction_details":{},"page":1,"total_items":0,"total_pages":0}),
        json!({"transaction_details":"private payer","page":1,"total_items":0,"total_pages":0}),
    ] {
        assert!(
            serde_json::from_value::<crate::models::paypal::TransactionPage>(value.clone())
                .is_err(),
            "accepted malformed report {value}"
        );
    }
}
