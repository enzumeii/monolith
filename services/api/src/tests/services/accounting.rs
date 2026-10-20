#![allow(clippy::unwrap_used, clippy::float_cmp, clippy::needless_pass_by_value)]
use super::super::accounting::{Payment, date, empty_transaction, money};
use crate::models::shopify::Order;
use domain::dto::{AccountingGateway, AccountingKind};
use serde_json::{Value, json};

#[test]
fn accounting_numbers_and_dates_reject_unusable_values() {
    for value in ["NaN", "infinity", "invalid"] {
        assert!(money(value).is_err());
    }
    assert!(date("not-a-date").is_err());
    assert_eq!(money("12.50").unwrap(), 12.5);
}

pub(super) fn page(nodes: Vec<Value>) -> Value {
    json!({"nodes": nodes, "pageInfo":{"hasNextPage":false,"endCursor":null}})
}
pub(super) fn payment(
    gateway: AccountingGateway,
    kind: AccountingKind,
    reference: &str,
) -> Payment {
    let mut transaction = empty_transaction(gateway, kind);
    transaction.payment_reference = reference.into();
    transaction.transaction_date = "2026-10-02T00:00:00Z".into();
    transaction.exchange_rate = 36.0;
    Payment {
        transaction,
        currency: "USD".into(),
    }
}
pub(super) fn order_value() -> Value {
    json!({
        "id":"gid://shopify/Order/1", "name":"#1", "number":1, "createdAt":"2026-10-01T00:00:00Z",
        "displayFulfillmentStatus":"UNFULFILLED","tags":[],
        "currentTotalPriceSet":{"shopMoney":{"amount":"100","currencyCode":"USD"}},
        "shippingAddress":{"name":"Buyer","address1":"1 Road","city":"Bangkok","zip":"10000","phone":"+66 81-234-5678"},
        "transactions":[transaction("stripe","CAPTURE",json!({"charges":{"data":[{"id":"ch_1"}]}}))],
        "lineItems":page(vec![json!({"title":"Keychain","variantTitle":"Blue","sku":"ABCKEY001","quantity":1,"originalUnitPriceSet":{"shopMoney":{"amount":"95","currencyCode":"USD"}}})]),
        "shippingLines":page(vec![json!({"priceSet":{"shopMoney":{"amount":"5","currencyCode":"USD"}}})]),
        "refunds":[]
    })
}
pub(super) fn order() -> Order {
    serde_json::from_value(order_value()).unwrap()
}
pub(super) fn transaction(gateway: &str, kind: &str, receipt: Value) -> Value {
    json!({"id":"gid://shopify/OrderTransaction/1","paymentId":"different-payment-id","gateway":gateway,"kind":kind,"status":"SUCCESS","processedAt":"2026-10-02T00:00:00Z","amountSet":{"shopMoney":{"amount":"100","currencyCode":"USD"}},"receiptJson":receipt.to_string()})
}
