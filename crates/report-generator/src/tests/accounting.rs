use super::*;

#[test]
fn transaction_csv_preserves_schema_numbers_ids_and_formula_guards() {
    let mut row = AccountingTransaction {
        gateway: AccountingGateway::Stripe,
        kind: AccountingKind::Refund,
        po_number: "-PO".into(),
        po_date: None,
        transaction_date: "2026-10-22".into(),
        payment_reference: "@ref".into(),
        description: "A, \"quote\"\nline".into(),
        shopify_order_id: 1_234_567_890_123_456_789,
        shopify_order_number: -123,
        customer_name: "+name".into(),
        customer_address: "=address".into(),
        customer_phone: 66_812_345_678,
        currency: "THB".into(),
        exchange_rate: Some(-1.5),
        net_paid: -100.5,
        fee: -2.5,
        vat: -0.5,
        gross_paid: -103.5,
        shipping_fee: Some(-10.0),
        order_adjusted: false,
        shopify_order_tags: vec![],
    };
    assert_eq!(
        encode_transactions([&row]),
        "gateway,type,po_number,po_date,transaction_date,payment_reference,description,shopify_order_id,shopify_order_number,customer_name,customer_address,customer_phone,currency,exchange_rate,net_paid,fee,vat,gross_paid,shipping_fee,order_adjusted,shopify_order_tags\r\nSTRIPE,REFUND,'-PO,,2026-10-22,'@ref,\"A, \"\"quote\"\"\nline\",1234567890123456789,-123,'+name,'=address,66812345678,THB,-1.5,-100.5,-2.5,-0.5,-103.5,-10,false,[]\r\n"
    );
    row.shopify_order_tags = vec![
        "Con,vention".into(),
        "商品\"tag".into(),
        "=SUM(1,2)".into(),
        "@tag".into(),
    ];
    assert_eq!(
        transaction_table([&row], "").rows[0][20],
        r#"["Con,vention","商品\"tag","=SUM(1,2)","@tag"]"#,
    );
    assert!(encode_transactions([&row]).ends_with(
        "false,\"[\"\"Con,vention\"\",\"\"商品\\\"\"tag\"\",\"\"=SUM(1,2)\"\",\"\"@tag\"\"]\"\r\n"
    ));
}

#[test]
fn item_csv_preserves_schema_negative_numbers_and_guards_sku() {
    let row = AccountingItem {
        quantity: -2,
        price: -12.5,
        currency: "THB".into(),
        title: "=title".into(),
        variant: "+variant".into(),
        category: "@category".into(),
        sku: "-SKU".into(),
        kind: AccountingKind::Refund,
        transaction_date: "2026-10-22".into(),
        shopify_order_id: 1_234_567_890_123_456_789,
        payment_reference: "ref".into(),
    };
    assert_eq!(
        encode_items(&[row]),
        "quantity,price,currency,title,variant,category,sku,kind,transaction_date,shopify_order_id,payment_reference\r\n-2,-12.5,THB,'=title,'+variant,'@category,'-SKU,REFUND,2026-10-22,1234567890123456789,ref\r\n"
    );
}
