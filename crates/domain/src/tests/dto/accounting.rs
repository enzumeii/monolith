use crate::dto::accounting::{
    AccountingGateway, AccountingItem, AccountingKind, AccountingReportResponse,
    AccountingTransaction, AccountingTransactions,
};
use serde_json::json;

#[test]
fn accounting_report_serializes_the_issue_wire_shape() {
    let response = AccountingReportResponse {
        transactions: AccountingTransactions {
            stripe: vec![],
            paypal: vec![AccountingTransaction {
                gateway: AccountingGateway::Paypal,
                kind: AccountingKind::Sale,
                po_number: "po_1".into(),
                po_date: None,
                transaction_date: "2026-10-01T00:00:00+07:00".into(),
                payment_reference: "txn_1".into(),
                description: "PAID TO buyer@example.test in currency THB".into(),
                shopify_order_id: 77_692_634_234,
                shopify_order_number: 1234,
                customer_name: "Buyer".into(),
                customer_address: "Bangkok".into(),
                customer_phone: 782_634_123,
                currency: "THB".into(),
                exchange_rate: None,
                net_paid: 100.0,
                fee: 3.0,
                vat: 0.0,
                gross_paid: 103.0,
                shipping_fee: Some(5.0),
                order_adjusted: false,
                shopify_order_tags: vec![
                    "Con,vention".into(),
                    "商品\"tag".into(),
                    "=SUM(1,2)".into(),
                ],
            }],
        },
        items: vec![AccountingItem {
            quantity: 1,
            price: 12.5,
            currency: "USD".into(),
            title: "Keychain".into(),
            variant: "Blue".into(),
            category: "KEYCHAIN".into(),
            sku: "ABCSSSXXX01".into(),
            kind: AccountingKind::Sale,
            transaction_date: "2026-10-01T00:00:00+07:00".into(),
            shopify_order_id: 77_692_634_234,
            payment_reference: "txn_1".into(),
        }],
    };

    let serialized = serde_json::to_value(response).expect("serialize accounting report");
    let decoded: AccountingReportResponse =
        serde_json::from_value(serialized.clone()).expect("decode tagged report");
    assert_eq!(
        decoded.transactions.paypal[0].shopify_order_tags,
        ["Con,vention", "商品\"tag", "=SUM(1,2)"]
    );
    let mut cached = serialized.clone();
    cached["transactions"]["paypal"][0]
        .as_object_mut()
        .expect("transaction")
        .remove("shopify_order_tags");
    let decoded: AccountingReportResponse =
        serde_json::from_value(cached).expect("decode older cached report");
    assert_eq!(
        decoded.transactions.paypal[0].shopify_order_tags,
        [] as [String; 0]
    );
    assert_eq!(
        serialized,
        json!({
            "transactions": {"stripe":[],"paypal": [{
                "gateway": "PAYPAL", "type": "SALE", "po_number": "po_1",
                "po_date": null, "transaction_date": "2026-10-01T00:00:00+07:00",
                "payment_reference": "txn_1", "description": "PAID TO buyer@example.test in currency THB",
                "shopify_order_id": 77_692_634_234_u64, "shopify_order_number": 1234, "customer_name": "Buyer",
                "customer_address": "Bangkok", "customer_phone": 782_634_123_u64, "currency":"THB", "exchange_rate": null,
                "net_paid": 100.0, "fee": 3.0, "vat": 0.0, "gross_paid": 103.0,
                "shipping_fee": 5.0, "order_adjusted": false,
                "shopify_order_tags": ["Con,vention", "商品\"tag", "=SUM(1,2)"]
            }]},
            "items": [{
                "quantity": 1, "price": 12.5, "currency": "USD", "title": "Keychain",
                "variant": "Blue", "category": "KEYCHAIN", "sku": "ABCSSSXXX01", "kind": "SALE",
                "transaction_date": "2026-10-01T00:00:00+07:00", "shopify_order_id": 77_692_634_234_u64,
                "payment_reference": "txn_1"
            }]
        })
    );
}

#[test]
fn empty_accounting_report_has_two_provider_arrays() {
    let decoded: AccountingReportResponse =
        serde_json::from_value(json!({"transactions":{"stripe":[],"paypal":[]},"items":[]}))
            .expect("decode empty accounting report");
    assert_eq!(
        serde_json::to_value(decoded).expect("serialize empty accounting report")["transactions"],
        json!({"stripe":[],"paypal":[]})
    );
}
