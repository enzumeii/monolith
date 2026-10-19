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
    assert_eq!(detail.transaction.transaction_amount.currency_code, "THB");
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
