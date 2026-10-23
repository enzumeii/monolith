#![allow(clippy::unwrap_used, clippy::float_cmp)]
use super::{address, apply, complete, phone, reference, thb};
use crate::{
    models::{
        SkuCategory,
        shopify::{Money, OrderTransaction},
    },
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
    assert_eq!(SkuCategory::from_sku("ABCKEY001").as_str(), "KEYCHAIN");
    assert_eq!(SkuCategory::from_sku("ABCAPP001").as_str(), "APPAREL");
    assert_eq!(SkuCategory::from_sku("ABCUNK001").as_str(), "");
    assert_eq!(SkuCategory::from_sku("ABCAPP_IP").as_str(), "STOCK");
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
fn unmatched_sale_allocates_neither_shipping_nor_items() {
    let mut row = payment(AccountingGateway::Stripe, AccountingKind::Sale, "missing");
    let mut items = Vec::new();
    apply(&mut row, &order(), &mut items).unwrap();
    assert_eq!(row.transaction.shipping_fee, Some(0.0));
    assert!(!row.transaction.order_adjusted);
    assert!(items.is_empty());
}

#[test]
fn native_sales_preserve_actual_order_tags() {
    let mut value = order_value();
    value["tags"] = json!(["Convention", "comma,quote\"", "ไทย"]);
    let mut row = payment(AccountingGateway::Stripe, AccountingKind::Sale, "ch_1");
    apply(
        &mut row,
        &serde_json::from_value(value).unwrap(),
        &mut Vec::new(),
    )
    .unwrap();
    assert_eq!(
        row.transaction.shopify_order_tags,
        ["Convention", "comma,quote\"", "ไทย"]
    );
}

fn checkout_snapshot() -> crate::models::checkout_completion::Snapshot {
    serde_json::from_value(json!({
        "draft_id":"gid://shopify/DraftOrder/1", "accepted":{"minor":9500,"currency":"USD","precision":2},
        "lines":[{"variant_id":"gid://shopify/ProductVariant/1","quantity":1,"unit_price":{"minor":9500,"currency":"USD","precision":2}}],
        "email":"fixture@example.invalid", "phone":null,"attributes":{"pickup":"desk"},"tags":["Convention","Portal"],"reservation_until":"2099-01-01T00:00:00Z"
    })).unwrap()
}

fn checkout_order_value() -> serde_json::Value {
    let mut value = order_value();
    value["transactions"] = json!([]);
    value["tags"] = json!(["Convention", "Portal"]);
    value["totalPriceSet"] = json!({"shopMoney":{"amount":"3420","currencyCode":"THB"},"presentmentMoney":{"amount":"95.00","currencyCode":"USD"}});
    value["totalShippingPriceSet"] = json!({"shopMoney":{"amount":"0","currencyCode":"THB"},"presentmentMoney":{"amount":"0.00","currencyCode":"USD"}});
    value["lineItems"]["nodes"][0]["variant"] = json!({"id":"gid://shopify/ProductVariant/1"});
    value["lineItems"]["nodes"][0]["originalUnitPriceSet"]["presentmentMoney"] =
        json!({"amount":"95.00","currencyCode":"USD"});
    value["lineItems"]["nodes"][0]["originalUnitPriceSet"]["shopMoney"] =
        json!({"amount":"3420.00","currencyCode":"THB"});
    value
}

#[test]
fn local_full_sale_checks_exact_presentment_and_allocates_one_real_item() {
    let value = checkout_order_value();
    let order = serde_json::from_value(value.clone()).unwrap();
    let snapshot = checkout_snapshot();
    super::validate_checkout_order(
        &order,
        &snapshot,
        crate::models::checkout_completion::CompletionState::OrderSaved,
    )
    .unwrap();
    let mut row = payment(AccountingGateway::Stripe, AccountingKind::Sale, "ch_portal");
    row.transaction.gross_paid = 3420.0;
    row.transaction.net_paid = 3300.0;
    let mut items = Vec::new();
    apply(&mut row, &order, &mut items).unwrap();
    assert!(items.is_empty());
    super::apply_checkout_sale(&mut row, &order, &mut items, true).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].payment_reference, "ch_portal");
    assert_eq!(items[0].price, 3420.0);
    assert_eq!(items[0].currency, "THB");
    assert_eq!(row.transaction.gross_paid, 3420.0);
    assert_eq!(row.transaction.net_paid, 3300.0);
    assert_eq!(row.transaction.shipping_fee, Some(0.0));
    let mut contradictory = payment(AccountingGateway::Paypal, AccountingKind::Sale, "pp_portal");
    contradictory.transaction.shipping_fee = Some(10.0);
    let mut rejected_items = Vec::new();
    assert!(
        super::apply_checkout_sale(&mut contradictory, &order, &mut rejected_items, true).is_err()
    );
    assert_eq!(contradictory.transaction.shipping_fee, Some(10.0));
    assert_eq!(rejected_items.len(), 0);
    let mut deleted = value.clone();
    deleted["lineItems"]["nodes"][0]["variant"] = serde_json::Value::Null;
    let deleted_order = serde_json::from_value(deleted.clone()).unwrap();
    super::validate_checkout_order(
        &deleted_order,
        &snapshot,
        crate::models::checkout_completion::CompletionState::Complete,
    )
    .unwrap();
    assert!(
        super::validate_checkout_order(
            &deleted_order,
            &snapshot,
            crate::models::checkout_completion::CompletionState::OrderSaved
        )
        .is_err()
    );
    let mut wrong_historical_price = deleted.clone();
    wrong_historical_price["lineItems"]["nodes"][0]["originalUnitPriceSet"]["presentmentMoney"]["amount"] =
        json!("95.01");
    assert!(
        super::validate_checkout_order(
            &serde_json::from_value(wrong_historical_price).unwrap(),
            &snapshot,
            crate::models::checkout_completion::CompletionState::Complete
        )
        .is_err()
    );
    deleted["lineItems"]["nodes"][0]["quantity"] = json!(2);
    assert!(
        super::validate_checkout_order(
            &serde_json::from_value(deleted).unwrap(),
            &snapshot,
            crate::models::checkout_completion::CompletionState::Complete
        )
        .is_err()
    );
    for (path, replacement) in [
        ("/totalPriceSet/presentmentMoney/amount", json!("95.001")),
        (
            "/totalShippingPriceSet/presentmentMoney/amount",
            json!("0.01"),
        ),
        ("/lineItems/nodes/0/quantity", json!(2)),
        (
            "/lineItems/nodes/0/variant/id",
            json!("gid://shopify/ProductVariant/2"),
        ),
        (
            "/lineItems/nodes/0/originalUnitPriceSet/presentmentMoney/currencyCode",
            json!("EUR"),
        ),
    ] {
        let mut invalid = value.clone();
        *invalid.pointer_mut(path).unwrap() = replacement;
        assert!(
            super::validate_checkout_order(
                &serde_json::from_value(invalid).unwrap(),
                &snapshot,
                crate::models::checkout_completion::CompletionState::OrderSaved
            )
            .is_err(),
            "{path}"
        );
    }
}

#[test]
fn malformed_checkout_presentment_is_an_upstream_protocol_error() {
    for deleted in [false, true] {
        for amount in ["invalid", "9223372036854775808.00", "95.001"] {
            let mut value = order_value();
            value["totalPriceSet"] = json!({"shopMoney":{"amount":"95","currencyCode":"USD"},"presentmentMoney":{"amount":"95.00","currencyCode":"USD"}});
            value["totalShippingPriceSet"] = json!({"shopMoney":{"amount":"0","currencyCode":"USD"},"presentmentMoney":{"amount":"0.00","currencyCode":"USD"}});
            value["lineItems"]["nodes"][0]["variant"] = if deleted {
                serde_json::Value::Null
            } else {
                json!({"id":"gid://shopify/ProductVariant/1"})
            };
            value["lineItems"]["nodes"][0]["originalUnitPriceSet"]["presentmentMoney"] =
                json!({"amount":amount,"currencyCode":"USD"});
            let error = super::validate_checkout_order(
                &serde_json::from_value(value).unwrap(),
                &checkout_snapshot(),
                crate::models::checkout_completion::CompletionState::Complete,
            )
            .unwrap_err();
            assert_eq!(
                error.http_status(),
                axum::http::StatusCode::BAD_GATEWAY,
                "deleted={deleted}, amount={amount}"
            );
        }
    }
}

#[test]
fn checkout_accounting_variant_must_be_selected_even_when_deleted() {
    let mut value = checkout_order_value();
    value["lineItems"]["nodes"][0]
        .as_object_mut()
        .unwrap()
        .remove("variant");
    assert!(serde_json::from_value::<crate::models::shopify::Order>(value).is_err());
}
