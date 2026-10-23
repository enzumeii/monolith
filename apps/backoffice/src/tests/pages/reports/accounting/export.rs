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
        shopify_order_tags: vec![],
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
            "order_adjusted",
            "shopify_order_tags"
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
            "false",
            "[]"
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
    let csv = crate::export::encode_table(table.headers, &table.rows);
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

#[test]
fn archive_contains_transaction_and_item_csv_files() {
    use std::io::{Cursor, Read};
    let mut rows = [
        transaction(AccountingGateway::Stripe, AccountingKind::Sale, "st_sale"),
        transaction(
            AccountingGateway::Paypal,
            AccountingKind::Refund,
            "pp_refund",
        ),
    ];
    rows[0].shopify_order_tags = vec![
        "Con,vention".into(),
        "商品\"tag".into(),
        "=SUM(1,2)".into(),
        "@tag".into(),
    ];
    let transactions = transaction_table(&rows, "");
    let items = TableData {
        headers: &["quantity", "title", "kind", "sku"],
        rows: vec![
            vec![
                "2".into(),
                "商品, \"quoted\"\r\nnext".into(),
                "SALE".into(),
                "00042".into(),
            ],
            vec![
                "-1".into(),
                "Returned item".into(),
                "REFUND".into(),
                "00042".into(),
            ],
        ],
    };
    let bytes = encode_archive(&transactions, &items).expect("report archive");
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).expect("valid ZIP");
    assert_eq!(archive.len(), 2);
    let mut transaction_csv = String::new();
    archive
        .by_name("transactions.csv")
        .expect("transactions")
        .read_to_string(&mut transaction_csv)
        .expect("UTF-8");
    assert!(transaction_csv.starts_with("gateway,type,po_number,po_date,transaction_date,"));
    assert!(
        transaction_csv.find("st_sale").expect("stripe")
            < transaction_csv.find("pp_refund").expect("paypal")
    );
    assert!(
        transaction_csv
            .lines()
            .next()
            .expect("header")
            .ends_with(",order_adjusted,shopify_order_tags")
    );
    assert!(transaction_csv.contains(
        "false,\"[\"\"Con,vention\"\",\"\"商品\\\"\"tag\"\",\"\"=SUM(1,2)\"\",\"\"@tag\"\"]\"\r\n"
    ));
    assert!(transaction_csv.ends_with("false,[]\r\n"));
    let mut item_csv = String::new();
    archive
        .by_name("items.csv")
        .expect("items")
        .read_to_string(&mut item_csv)
        .expect("UTF-8");
    assert_eq!(
        item_csv,
        "quantity,title,kind,sku\r\n2,\"商品, \"\"quoted\"\"\r\nnext\",SALE,00042\r\n-1,Returned item,REFUND,00042\r\n"
    );
}
#[test]
fn archive_includes_header_only_csv_for_an_empty_section() {
    use std::io::{Cursor, Read};
    let empty_transactions = transaction_table([], "");
    let empty_items = item_table(&[]);
    let bytes = encode_archive(&empty_transactions, &empty_items).expect("empty archive");
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).expect("ZIP");
    let mut csv = String::new();
    archive
        .by_name("items.csv")
        .expect("items entry")
        .read_to_string(&mut csv)
        .expect("CSV");
    assert_eq!(
        csv,
        "quantity,price,currency,title,variant,category,sku,kind,transaction_date,shopify_order_id,payment_reference\r\n"
    );
}
