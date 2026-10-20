#![allow(clippy::unwrap_used, clippy::float_cmp)]
use super::{amounts, exchange_rate, payment_session_id};
use crate::models::stripe::BalanceTransaction;
use serde_json::json;

#[test]
fn settlement_amounts_are_not_converted_again() {
    let mut value = json!({"amount":360_000,"net":348_000,"fee":12_000,"fee_details":[{"amount":10_000,"currency":"thb","type":"stripe_fee"},{"amount":2000,"currency":"thb","type":"tax"}],"id":"txn_1","available_on":1,"created":1,"currency":"thb","object":"balance_transaction","reporting_category":"charge","source":"ch_1","status":"available","type":"charge","exchange_rate":36,"description":null});
    let balance: BalanceTransaction = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(amounts(&balance).unwrap(), (3480.0, 100.0, 20.0, 3600.0));
    value["amount"] = json!(-36_000);
    value["net"] = json!(-36_000);
    value["fee_details"] = json!([]);
    value["exchange_rate"] = json!(null);
    assert_eq!(
        amounts(&serde_json::from_value(value.clone()).unwrap()).unwrap(),
        (-360.0, 0.0, 0.0, -360.0)
    );
    let no_rate: BalanceTransaction = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(exchange_rate(&no_rate, "THB").unwrap(), 1.0);
    assert!(exchange_rate(&no_rate, "USD").is_err());
    value["currency"] = json!("usd");
    assert!(amounts(&serde_json::from_value(value.clone()).unwrap()).is_err());
    value["currency"] = json!("thb");
    value["fee_details"] = json!([{"amount":100,"currency":"usd","type":"stripe_fee"}]);
    assert!(amounts(&serde_json::from_value(value).unwrap()).is_err());
}
#[test]
fn payment_session_ids_are_literal_numeric_references() {
    assert_eq!(
        payment_session_id("Paid (gid://shopify/PaymentSession/12345)"),
        Some("12345".into())
    );
    assert_eq!(payment_session_id("not a session"), None);
}
