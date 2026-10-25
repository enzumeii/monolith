#![allow(clippy::unwrap_used, clippy::float_cmp)]
use super::{apply, complete, matching_position, native_index, reference, thb};
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
    assert_eq!(
        payment.transaction.customer_address,
        "1 Road, Bangkok, 10000"
    );
    assert_eq!(payment.transaction.customer_phone, 66_812_345_678);
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
    let mut row = payment(AccountingGateway::Stripe, AccountingKind::Sale, "ch_1");
    apply(&mut row, &order, &mut Vec::new()).unwrap();
    assert_eq!(
        row.transaction.customer_address,
        "บ้าน 1, กรุงเทพ, TH-10, Thailand, 00100"
    );
    assert_eq!(row.transaction.customer_phone, 81345);
    value["shippingAddress"]["phone"] = json!("18446744073709551616");
    apply(
        &mut row,
        &serde_json::from_value(value).unwrap(),
        &mut Vec::new(),
    )
    .unwrap();
    assert_eq!(row.transaction.customer_phone, 0);
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

fn external_order(gateway: AccountingGateway, id: &str) -> crate::models::shopify::Order {
    let (method, kind) = match gateway {
        AccountingGateway::Stripe => ("Stripe Card Payments", "stripe:charge"),
        AccountingGateway::Paypal => ("paypal", "paypal:capture"),
    };
    let mut value = order_value();
    value["tags"] = json!([
        "co:11111111-1111-4111-8111-111111111111",
        "cv:22222222-2222-4222-8222-222222222222",
        "qa-convention",
        crate::utils::payment_reference::tag(kind, id).unwrap()
    ]);
    value["displayFinancialStatus"] = json!("PAID");
    value["cancelledAt"] = json!(null);
    value["currentTotalPriceSet"] = json!({"shopMoney":{"amount":"4.00","currencyCode":"AUD"},"presentmentMoney":{"amount":"4.00","currencyCode":"AUD"}});
    value["totalOutstandingSet"] = json!({"shopMoney":{"amount":"0.00","currencyCode":"AUD"},"presentmentMoney":{"amount":"0.00","currencyCode":"AUD"}});
    value["transactions"] = json!([transaction(method, "SALE", json!({}))]);
    value["transactions"][0]["paymentId"] = json!("shopify-external-record-id");
    value["transactions"][0]["amountSet"] = value["currentTotalPriceSet"].clone();
    value["lineItems"]["nodes"][0]["originalUnitPriceSet"] = value["currentTotalPriceSet"].clone();
    value["shippingLines"] = page(vec![]);
    serde_json::from_value(value).unwrap()
}

fn external_payment(gateway: AccountingGateway, reference: &str) -> super::Payment {
    let mut row = payment(gateway, AccountingKind::Sale, reference);
    row.currency = "AUD".into();
    row.source_money =
        Some(crate::models::checkout_completion::ExactMoney::new(400, "AUD", 2).unwrap());
    row.transaction.gross_paid = if gateway == AccountingGateway::Stripe {
        96.0
    } else {
        4.0
    };
    row.transaction.exchange_rate = Some(24.0);
    row
}

#[test]
fn externally_verified_sales_join_order_items_and_slug_using_source_money() {
    for (gateway, id) in [
        (
            AccountingGateway::Stripe,
            "ch_3SensibleLongActualChargeReference00000001",
        ),
        (AccountingGateway::Paypal, "6UW11105W4629325Y"),
    ] {
        let order = external_order(gateway, id);
        let mut row = external_payment(gateway, id);
        let orders = vec![order];
        assert_eq!(
            matching_position(&orders, &native_index(&orders).unwrap(), &row).unwrap(),
            Some(0)
        );
        let mut items = Vec::new();
        apply(&mut row, &orders[0], &mut items).unwrap();
        assert_eq!(row.transaction.shopify_order_id, 1);
        assert!(
            row.transaction
                .shopify_order_tags
                .iter()
                .any(|tag| tag == "qa-convention")
        );
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].price, 4.0);
        assert_eq!(items[0].currency, "AUD");
        assert_eq!(items[0].payment_reference, id);
    }
}

#[test]
fn external_sale_tags_never_replace_provider_money_or_successful_shopify_evidence() {
    let id = "ch_external";
    let gateway = AccountingGateway::Stripe;
    let source = serde_json::to_value(external_order(gateway, id)).unwrap();
    for (path, replacement) in [
        ("/displayFinancialStatus", json!("PENDING")),
        ("/displayFinancialStatus", json!(null)),
        ("/cancelledAt", json!("2026-10-24T00:00:00Z")),
        (
            "/currentTotalPriceSet/presentmentMoney/amount",
            json!("3.99"),
        ),
        (
            "/currentTotalPriceSet/presentmentMoney/currencyCode",
            json!("USD"),
        ),
        (
            "/totalOutstandingSet/presentmentMoney/amount",
            json!("0.01"),
        ),
        ("/transactions/0/status", json!("PENDING")),
        ("/transactions/0/kind", json!("CAPTURE")),
        ("/transactions/0/kind", json!("REFUND")),
        ("/transactions/0/gateway", json!("manual")),
        (
            "/transactions/0/amountSet/presentmentMoney/amount",
            json!("4.01"),
        ),
        (
            "/transactions/0/amountSet/presentmentMoney/currencyCode",
            json!("USD"),
        ),
        ("/tags/0", json!("co:invalid")),
        ("/tags/1", json!("cv:invalid")),
        ("/tags/3", json!("payment:stripe:charge:another")),
    ] {
        let mut value = source.clone();
        *value.pointer_mut(path).unwrap() = replacement;
        let orders = vec![serde_json::from_value(value).unwrap()];
        let row = external_payment(gateway, id);
        assert_eq!(
            matching_position(&orders, &native_index(&orders).unwrap(), &row).unwrap(),
            None,
            "{path}"
        );
    }
    let orders = vec![external_order(gateway, id)];
    let index = native_index(&orders).unwrap();
    let mut row = external_payment(gateway, id);
    row.source_money = None;
    assert_eq!(matching_position(&orders, &index, &row).unwrap(), None);
    row = external_payment(gateway, id);
    row.transaction.kind = AccountingKind::Refund;
    assert_eq!(matching_position(&orders, &index, &row).unwrap(), None);
}

#[test]
fn external_matching_rejects_ambiguous_orders_and_success_transactions() {
    let id = "capture_external";
    let row = external_payment(AccountingGateway::Paypal, id);
    let mut order = external_order(AccountingGateway::Paypal, id);
    order.transactions.push(order.transactions[0].clone());
    let orders = vec![order];
    assert_eq!(
        matching_position(&orders, &native_index(&orders).unwrap(), &row).unwrap(),
        None
    );
    let first = external_order(AccountingGateway::Paypal, id);
    let mut second = first.clone();
    second.id = serde_json::from_value(json!("gid://shopify/Order/2")).unwrap();
    let orders = vec![first, second];
    assert!(matching_position(&orders, &native_index(&orders).unwrap(), &row).is_err());
    let mut native = order_value();
    native["id"] = json!("gid://shopify/Order/2");
    native["transactions"][0] = transaction("paypal", "SALE", json!({"transaction_id":id}));
    let orders = vec![
        external_order(AccountingGateway::Paypal, id),
        serde_json::from_value(native).unwrap(),
    ];
    assert!(matching_position(&orders, &native_index(&orders).unwrap(), &row).is_err());
}

#[test]
fn native_payment_session_and_refund_indexes_preserve_matching_without_fallback_evidence() {
    let mut value = order_value();
    value["transactions"][0] = transaction("Stripe Card Payments", "CAPTURE", json!({}));
    let orders = vec![serde_json::from_value(value).unwrap()];
    let row = payment(
        AccountingGateway::Stripe,
        AccountingKind::Sale,
        "different-payment-id",
    );
    assert_eq!(
        matching_position(&orders, &native_index(&orders).unwrap(), &row).unwrap(),
        Some(0)
    );
    let mut value = order_value();
    value["transactions"][0] = transaction(
        "paypal",
        "REFUND",
        json!({"refund_transaction_id":"refund-native"}),
    );
    let orders = vec![serde_json::from_value(value).unwrap()];
    let row = payment(
        AccountingGateway::Paypal,
        AccountingKind::Refund,
        "refund-native",
    );
    assert_eq!(
        matching_position(&orders, &native_index(&orders).unwrap(), &row).unwrap(),
        Some(0)
    );
}

#[test]
fn external_sales_reject_duplicate_correlation_or_payment_tags_and_refunded_orders() {
    let id = "ch_external";
    let row = external_payment(AccountingGateway::Stripe, id);
    let original = external_order(AccountingGateway::Stripe, id);
    for index in [0, 1, 3] {
        let mut order = original.clone();
        order.tags.push(order.tags[index].clone());
        let orders = vec![order];
        assert_eq!(
            matching_position(&orders, &native_index(&orders).unwrap(), &row).unwrap(),
            None
        );
    }
    let mut order = original;
    order.refunds.push(serde_json::from_value(json!({
        "transactions":page(vec![]), "refundShippingLines":page(vec![]), "refundLineItems":page(vec![])
    })).unwrap());
    let orders = vec![order];
    assert_eq!(
        matching_position(&orders, &native_index(&orders).unwrap(), &row).unwrap(),
        None
    );
}

#[test]
fn external_sale_requires_explicit_cancellation_evidence() {
    let id = "ch_external";
    let mut value = serde_json::to_value(external_order(AccountingGateway::Stripe, id)).unwrap();
    value.as_object_mut().unwrap().remove("cancelledAt");
    let orders = vec![serde_json::from_value(value).unwrap()];
    let row = external_payment(AccountingGateway::Stripe, id);
    assert_eq!(
        matching_position(&orders, &native_index(&orders).unwrap(), &row).unwrap(),
        None
    );
}

#[test]
fn external_sales_validate_presentment_money_and_keep_shop_money_item_allocation() {
    for (gateway, id) in [
        (AccountingGateway::Stripe, "ch_multicurrency"),
        (AccountingGateway::Paypal, "capture_multicurrency"),
    ] {
        let mut value = serde_json::to_value(external_order(gateway, id)).unwrap();
        let total = json!({"shopMoney":{"amount":"52.00","currencyCode":"AUD"},"presentmentMoney":{"amount":"40.00","currencyCode":"SGD"}});
        value["currentTotalPriceSet"] = total.clone();
        value["transactions"][0]["amountSet"] = total;
        value["totalOutstandingSet"] = json!({"shopMoney":{"amount":"0.00","currencyCode":"AUD"},"presentmentMoney":{"amount":"0.00","currencyCode":"SGD"}});
        value["lineItems"]["nodes"][0]["quantity"] = json!(2);
        value["lineItems"]["nodes"][0]["originalUnitPriceSet"] = json!({"shopMoney":{"amount":"26.00","currencyCode":"AUD"},"presentmentMoney":{"amount":"20.00","currencyCode":"SGD"}});
        let orders = vec![serde_json::from_value(value).unwrap()];
        let mut row = external_payment(gateway, id);
        row.currency = "SGD".into();
        row.source_money =
            Some(crate::models::checkout_completion::ExactMoney::new(4000, "SGD", 2).unwrap());
        assert_eq!(
            matching_position(&orders, &native_index(&orders).unwrap(), &row).unwrap(),
            Some(0)
        );
        let mut items = Vec::new();
        apply(&mut row, &orders[0], &mut items).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].quantity, 2);
        assert_eq!(items[0].price, 26.0);
        assert_eq!(items[0].currency, "AUD");
        row.currency = "AUD".into();
        row.source_money =
            Some(crate::models::checkout_completion::ExactMoney::new(5200, "AUD", 2).unwrap());
        assert_eq!(
            matching_position(&orders, &native_index(&orders).unwrap(), &row).unwrap(),
            None
        );
    }
}

#[test]
fn external_sales_require_presentment_evidence_even_when_shop_money_matches() {
    for gateway in [AccountingGateway::Stripe, AccountingGateway::Paypal] {
        let original =
            serde_json::to_value(external_order(gateway, "external-missing-currency")).unwrap();
        for path in [
            "/currentTotalPriceSet",
            "/totalOutstandingSet",
            "/transactions/0/amountSet",
        ] {
            let mut value = original.clone();
            value
                .pointer_mut(path)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .remove("presentmentMoney");
            let orders = vec![serde_json::from_value(value).unwrap()];
            let row = external_payment(gateway, "external-missing-currency");
            assert_eq!(
                matching_position(&orders, &native_index(&orders).unwrap(), &row).unwrap(),
                None,
                "{path}"
            );
        }
    }
}

async fn enrich_orders(
    orders: Vec<crate::models::shopify::Order>,
    payments: Vec<super::Payment>,
) -> Result<domain::dto::AccountingReportResponse, crate::error::AppError> {
    let fixture = crate::client::tests::Fixture::new(vec![crate::client::tests::Reply::json(
        json!({"data":{"orders":page(orders.into_iter().map(|order| serde_json::to_value(order).unwrap()).collect())}})
    )]).await;
    let client = crate::client::shopify::ShopifyClient::new(&crate::config::ShopifyConfig {
        admin_api_url: fixture.url.to_string(),
        timeout_secs: 3,
        access_token: Some("fixture-token".into()),
    })
    .unwrap();
    let range = crate::models::accounting::AccountingDateRange {
        start: "2026-10-01T00:00:00Z".parse().unwrap(),
        end: "2026-10-31T23:59:59Z".parse().unwrap(),
    };
    super::enrich(&client, payments, &range).await
}

#[tokio::test]
async fn external_sale_rejects_distinct_provider_claims_for_one_shopify_transaction() {
    for (gateway, kind, first, second) in [
        (
            AccountingGateway::Stripe,
            "stripe:charge",
            "ch_first",
            "ch_second",
        ),
        (
            AccountingGateway::Paypal,
            "paypal:capture",
            "capture_first",
            "capture_second",
        ),
    ] {
        for reverse in [false, true] {
            let mut order = external_order(gateway, first);
            order
                .tags
                .push(crate::utils::payment_reference::tag(kind, second).unwrap());
            if gateway == AccountingGateway::Stripe {
                order.tags.push(
                    crate::utils::payment_reference::tag("stripe:session", "cs_checkout").unwrap(),
                );
                order.tags.push(
                    crate::utils::payment_reference::tag("stripe:intent", "pi_checkout").unwrap(),
                );
            }
            let mut payments = vec![
                external_payment(gateway, first),
                external_payment(gateway, second),
            ];
            if reverse {
                payments.reverse();
            }
            assert!(enrich_orders(vec![order], payments).await.is_err());
        }
    }
}

#[tokio::test]
async fn conflicting_fallback_tags_do_not_override_a_native_transaction_reference() {
    for gateway in [AccountingGateway::Stripe, AccountingGateway::Paypal] {
        for reverse in [false, true] {
            let mut value = serde_json::to_value(external_order(gateway, "external_tag")).unwrap();
            match gateway {
                AccountingGateway::Stripe => {
                    value["transactions"][0]["paymentId"] = json!("native-session");
                }
                AccountingGateway::Paypal => {
                    value["transactions"][0]["receiptJson"] =
                        json!(json!({"transaction_id":"native-session"}).to_string());
                }
            }
            let mut payments = vec![
                payment(gateway, AccountingKind::Sale, "native-session"),
                external_payment(gateway, "external_tag"),
            ];
            if reverse {
                payments.reverse();
            }
            let result =
                enrich_orders(vec![serde_json::from_value(value).unwrap()], payments).await;
            if gateway == AccountingGateway::Stripe {
                assert!(result.is_err());
                continue;
            }
            let response = result.unwrap();
            let transactions = match gateway {
                AccountingGateway::Stripe => response.transactions.stripe,
                AccountingGateway::Paypal => response.transactions.paypal,
            };
            assert_eq!(
                transactions
                    .iter()
                    .find(|row| row.payment_reference == "native-session")
                    .unwrap()
                    .shopify_order_id,
                1
            );
            assert_eq!(
                transactions
                    .iter()
                    .find(|row| row.payment_reference == "external_tag")
                    .unwrap()
                    .shopify_order_id,
                0
            );
            assert_eq!(response.items.len(), 1);
        }
    }
}

#[tokio::test]
async fn external_stripe_charge_matches_with_opaque_shopify_payment_id() {
    let charge = "ch_opaque_shopify_id";
    let order = external_order(AccountingGateway::Stripe, charge);
    assert_eq!(
        order.transactions[0].payment_id.as_deref(),
        Some("shopify-external-record-id")
    );
    let response = enrich_orders(
        vec![order],
        vec![external_payment(AccountingGateway::Stripe, charge)],
    )
    .await
    .unwrap();
    assert_eq!(response.transactions.stripe[0].shopify_order_id, 1);
    assert_eq!(response.transactions.stripe[0].payment_reference, charge);
    assert_eq!(response.items.len(), 1);
}

#[tokio::test]
async fn paypal_capture_receipt_contradictions_still_reject_external_tag_matching() {
    let mut order = external_order(AccountingGateway::Paypal, "capture_from_tag");
    order.transactions[0].receipt_json =
        Some(json!({"transaction_id":"different_provider_capture"}).to_string());
    let response = enrich_orders(
        vec![order],
        vec![external_payment(
            AccountingGateway::Paypal,
            "capture_from_tag",
        )],
    )
    .await
    .unwrap();
    assert_eq!(response.transactions.paypal[0].shopify_order_id, 0);
    assert!(response.items.is_empty());
}

fn metadata_order(gateway: AccountingGateway, reference: &str) -> crate::models::shopify::Order {
    let mut value = serde_json::to_value(external_order(gateway, reference)).unwrap();
    value["tags"] = json!(["qa-convention"]);
    value["transactions"][0]["gateway"] = json!("manual");
    let mut attributes = vec![
        json!({"key":"checkout_correlation_version","value":"2"}),
        json!({"key":"checkout_id","value":"11111111-1111-4111-8111-111111111111"}),
        json!({"key":"convention_id","value":"22222222-2222-4222-8222-222222222222"}),
        json!({"key":"convention_slug","value":"qa-convention"}),
        json!({"key":"payment_provider","value":match gateway { AccountingGateway::Stripe => "stripe", AccountingGateway::Paypal => "paypal" }}),
    ];
    match gateway {
        AccountingGateway::Stripe => attributes.extend([
            json!({"key":"stripe_checkout_session_id","value":"cs_metadata"}),
            json!({"key":"stripe_payment_intent_id","value":"pi_metadata"}),
            json!({"key":"stripe_charge_id","value":reference}),
        ]),
        AccountingGateway::Paypal => attributes.extend([
            json!({"key":"paypal_order_id","value":"order_metadata"}),
            json!({"key":"paypal_capture_id","value":reference}),
        ]),
    }
    value["customAttributes"] = json!(attributes);
    serde_json::from_value(value).unwrap()
}

#[tokio::test]
async fn manual_checkout_metadata_links_both_provider_rows_items_and_only_slug_tag() {
    for (gateway, reference) in [
        (AccountingGateway::Stripe, "ch_metadata"),
        (AccountingGateway::Paypal, "capture_metadata"),
    ] {
        let response = enrich_orders(
            vec![metadata_order(gateway, reference)],
            vec![external_payment(gateway, reference)],
        )
        .await
        .unwrap();
        let rows = match gateway {
            AccountingGateway::Stripe => response.transactions.stripe,
            AccountingGateway::Paypal => response.transactions.paypal,
        };
        assert_eq!(rows[0].shopify_order_id, 1);
        assert_eq!(rows[0].shopify_order_number, 1);
        assert_eq!(rows[0].payment_reference, reference);
        assert_eq!(rows[0].shopify_order_tags, ["qa-convention"]);
        assert_eq!(response.items.len(), 1);
        assert_eq!(response.items[0].payment_reference, reference);
        assert_eq!(response.items[0].currency, "AUD");
        assert_eq!(response.items[0].price, 4.0);
    }
}

#[tokio::test]
async fn manual_checkout_requires_complete_canonical_metadata_and_exact_source_reference() {
    for gateway in [AccountingGateway::Stripe, AccountingGateway::Paypal] {
        let reference = "source_reference";
        let original = serde_json::to_value(metadata_order(gateway, reference)).unwrap();
        let reference_key = match gateway {
            AccountingGateway::Stripe => "stripe_charge_id",
            AccountingGateway::Paypal => "paypal_capture_id",
        };
        for missing in ["customAttributes", "provider_reference", "provider_parent"] {
            let mut value = original.clone();
            if missing == "customAttributes" {
                value["customAttributes"] = json!([]);
            } else {
                let key = if missing == "provider_reference" {
                    reference_key
                } else {
                    match gateway {
                        AccountingGateway::Stripe => "stripe_payment_intent_id",
                        AccountingGateway::Paypal => "paypal_order_id",
                    }
                };
                value["customAttributes"]
                    .as_array_mut()
                    .unwrap()
                    .retain(|attribute| attribute["key"] != key);
            }
            let response = enrich_orders(
                vec![serde_json::from_value(value).unwrap()],
                vec![external_payment(gateway, reference)],
            )
            .await
            .unwrap();
            let rows = match gateway {
                AccountingGateway::Stripe => response.transactions.stripe,
                AccountingGateway::Paypal => response.transactions.paypal,
            };
            assert_eq!(rows[0].shopify_order_id, 0, "{missing}");
            assert!(response.items.is_empty(), "{missing}");
        }
        let response = enrich_orders(
            vec![metadata_order(gateway, "another_source_reference")],
            vec![external_payment(gateway, reference)],
        )
        .await
        .unwrap();
        let rows = match gateway {
            AccountingGateway::Stripe => response.transactions.stripe,
            AccountingGateway::Paypal => response.transactions.paypal,
        };
        assert_eq!(rows[0].shopify_order_id, 0);
        assert!(response.items.is_empty());
    }
}

#[tokio::test]
async fn metadata_manual_sale_keeps_full_paid_single_transaction_and_presentment_proof() {
    for gateway in [AccountingGateway::Stripe, AccountingGateway::Paypal] {
        let source = serde_json::to_value(metadata_order(gateway, "source_reference")).unwrap();
        for (path, replacement) in [
            ("/displayFinancialStatus", json!("PENDING")),
            ("/cancelledAt", json!("2026-10-25T00:00:00Z")),
            (
                "/currentTotalPriceSet/presentmentMoney/amount",
                json!("3.99"),
            ),
            (
                "/totalOutstandingSet/presentmentMoney/amount",
                json!("0.01"),
            ),
            ("/transactions/0/status", json!("PENDING")),
            ("/transactions/0/kind", json!("CAPTURE")),
            ("/transactions/0/gateway", json!("bogus")),
            (
                "/transactions/0/amountSet/presentmentMoney/amount",
                json!("4.01"),
            ),
            (
                "/transactions/0/amountSet/presentmentMoney/currencyCode",
                json!("USD"),
            ),
        ] {
            let mut value = source.clone();
            *value.pointer_mut(path).unwrap() = replacement;
            let response = enrich_orders(
                vec![serde_json::from_value(value).unwrap()],
                vec![external_payment(gateway, "source_reference")],
            )
            .await
            .unwrap();
            let rows = match gateway {
                AccountingGateway::Stripe => response.transactions.stripe,
                AccountingGateway::Paypal => response.transactions.paypal,
            };
            assert_eq!(rows[0].shopify_order_id, 0, "{path}");
            assert!(response.items.is_empty(), "{path}");
        }
        let mut order = metadata_order(gateway, "source_reference");
        order.transactions.push(order.transactions[0].clone());
        let response = enrich_orders(
            vec![order],
            vec![external_payment(gateway, "source_reference")],
        )
        .await
        .unwrap();
        let rows = match gateway {
            AccountingGateway::Stripe => response.transactions.stripe,
            AccountingGateway::Paypal => response.transactions.paypal,
        };
        assert_eq!(rows[0].shopify_order_id, 0);
        assert!(response.items.is_empty());
    }
}

#[tokio::test]
async fn metadata_conflicts_and_duplicate_checkout_identity_fail_before_enrichment() {
    for gateway in [AccountingGateway::Stripe, AccountingGateway::Paypal] {
        let original = serde_json::to_value(metadata_order(gateway, "source_reference")).unwrap();
        for replacement in [
            json!({"key":"checkout_id","value":"different"}),
            json!({"key":"payment_provider","value":"stripe"}),
            json!({"key":"convention_slug","value":"different"}),
        ] {
            let mut value = original.clone();
            value["customAttributes"]
                .as_array_mut()
                .unwrap()
                .push(replacement);
            assert!(
                enrich_orders(
                    vec![serde_json::from_value(value).unwrap()],
                    vec![external_payment(gateway, "source_reference")]
                )
                .await
                .is_err()
            );
        }
        let mut second = metadata_order(gateway, "another_source");
        second.id = serde_json::from_value(json!("gid://shopify/Order/2")).unwrap();
        assert!(
            enrich_orders(
                vec![metadata_order(gateway, "source_reference"), second],
                vec![external_payment(gateway, "source_reference")]
            )
            .await
            .is_err()
        );
        let mut legacy = external_order(gateway, "legacy_source");
        legacy.id = serde_json::from_value(json!("gid://shopify/Order/2")).unwrap();
        assert!(
            enrich_orders(
                vec![metadata_order(gateway, "source_reference"), legacy],
                vec![external_payment(gateway, "source_reference")],
            )
            .await
            .is_err()
        );
        let mut conflicting_tags = metadata_order(gateway, "source_reference");
        conflicting_tags
            .tags
            .push("co:33333333-3333-4333-8333-333333333333".into());
        assert!(
            enrich_orders(
                vec![conflicting_tags],
                vec![external_payment(gateway, "source_reference")]
            )
            .await
            .is_err()
        );
    }
}

#[tokio::test]
async fn metadata_and_native_orders_cannot_claim_the_same_provider_reference() {
    for gateway in [AccountingGateway::Stripe, AccountingGateway::Paypal] {
        let reference = "source_reference";
        let mut native = order_value();
        native["id"] = json!("gid://shopify/Order/2");
        native["transactions"][0] = match gateway {
            AccountingGateway::Stripe => transaction(
                "stripe",
                "CAPTURE",
                json!({"charges":{"data":[{"id":reference}]}}),
            ),
            AccountingGateway::Paypal => {
                transaction("paypal", "SALE", json!({"transaction_id":reference}))
            }
        };
        for reverse in [false, true] {
            let mut orders = vec![
                metadata_order(gateway, reference),
                serde_json::from_value(native.clone()).unwrap(),
            ];
            if reverse {
                orders.reverse();
            }
            assert!(
                enrich_orders(orders, vec![external_payment(gateway, reference)])
                    .await
                    .is_err()
            );
        }
    }
}

#[tokio::test]
async fn metadata_both_provider_or_legacy_reference_conflicts_fail_closed() {
    for gateway in [AccountingGateway::Stripe, AccountingGateway::Paypal] {
        let original = serde_json::to_value(metadata_order(gateway, "source_reference")).unwrap();
        let other_provider_key = match gateway {
            AccountingGateway::Stripe => "paypal_capture_id",
            AccountingGateway::Paypal => "stripe_charge_id",
        };
        let mut both_providers = original.clone();
        both_providers["customAttributes"]
            .as_array_mut()
            .unwrap()
            .push(json!({"key":other_provider_key,"value":"other_provider"}));
        assert!(
            enrich_orders(
                vec![serde_json::from_value(both_providers).unwrap()],
                vec![external_payment(gateway, "source_reference")]
            )
            .await
            .is_err()
        );
        let mut legacy_conflict = original;
        let kind = match gateway {
            AccountingGateway::Stripe => "stripe:charge",
            AccountingGateway::Paypal => "paypal:capture",
        };
        legacy_conflict["tags"].as_array_mut().unwrap().push(json!(
            crate::utils::payment_reference::tag(kind, "different_source_reference").unwrap()
        ));
        assert!(
            enrich_orders(
                vec![serde_json::from_value(legacy_conflict).unwrap()],
                vec![external_payment(gateway, "source_reference")]
            )
            .await
            .is_err()
        );
    }
}

#[tokio::test]
async fn metadata_sale_rejects_refund_intents_and_a_saturated_transaction_connection() {
    for mode in ["pending_refund", "transaction_limit"] {
        let mut order = metadata_order(AccountingGateway::Stripe, "ch_metadata");
        let mut pending = order.transactions[0].clone();
        pending.status = "PENDING".into();
        if mode == "pending_refund" {
            pending.kind = "REFUND".into();
            order.transactions.push(pending);
        } else {
            order.transactions.extend(std::iter::repeat_n(pending, 99));
        }
        let response = enrich_orders(
            vec![order],
            vec![external_payment(AccountingGateway::Stripe, "ch_metadata")],
        )
        .await
        .unwrap();
        assert_eq!(
            response.transactions.stripe[0].shopify_order_id, 0,
            "{mode}"
        );
        assert!(response.items.is_empty(), "{mode}");
    }
}

#[tokio::test]
async fn manual_metadata_compares_source_currency_and_allocates_shop_currency_items() {
    for gateway in [AccountingGateway::Stripe, AccountingGateway::Paypal] {
        let mut value = serde_json::to_value(metadata_order(gateway, "source_reference")).unwrap();
        let total = json!({"shopMoney":{"amount":"52.00","currencyCode":"AUD"},"presentmentMoney":{"amount":"40.00","currencyCode":"SGD"}});
        value["currentTotalPriceSet"] = total.clone();
        value["transactions"][0]["amountSet"] = total;
        value["totalOutstandingSet"] = json!({"shopMoney":{"amount":"0.00","currencyCode":"AUD"},"presentmentMoney":{"amount":"0.00","currencyCode":"SGD"}});
        value["lineItems"]["nodes"][0]["quantity"] = json!(2);
        value["lineItems"]["nodes"][0]["originalUnitPriceSet"] = json!({"shopMoney":{"amount":"26.00","currencyCode":"AUD"},"presentmentMoney":{"amount":"20.00","currencyCode":"SGD"}});
        let mut row = external_payment(gateway, "source_reference");
        row.currency = "SGD".into();
        row.source_money =
            Some(crate::models::checkout_completion::ExactMoney::new(4000, "SGD", 2).unwrap());
        let response = enrich_orders(vec![serde_json::from_value(value).unwrap()], vec![row])
            .await
            .unwrap();
        let rows = match gateway {
            AccountingGateway::Stripe => response.transactions.stripe,
            AccountingGateway::Paypal => response.transactions.paypal,
        };
        assert_eq!(rows[0].shopify_order_id, 1);
        assert_eq!(response.items.len(), 1);
        assert_eq!(response.items[0].quantity, 2);
        assert_eq!(response.items[0].currency, "AUD");
        assert_eq!(response.items[0].price, 26.0);
    }
}

fn independent_order(
    gateway: AccountingGateway,
    reference: &str,
    payment_currency: &str,
    shopify_currency: &str,
) -> crate::models::shopify::Order {
    let mut value = serde_json::to_value(metadata_order(gateway, reference)).unwrap();
    value["customAttributes"][0]["value"] = json!("3");
    value["customAttributes"].as_array_mut().unwrap().extend([
        json!({"key":"payment_amount_minor","value":"1250"}),
        json!({"key":"payment_currency","value":payment_currency}),
        json!({"key":"shopify_amount_minor","value":"400"}),
        json!({"key":"shopify_currency","value":shopify_currency}),
    ]);
    for path in [
        "/currentTotalPriceSet",
        "/totalOutstandingSet",
        "/transactions/0/amountSet",
        "/lineItems/nodes/0/originalUnitPriceSet",
    ] {
        for kind in ["shopMoney", "presentmentMoney"] {
            value.pointer_mut(path).unwrap()[kind]["currencyCode"] = json!(shopify_currency);
        }
    }
    serde_json::from_value(value).unwrap()
}

fn independent_payment(
    gateway: AccountingGateway,
    reference: &str,
    currency: &str,
) -> super::Payment {
    let mut row = external_payment(gateway, reference);
    row.currency = currency.into();
    row.source_money =
        Some(crate::models::checkout_completion::ExactMoney::new(1250, currency, 2).unwrap());
    if gateway == AccountingGateway::Paypal {
        row.transaction.currency = currency.into();
        row.transaction.gross_paid = 12.5;
    }
    row
}

#[tokio::test]
async fn independent_money_metadata_links_distinct_ledgers_and_preserves_exported_money() {
    for (gateway, currency, shop_currency) in [
        (AccountingGateway::Stripe, "THB", "AUD"),
        (AccountingGateway::Paypal, "SGD", "USD"),
        (AccountingGateway::Stripe, "AUD", "AUD"),
    ] {
        let row = independent_payment(gateway, "source_reference", currency);
        let gross = row.transaction.gross_paid;
        let payment_currency = row.transaction.currency.clone();
        let response = enrich_orders(
            vec![independent_order(
                gateway,
                "source_reference",
                currency,
                shop_currency,
            )],
            vec![row],
        )
        .await
        .unwrap();
        let rows = match gateway {
            AccountingGateway::Stripe => response.transactions.stripe,
            AccountingGateway::Paypal => response.transactions.paypal,
        };
        assert_eq!(rows[0].shopify_order_id, 1);
        assert_eq!(rows[0].shopify_order_tags, ["qa-convention"]);
        assert_eq!(rows[0].gross_paid, gross);
        assert_eq!(rows[0].currency, payment_currency);
        assert_eq!(response.items.len(), 1);
        assert_eq!(response.items[0].price, 4.0);
        assert_eq!(response.items[0].currency, shop_currency);
        assert_eq!(response.items[0].payment_reference, "source_reference");
    }
}

#[test]
fn independent_money_rejects_either_ledger_mismatch_and_native_reference_bypass() {
    for gateway in [AccountingGateway::Stripe, AccountingGateway::Paypal] {
        let original =
            serde_json::to_value(independent_order(gateway, "source_reference", "THB", "AUD"))
                .unwrap();
        for (path, replacement) in [
            (
                "/currentTotalPriceSet/presentmentMoney/amount",
                json!("12.50"),
            ),
            ("/currentTotalPriceSet/shopMoney/amount", json!("4.01")),
            ("/currentTotalPriceSet/shopMoney/currencyCode", json!("THB")),
            (
                "/transactions/0/amountSet/presentmentMoney/amount",
                json!("12.50"),
            ),
            ("/transactions/0/amountSet/shopMoney/amount", json!("4.01")),
            (
                "/totalOutstandingSet/presentmentMoney/amount",
                json!("0.01"),
            ),
            ("/totalOutstandingSet/shopMoney/amount", json!("0.01")),
            (
                "/transactions/0/gateway",
                json!(match gateway {
                    AccountingGateway::Stripe => "stripe",
                    AccountingGateway::Paypal => "paypal",
                }),
            ),
        ] {
            let mut value = original.clone();
            *value.pointer_mut(path).unwrap() = replacement;
            value["transactions"][0]["receiptJson"] = json!(
                match gateway {
                    AccountingGateway::Stripe =>
                        json!({"charges":{"data":[{"id":"source_reference"}]}}),
                    AccountingGateway::Paypal => json!({"transaction_id":"source_reference"}),
                }
                .to_string()
            );
            let orders = vec![serde_json::from_value(value).unwrap()];
            let row = independent_payment(gateway, "source_reference", "THB");
            assert_eq!(
                matching_position(&orders, &native_index(&orders).unwrap(), &row).unwrap(),
                None,
                "{path}"
            );
        }
        let orders = vec![serde_json::from_value(original).unwrap()];
        for (minor, currency) in [(1251, "THB"), (1250, "SGD")] {
            let mut row = independent_payment(gateway, "source_reference", "THB");
            row.source_money = Some(
                crate::models::checkout_completion::ExactMoney::new(minor, currency, 2).unwrap(),
            );
            assert_eq!(
                matching_position(&orders, &native_index(&orders).unwrap(), &row).unwrap(),
                None
            );
        }
    }
}

#[test]
fn independent_money_requires_complete_unique_canonical_snapshots() {
    let original = serde_json::to_value(independent_order(
        AccountingGateway::Stripe,
        "source_reference",
        "THB",
        "AUD",
    ))
    .unwrap();
    for key in [
        "payment_amount_minor",
        "payment_currency",
        "shopify_amount_minor",
        "shopify_currency",
    ] {
        for mode in ["missing", "duplicate", "noncanonical"] {
            let mut value = original.clone();
            let attributes = value["customAttributes"].as_array_mut().unwrap();
            let index = attributes
                .iter()
                .position(|attribute| attribute["key"] == key)
                .unwrap();
            match mode {
                "missing" => {
                    attributes.remove(index);
                }
                "duplicate" => attributes.push(attributes[index].clone()),
                _ => {
                    attributes[index]["value"] = json!(if key.ends_with("minor") {
                        "0400"
                    } else {
                        "aud"
                    });
                }
            }
            assert!(
                native_index(&[serde_json::from_value(value).unwrap()]).is_err(),
                "{key}: {mode}"
            );
        }
    }
}

#[tokio::test]
async fn independent_money_never_converts_unrelated_nonzero_shipping() {
    for gateway in [AccountingGateway::Stripe, AccountingGateway::Paypal] {
        let mut value =
            serde_json::to_value(independent_order(gateway, "source_reference", "THB", "AUD"))
                .unwrap();
        value["lineItems"]["nodes"][0]["originalUnitPriceSet"]["shopMoney"]["amount"] =
            json!("3.00");
        value["shippingLines"] = page(vec![
            json!({"priceSet":{"shopMoney":{"amount":"1.00","currencyCode":"AUD"}}}),
        ]);
        let mut row = independent_payment(gateway, "source_reference", "THB");
        row.transaction.shipping_fee = None;
        let response = enrich_orders(
            vec![serde_json::from_value(value.clone()).unwrap()],
            vec![row],
        )
        .await
        .unwrap();
        let rows = match gateway {
            AccountingGateway::Stripe => response.transactions.stripe,
            AccountingGateway::Paypal => response.transactions.paypal,
        };
        assert_eq!(rows[0].shopify_order_id, 1);
        assert_eq!(rows[0].shipping_fee, None);
        assert_eq!(response.items[0].price, 3.0);
        assert_eq!(response.items[0].currency, "AUD");
        if gateway == AccountingGateway::Paypal {
            let mut row = independent_payment(gateway, "source_reference", "THB");
            row.transaction.shipping_fee = Some(2.5);
            let response = enrich_orders(vec![serde_json::from_value(value).unwrap()], vec![row])
                .await
                .unwrap();
            assert_eq!(response.transactions.paypal[0].shipping_fee, Some(2.5));
        }
    }
}

#[test]
fn independent_money_rejects_duplicate_claims_and_ambiguous_or_unpaid_orders() {
    let gateway = AccountingGateway::Stripe;
    let original = independent_order(gateway, "source_reference", "THB", "AUD");
    let row = independent_payment(gateway, "source_reference", "THB");
    for mode in [
        "cancelled",
        "refunded",
        "multiple_sales",
        "unpaid",
        "wrong_currency",
    ] {
        let mut value = serde_json::to_value(&original).unwrap();
        match mode {
            "cancelled" => value["cancelledAt"] = json!("2026-10-25T00:00:00Z"),
            "refunded" => {
                value["refunds"] = json!([{"transactions":page(vec![]),"refundShippingLines":page(vec![]),"refundLineItems":page(vec![])}]);
            }
            "multiple_sales" => {
                let tx = value["transactions"][0].clone();
                value["transactions"].as_array_mut().unwrap().push(tx);
            }
            "unpaid" => value["displayFinancialStatus"] = json!("PENDING"),
            _ => {
                value["transactions"][0]["amountSet"]["presentmentMoney"]["currencyCode"] =
                    json!("THB");
            }
        }
        let orders = vec![serde_json::from_value(value).unwrap()];
        assert_eq!(
            matching_position(&orders, &native_index(&orders).unwrap(), &row).unwrap(),
            None,
            "{mode}"
        );
    }
    assert!(
        super::matching_positions(
            std::slice::from_ref(&original),
            &[
                independent_payment(gateway, "source_reference", "THB"),
                independent_payment(gateway, "source_reference", "THB")
            ]
        )
        .is_err()
    );
    let mut duplicate = original.clone();
    duplicate.id = serde_json::from_value(json!("gid://shopify/Order/2")).unwrap();
    assert!(native_index(&[original, duplicate]).is_err());
}
