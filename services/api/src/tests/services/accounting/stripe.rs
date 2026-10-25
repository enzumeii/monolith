#![allow(clippy::unwrap_used, clippy::float_cmp)]
use super::{amounts, exchange_rate, payment_session_id, reference};
use crate::models::stripe::BalanceTransaction;
use crate::{
    client::{
        stripe::StripeClient,
        tests::{Fixture, Reply},
    },
    config::StripeConfig,
};
use serde_json::json;

#[test]
fn settlement_amounts_are_not_converted_again() {
    let mut value = balance_value();
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
fn payment_session_ids_preserve_the_complete_opaque_reference() {
    assert_eq!(
        payment_session_id("Paid (gid://shopify/PaymentSession/12345)"),
        Some("12345".into())
    );
    assert_eq!(
        payment_session_id("Paid (gid://shopify/PaymentSession/session_12345)"),
        Some("session_12345".into())
    );
    assert_eq!(payment_session_id("not a session"), None);
}

#[tokio::test]
async fn card_payment_refunds_preserve_the_balance_payment_session_reference() {
    let fixture = Fixture::new(vec![Reply::json(
        json!({"id":"re_123","amount":1000,"created":1,"currency":"usd"}),
    )])
    .await;
    let client = StripeClient::new(&StripeConfig {
        checkout_api_version: "2026-08-26.dahlia".into(),
        base_url: fixture.url.to_string(),
        timeout_secs: 3,
        secret_key: Some("fixture-key".into()),
    })
    .unwrap();
    let mut value = balance_value();
    value["description"] = json!("REFUND FOR CHARGE (gid://shopify/PaymentSession/session_12345)");
    let balance = serde_json::from_value(value).unwrap();
    let (payment_reference, _, _, _, _) =
        reference(&client, &balance, "re_123", true).await.unwrap();
    assert_eq!(payment_reference, "session_12345");
}

fn balance_value() -> serde_json::Value {
    json!({"amount":360_000,"net":348_000,"fee":12_000,"fee_details":[{"amount":10_000,"currency":"thb","type":"stripe_fee"},{"amount":2000,"currency":"thb","type":"tax"}],"id":"txn_1","available_on":1,"created":1,"currency":"thb","object":"balance_transaction","reporting_category":"charge","source":"ch_1","status":"available","type":"charge","exchange_rate":36,"description":null})
}

#[test]
fn unknown_stripe_fee_kinds_do_not_become_processing_fees_or_vat() {
    let mut value = balance_value();
    value["fee_details"]
        .as_array_mut()
        .unwrap()
        .push(json!({"amount":900,"currency":"thb","type":"future_fee"}));
    let balance = serde_json::from_value(value).unwrap();
    assert_eq!(amounts(&balance).unwrap(), (3480.0, 100.0, 20.0, 3600.0));
}

#[tokio::test]
async fn unknown_stripe_balance_kind_is_skipped_without_fetching_its_source() {
    let mut value = balance_value();
    value["type"] = json!("future_type");
    value["source"] = json!(null);
    let fixture = Fixture::new(vec![Reply::json(json!({"data":[value], "has_more":false}))]).await;
    let client = StripeClient::new(&StripeConfig {
        checkout_api_version: "2026-08-26.dahlia".into(),
        base_url: fixture.url.to_string(),
        timeout_secs: 3,
        secret_key: Some("fixture-key".into()),
    })
    .unwrap();
    let payout = crate::models::stripe::Payout {
        id: "po_fixture".into(),
        arrival_date: 1,
    };
    let payments = super::collect_payout(&client, &payout).await.unwrap();
    assert!(payments.is_empty());
}

#[test]
fn external_stripe_source_money_requires_successful_full_capture() {
    let original = json!({"id":"ch_external", "amount":400,"status":"succeeded", "amount_captured":400,"amount_refunded":0,
        "captured":true,"paid":true,"refunded":false,"created":1,"currency":"aud","disputed":false,"metadata":{}});
    let charge = serde_json::from_value(original.clone()).unwrap();
    let source = super::source_money(&charge).unwrap().unwrap();
    assert_eq!(source.minor, 400);
    assert_eq!(source.currency, "AUD");
    for (key, replacement) in [
        ("paid", json!(false)),
        ("captured", json!(false)),
        ("status", json!("pending")),
        ("status", json!(null)),
        ("amount", json!(null)),
        ("amount_captured", json!(399)),
        ("amount_refunded", json!(1)),
        ("disputed", json!(true)),
        ("amount", json!(0)),
    ] {
        let mut value = original.clone();
        value[key] = replacement;
        assert!(
            super::source_money(&serde_json::from_value(value).unwrap())
                .unwrap()
                .is_none(),
            "{key}"
        );
    }
}
