use super::*;
use domain::dto::accounting::{AccountingGateway, AccountingKind};
fn transaction(
    gateway: AccountingGateway,
    kind: AccountingKind,
    reference: &str,
) -> AccountingTransaction {
    AccountingTransaction {
        gateway,
        kind,
        po_number: "po_1".into(),
        po_date: None,
        transaction_date: "2026-10-01T00:00:00+07:00".into(),
        payment_reference: reference.into(),
        description: "商品, \"quoted\"\r\nnext".into(),
        shopify_order_id: u64::MAX,
        shopify_order_number: -42,
        customer_name: "=Alice".into(),
        customer_address: "Bangkok".into(),
        customer_phone: 66_123_456_789,
        currency: "AUD".into(),
        exchange_rate: None,
        net_paid: -50.26,
        fee: 2.82,
        vat: 0.,
        gross_paid: -53.08,
        shipping_fee: None,
        order_adjusted: false,
    }
}
#[test]
fn transaction_columns_preserve_every_value_and_csv_nulls() {
    let mut row = transaction(AccountingGateway::Paypal, AccountingKind::Refund, "pp_1");
    let table = transaction_table([&row], "null");
    assert_eq!(
        table.headers,
        &[
            "gateway",
            "type",
            "po_number",
            "po_date",
            "transaction_date",
            "payment_reference",
            "description",
            "shopify_order_id",
            "shopify_order_number",
            "customer_name",
            "customer_address",
            "customer_phone",
            "currency",
            "exchange_rate",
            "net_paid",
            "fee",
            "vat",
            "gross_paid",
            "shipping_fee",
            "order_adjusted"
        ]
    );
    assert_eq!(
        table.rows[0],
        [
            "PAYPAL",
            "REFUND",
            "po_1",
            "null",
            "2026-10-01T00:00:00+07:00",
            "pp_1",
            "商品, \"quoted\"\r\nnext",
            "18446744073709551615",
            "-42",
            "=Alice",
            "Bangkok",
            "66123456789",
            "AUD",
            "null",
            "-50.26",
            "2.82",
            "0",
            "-53.08",
            "null",
            "false"
        ]
    );
    let csv_table = transaction_table([&row], "");
    for column in [3, 13, 18] {
        assert_eq!(csv_table.rows[0][column], "");
    }
    row.po_date = Some("2026-10-02T01:02:03Z".into());
    row.exchange_rate = Some(23.123_456);
    row.shipping_fee = Some(10.08);
    row.order_adjusted = true;
    let table = transaction_table([&row], "null");
    assert_eq!(table.rows[0][3], "2026-10-02T01:02:03Z");
    assert_eq!(table.rows[0][13], "23.123456");
    assert_eq!(table.rows[0][18], "10.08");
    assert_eq!(table.rows[0][19], "true");
}
#[test]
fn consolidated_csv_preserves_provider_order_and_escapes_text() {
    let stripe = [
        transaction(AccountingGateway::Stripe, AccountingKind::Sale, "st_second"),
        transaction(
            AccountingGateway::Stripe,
            AccountingKind::Refund,
            "st_first",
        ),
    ];
    let paypal = [transaction(
        AccountingGateway::Paypal,
        AccountingKind::Sale,
        "pp_last",
    )];
    let table = transaction_table(stripe.iter().chain(&paypal), "");
    assert_eq!(
        table
            .rows
            .iter()
            .map(|row| row[5].as_str())
            .collect::<Vec<_>>(),
        ["st_second", "st_first", "pp_last"]
    );
    assert_eq!(table.rows[0][0], "STRIPE");
    assert_eq!(table.rows[0][1], "SALE");
    let csv = crate::pages::reports::encode_table(table.headers, &table.rows);
    assert!(csv.starts_with(
        "gateway,type,po_number,po_date,transaction_date,payment_reference,description,"
    ));
    assert!(csv.contains("\"商品, \"\"quoted\"\"\r\nnext\""));
    assert!(!csv.contains("quantity"));
}
#[test]
fn items_preserve_all_fields_in_backend_order() {
    let rows = [AccountingItem {
        quantity: -2,
        price: 12.345,
        currency: "THB".into(),
        title: "Item".into(),
        variant: String::new(),
        category: "Goods".into(),
        sku: "00042".into(),
        kind: AccountingKind::Refund,
        transaction_date: "2026-10-01".into(),
        shopify_order_id: u64::MAX,
        payment_reference: "opaque_ref".into(),
    }];
    let table = item_table(&rows);
    assert_eq!(
        table.headers,
        &[
            "quantity",
            "price",
            "currency",
            "title",
            "variant",
            "category",
            "sku",
            "kind",
            "transaction_date",
            "shopify_order_id",
            "payment_reference"
        ]
    );
    assert_eq!(
        table.rows[0],
        [
            "-2",
            "12.345",
            "THB",
            "Item",
            "",
            "Goods",
            "00042",
            "REFUND",
            "2026-10-01",
            "18446744073709551615",
            "opaque_ref"
        ]
    );
    assert_eq!(
        item_table(&[]).rows,
        [] as [std::vec::Vec<std::string::String>; 0]
    );
    assert_eq!(
        transaction_table([], "null").rows,
        [] as [std::vec::Vec<std::string::String>; 0]
    );
}
