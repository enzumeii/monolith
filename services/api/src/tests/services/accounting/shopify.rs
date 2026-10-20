#![allow(clippy::unwrap_used, clippy::float_cmp)]
use super::{address, apply, complete, phone, reference, sku_category, thb};
use crate::{
    models::shopify::{Money, OrderTransaction},
    services::accounting::tests::{order, order_value, page, payment, transaction},
};
use domain::dto::{AccountingGateway, AccountingKind};
use serde_json::json;

#[test]
fn all_legacy_gateway_references_use_receipt_fields() {
    for (gateway, kind, receipt, expected) in [
        (
            "stripe",
            "CAPTURE",
            json!({"charges":{"data":[{"id":"charge"}]}}),
            "charge",
        ),
        ("stripe", "REFUND", json!({"id":"refund"}), "refund"),
        ("paypal", "SALE", json!({"transaction_id":"sale"}), "sale"),
        (
            "paypal",
            "REFUND",
            json!({"refund_transaction_id":"refund"}),
            "refund",
        ),
        (
            "Stripe Card Payments",
            "CAPTURE",
            json!({}),
            "different-payment-id",
        ),
    ] {
        let tx: OrderTransaction =
            serde_json::from_value(transaction(gateway, kind, receipt)).unwrap();
        assert_eq!(reference(&tx).unwrap().unwrap().1, expected);
    }
    let mut value = transaction("paypal", "SALE", json!({}));
    assert!(
        reference(&serde_json::from_value(value.clone()).unwrap())
            .unwrap()
            .is_none()
    );
    value["receiptJson"] = json!("{");
    assert!(reference(&serde_json::from_value(value).unwrap()).is_err());
}
#[test]
fn sales_ship_once_and_use_the_receipt_reference() {
    let mut payment = payment(AccountingGateway::Stripe, AccountingKind::Sale, "ch_1");
    let mut items = Vec::new();
    apply(&mut payment, &order(), &mut items).unwrap();
    assert_eq!(payment.transaction.shipping_fee, Some(180.0));
    assert_eq!(payment.transaction.shopify_order_id, 1);
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].price, 95.0);
    assert_eq!(address(&order()), "1 Road, Bangkok, 10000");
    assert_eq!(phone(&order()), 66_812_345_678);
    assert_eq!(sku_category("ABCKEY001"), "KEYCHAIN");
    assert_eq!(sku_category("ABCAPP001"), "APPAREL");
    assert_eq!(sku_category("ABCUNK001"), "");
    assert_eq!(sku_category("ABCAPP_IP"), "STOCK");
}
#[test]
fn shipping_currency_is_converted_only_when_compatible() {
    let payment = payment(AccountingGateway::Stripe, AccountingKind::Sale, "ch_1");
    for (currency, expected) in [("THB", 5.0), ("USD", 180.0)] {
        assert_eq!(
            thb(
                &Money {
                    amount: "5".into(),
                    currency_code: currency.into()
                },
                &payment
            )
            .unwrap(),
            expected
        );
    }
    assert!(
        thb(
            &Money {
                amount: "5".into(),
                currency_code: "EUR".into()
            },
            &payment
        )
        .is_err()
    );
}
#[test]
fn refund_connection_controls_shipping_and_item_allocation() {
    let mut value = order_value();
    value["refunds"] = json!([{
        "transactions":page(vec![transaction("stripe","REFUND",json!({"id":"re_1"}))]),
        "refundShippingLines":page(vec![json!({"subtotalAmountSet":{"shopMoney":{"amount":"1","currencyCode":"USD"}}})]),
        "refundLineItems":page(vec![json!({"quantity":1,"subtotalSet":{"shopMoney":{"amount":"10","currencyCode":"USD"}},"lineItem":value["lineItems"]["nodes"][0]})])
    }]);
    let order = serde_json::from_value(value.clone()).unwrap();
    let mut payment = payment(AccountingGateway::Stripe, AccountingKind::Refund, "re_1");
    let mut items = Vec::new();
    apply(&mut payment, &order, &mut items).unwrap();
    assert_eq!(payment.transaction.shipping_fee, Some(-36.0));
    assert_eq!(items[0].quantity, -1);
    assert_eq!(items[0].price, -10.0);
    for path in [
        "/lineItems/pageInfo/hasNextPage",
        "/shippingLines/pageInfo/hasNextPage",
        "/refunds/0/transactions/pageInfo/hasNextPage",
        "/refunds/0/refundShippingLines/pageInfo/hasNextPage",
        "/refunds/0/refundLineItems/pageInfo/hasNextPage",
    ] {
        let mut incomplete = value.clone();
        *incomplete.pointer_mut(path).unwrap() = json!(true);
        assert!(complete(&serde_json::from_value(incomplete).unwrap()).is_err());
    }
}
#[test]
fn partial_payment_items_and_shipping_are_allocated_once() {
    let mut value = order_value();
    value["transactions"][0]["amountSet"]["shopMoney"]["amount"] = json!("40");
    let mut second = transaction(
        "stripe",
        "CAPTURE",
        json!({"charges":{"data":[{"id":"ch_2"}]}}),
    );
    second["amountSet"]["shopMoney"]["amount"] = json!("60");
    value["transactions"].as_array_mut().unwrap().push(second);
    let order = serde_json::from_value(value).unwrap();
    let mut first = payment(AccountingGateway::Stripe, AccountingKind::Sale, "ch_1");
    let mut second = payment(AccountingGateway::Stripe, AccountingKind::Sale, "ch_2");
    let mut items = Vec::new();
    apply(&mut first, &order, &mut items).unwrap();
    apply(&mut second, &order, &mut items).unwrap();
    assert!(first.transaction.order_adjusted);
    assert_eq!(first.transaction.shipping_fee, Some(180.0));
    assert_eq!(second.transaction.shipping_fee, Some(0.0));
    assert_eq!(items.len(), 1);
}

#[test]
fn paypal_shipping_prefers_provider_and_never_uses_another_currency() {
    let mut value = order_value();
    value["transactions"][0] = transaction("paypal", "SALE", json!({"transaction_id":"pp_1"}));
    let mut row = payment(AccountingGateway::Paypal, AccountingKind::Sale, "pp_1");
    row.transaction.currency = "AUD".into();
    row.transaction.exchange_rate = None;
    row.transaction.shipping_fee = Some(10.08);
    apply(
        &mut row,
        &serde_json::from_value(value.clone()).unwrap(),
        &mut Vec::new(),
    )
    .unwrap();
    assert_eq!(row.transaction.shipping_fee, Some(10.08));
    row.transaction.shipping_fee = Some(0.0);
    apply(
        &mut row,
        &serde_json::from_value(value.clone()).unwrap(),
        &mut Vec::new(),
    )
    .unwrap();
    assert_eq!(row.transaction.shipping_fee, Some(0.0));
    row.transaction.shipping_fee = None;
    apply(
        &mut row,
        &serde_json::from_value(value.clone()).unwrap(),
        &mut Vec::new(),
    )
    .unwrap();
    assert_eq!(row.transaction.shipping_fee, None);
    value["shippingLines"]["nodes"][0]["priceSet"]["shopMoney"]["currencyCode"] = json!("AUD");
    apply(
        &mut row,
        &serde_json::from_value(value).unwrap(),
        &mut Vec::new(),
    )
    .unwrap();
    assert_eq!(row.transaction.shipping_fee, Some(5.0));
}

#[test]
fn report_sku_category_uses_character_positions() {
    assert_eq!(sku_category("กขคAPP001"), "APPAREL");
    assert_eq!(sku_category("กขคKEY001"), "KEYCHAIN");
    assert_eq!(sku_category("กขคAPP_SS"), "STOCK");
    assert_eq!(sku_category("短"), "");
}

#[test]
fn report_contact_preserves_locality_unicode_and_numeric_phone_policy() {
    let mut value = order_value();
    value["shippingAddress"] = json!({"name":"Name","address1":" บ้าน 1 ","address2":" ","city":" กรุงเทพ ","provinceCode":" TH-10 ","country":" Thailand ","zip":" 00100 ","phone":"00 (081) ٢3-45"});
    let order = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(address(&order), "บ้าน 1, กรุงเทพ, TH-10, Thailand, 00100");
    assert_eq!(phone(&order), 81345);
    value["shippingAddress"]["phone"] = json!("18446744073709551616");
    assert_eq!(phone(&serde_json::from_value(value).unwrap()), 0);
}

#[test]
fn report_category_labels_keep_each_product_group() {
    for (code, expected) in [
        ("APP", "APPAREL"),
        ("BBT", "APPAREL"),
        ("TEE", "APPAREL"),
        ("TOT", "BAG"),
        ("BLK", "BLANKET"),
        ("QCS", "PHONE_ACCESSORY"),
        ("CSE", "PHONE_ACCESSORY"),
        ("GTK", "PHONE_ACCESSORY"),
        ("PSR", "PHONE_ACCESSORY"),
        ("CLC", "CLAWCLIP"),
        ("PIN", "BADGE"),
        ("KEY", "KEYCHAIN"),
        ("QSK", "KEYCHAIN"),
        ("LNK", "KEYCHAIN"),
        ("WDK", "KEYCHAIN"),
        ("KMB", "KEYCHAIN"),
        ("LQD", "KEYCHAIN"),
        ("CRB", "CARABINER"),
        ("PSH", "PLUSHIE"),
        ("STD", "STANDEE"),
        ("STS", "STICKER_SHEET"),
        ("PTC", "STICKER_SHEET"),
        ("PCT", "PHOTOCARD"),
        ("MAG", "MAGNET"),
        ("STK", "STICKER"),
        ("BST", "STICKER"),
        ("ZPB", "ZIPPER_BAG"),
        ("BDS", "BEAD"),
        ("CLP", "HAIRCLIP"),
        ("LYD", "LANYARD"),
        ("CHD", "CARDHOLDER"),
        ("UNK", ""),
    ] {
        assert_eq!(sku_category(&format!("ABC{code}001")), expected);
    }
    for marker in ["_IP", "_SS", "_XM"] {
        assert_eq!(sku_category(&format!("ABCBLK{marker}001")), "STOCK");
    }
}

#[test]
fn unmatched_sale_allocates_neither_shipping_nor_items() {
    let mut row = payment(AccountingGateway::Stripe, AccountingKind::Sale, "missing");
    let mut items = Vec::new();
    apply(&mut row, &order(), &mut items).unwrap();
    assert_eq!(row.transaction.shipping_fee, Some(0.0));
    assert!(!row.transaction.order_adjusted);
    assert!(items.is_empty());
}
