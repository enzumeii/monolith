use super::super::accounting::{address, paypal_records, phone, sku_category};
use crate::models::paypal::TransactionDetail;

#[test]
fn maps_accounting_categories_from_sku_codes() {
    assert_eq!(sku_category("ABCKEY001"), "KEYCHAIN");
    assert_eq!(sku_category("ABCAPP001"), "APPAREL");
    assert_eq!(sku_category("ABCUNK001"), "");
    assert_eq!(sku_category("ABCAPP_IP"), "STOCK");
}

#[test]
fn missing_customer_address_is_empty_and_phone_is_numeric() {
    let order: crate::models::shopify::Order = serde_json::from_value(serde_json::json!({
        "id": "gid://shopify/Order/1",
        "name": "#1",
        "number": 1,
        "createdAt": "2026-10-01T00:00:00Z",
        "displayFulfillmentStatus": "UNFULFILLED",
        "tags": [],
        "currentTotalPriceSet": {"shopMoney": {"amount": "0.00", "currencyCode": "THB"}},
        "transactions": [],
        "shippingAddress": {"name": "Buyer", "address1": "1 Road", "address2": null, "city": "Bangkok", "zip": "10000", "countryCodeV2": "TH", "phone": "+66 81-234-5678"}
    })).expect("Shopify order fixture should deserialize");
    assert_eq!(address(&order), "1 Road, Bangkok, 10000");
    assert_eq!(phone(&order), 66_812_345_678);
}

#[test]
fn paypal_usd_checkout_uses_the_nearby_withdrawal_exchange_rate() {
    let details = vec![
        paypal_detail("withdrawal", "T0400", "2026-10-31T00:00:00Z", "THB", "0"),
        paypal_detail("thb", "T0201", "2026-10-30T00:00:00Z", "THB", "3600"),
        paypal_detail("usd", "T0201", "2026-10-29T00:00:00Z", "USD", "-100"),
        paypal_detail("sale", "T0006", "2026-10-01T00:00:00Z", "USD", "10"),
    ];
    let records = paypal_records(details, 10);
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].0.transaction.transaction_id, "sale");
    assert!((records[0].1 - 36.0).abs() < f64::EPSILON);
    assert_eq!(records[0].2.to_rfc3339(), "2026-10-31T00:00:00+00:00");
}

fn paypal_detail(
    id: &str,
    event: &str,
    date: &str,
    currency: &str,
    amount: &str,
) -> TransactionDetail {
    serde_json::from_value(serde_json::json!({
        "transaction_info": {
            "transaction_id": id,
            "transaction_event_code": event,
            "transaction_amount": {"currency_code": currency, "value": amount},
            "transaction_initiation_date": date,
            "transaction_updated_date": date,
            "transaction_status": "S",
            "invoice_id": null,
            "fee_amount": null,
            "shipping_amount": null
        }
    }))
    .expect("PayPal transaction fixture should deserialize")
}
