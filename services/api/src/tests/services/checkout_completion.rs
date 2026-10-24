#![allow(clippy::unwrap_used)]
use super::*;
use crate::{
    client::{
        stripe::StripeClient,
        tests::{Fixture, Reply},
    },
    config::StripeConfig,
    services::common::{client, id},
};
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn expected() -> stripe::payment::ExpectedPayment {
    stripe::payment::ExpectedPayment {
        session_id: "cs_real".into(),
        account_id: "acct_real".into(),
        livemode: false,
        amount: 1250,
        currency: "THB".into(),
        metadata: BTreeMap::from([("draft".into(), "1".into())]),
    }
}
fn paid() -> Vec<Value> {
    vec![
        json!({"id":"acct_real","object":"account"}),
        json!({"id":"cs_real","object":"checkout.session","mode":"payment","status":"complete","payment_status":"paid","livemode":false,"amount_total":1250,"currency":"thb","metadata":{"draft":"1"},"payment_intent":"pi_real","url":null}),
        json!({"id":"pi_real","object":"payment_intent","status":"succeeded","amount":1250,"amount_received":1250,"currency":"thb","livemode":false,"metadata":{"draft":"1"},"latest_charge":"ch_real"}),
        json!({"id":"ch_real","object":"charge","amount":1250,"livemode":false,"status":"succeeded","amount_captured":1250,"amount_refunded":0,"captured":true,"paid":true,"refunded":false,"created":1,"currency":"thb","disputed":false,"metadata":{},"payment_intent":"pi_real","balance_transaction":null}),
    ]
}
fn stripe_client(f: &Fixture) -> StripeClient {
    StripeClient::new(&StripeConfig {
        checkout_api_version: "2026-08-26.dahlia".into(),
        base_url: f.url.join("/v1/").unwrap().to_string(),
        timeout_secs: 1,
        secret_key: Some("fixture".into()),
    })
    .unwrap()
}
fn draft(completed: bool) -> Value {
    json!({"data":{"draftOrder":{"id":"gid://shopify/DraftOrder/1","ready":true,"tags":[],"status":if completed {"COMPLETED"} else {"OPEN"},"order":if completed {json!({"id":"gid://shopify/Order/2"})} else {Value::Null}, "paymentTerms":null,"totalPriceSet":{"presentmentMoney":{"amount":"12.50","currencyCode":"THB"}}}}})
}
fn tags() -> Vec<String> {
    vec![
        "payment:stripe:session:cs_real".into(),
        "payment:stripe:intent:pi_real".into(),
        "payment:stripe:charge:ch_real".into(),
    ]
}
fn order(tags: Vec<String>) -> Value {
    let tags = Value::from(tags);
    json!({"data":{"order":{"id":"gid://shopify/Order/2","displayFinancialStatus":"PAID","cancelledAt":null,"tags":tags,"totalPriceSet":{"presentmentMoney":{"amount":"12.5000","currencyCode":"THB"}}}}})
}
fn completed() -> Value {
    json!({"data":{"draftOrderComplete":{"draftOrder":{"id":"gid://shopify/DraftOrder/1","order":{"id":"gid://shopify/Order/2"}},"userErrors":[]}}})
}
fn tagged() -> Value {
    json!({"data":{"tagsAdd":{"node":{"id":"gid://shopify/Order/2"},"userErrors":[]}}})
}
async fn run(
    shop: &Fixture,
    provider: &Fixture,
    mode: CompletionMode,
) -> Result<CompletionOutcome, AppError> {
    complete_paid_draft(
        &client(shop),
        &id("DraftOrder", 1),
        PaymentContext::Stripe {
            client: &stripe_client(provider),
            expected: &expected(),
        },
        mode,
    )
    .await
}
fn operations(shop: &Fixture) -> Vec<String> {
    shop.requests()
        .iter()
        .map(|r| r.2["operationName"].as_str().unwrap().to_owned())
        .collect()
}
#[tokio::test]
async fn completes_open_draft_and_confirms_real_order_and_added_tags() {
    let provider = Fixture::new(paid().into_iter().map(Reply::json).collect()).await;
    let shop = Fixture::new(
        vec![
            draft(false),
            completed(),
            order(vec!["OTHER".into()]),
            tagged(),
            order(tags()),
        ]
        .into_iter()
        .map(Reply::json)
        .collect(),
    )
    .await;
    assert!(
        matches!(run(&shop,&provider,CompletionMode::CompleteIfOpen).await.unwrap(), CompletionOutcome::Completed {order_id} if order_id == id("Order",2))
    );
    assert_eq!(
        operations(&shop),
        [
            "CheckoutDraft",
            "CompleteCheckoutDraft",
            "CheckoutOrder",
            "EditOrderTags",
            "CheckoutOrder"
        ]
    );
    assert_eq!(provider.requests().len(), 4);
}
#[tokio::test]
async fn completed_draft_replay_only_reconciles_missing_tags() {
    let provider = Fixture::new(paid().into_iter().map(Reply::json).collect()).await;
    let shop = Fixture::new(
        vec![
            draft(true),
            order(vec!["OTHER".into()]),
            tagged(),
            order(tags()),
        ]
        .into_iter()
        .map(Reply::json)
        .collect(),
    )
    .await;
    run(&shop, &provider, CompletionMode::CompleteIfOpen)
        .await
        .unwrap();
    assert_eq!(
        operations(&shop),
        [
            "CheckoutDraft",
            "CheckoutOrder",
            "EditOrderTags",
            "CheckoutOrder"
        ]
    );
}
#[tokio::test]
async fn interrupted_tagging_replays_without_another_completion() {
    let provider = Fixture::new(paid().into_iter().chain(paid()).map(Reply::json).collect()).await;
    let shop = Fixture::new(vec![
        Reply::json(draft(false)),
        Reply::json(completed()),
        Reply::json(order(vec![])),
        Reply::json(json!({"data":{"tagsAdd":{"node":null,"userErrors":[{"field":["tags"],"message":"Fixture tag write rejected"}]}}})),
        Reply::json(draft(true)),
        Reply::json(order(vec![])),
        Reply::json(tagged()),
        Reply::json(order(tags())),
    ])
    .await;
    assert!(
        run(&shop, &provider, CompletionMode::CompleteIfOpen)
            .await
            .is_err()
    );
    run(&shop, &provider, CompletionMode::ReconcileOnly)
        .await
        .unwrap();
    assert_eq!(
        operations(&shop)
            .iter()
            .filter(|name| name.as_str() == "CompleteCheckoutDraft")
            .count(),
        1
    );
}
#[tokio::test]
async fn reconcile_only_never_completes_an_open_draft() {
    let provider = Fixture::new(paid().into_iter().map(Reply::json).collect()).await;
    let shop = Fixture::new(vec![Reply::json(draft(false))]).await;
    assert!(
        run(&shop, &provider, CompletionMode::ReconcileOnly)
            .await
            .is_err()
    );
    assert_eq!(operations(&shop), ["CheckoutDraft"]);
}
#[tokio::test]
async fn ambiguous_completion_reads_back_once_and_never_retries_mutation() {
    for visible in [true, false] {
        let provider = Fixture::new(paid().into_iter().map(Reply::json).collect()).await;
        let mut replies = vec![
            Reply::json(draft(false)),
            Reply::status(500),
            Reply::json(draft(visible)),
        ];
        if visible {
            replies.push(Reply::json(order(tags())));
        }
        let shop = Fixture::new(replies).await;
        assert_eq!(
            run(&shop, &provider, CompletionMode::CompleteIfOpen)
                .await
                .is_ok(),
            visible
        );
        assert_eq!(
            operations(&shop)
                .iter()
                .filter(|name| name.as_str() == "CompleteCheckoutDraft")
                .count(),
            1
        );
    }
}
#[tokio::test]
async fn provider_unpaid_refund_dispute_or_mismatch_never_mutates_shopify() {
    for field in [
        "pending",
        "expired",
        "refunded",
        "amount_refunded",
        "disputed",
        "amount",
    ] {
        let mut objects = paid();
        match field {
            "pending" | "expired" => {
                objects[1]["payment_status"] = json!("unpaid");
                objects[1]["status"] = json!(if field == "pending" {
                    "open"
                } else {
                    "expired"
                });
            }
            "amount_refunded" => objects[3][field] = json!(1),
            "amount" => objects[3][field] = json!(1251),
            _ => objects[3][field] = json!(true),
        }
        let provider = Fixture::new(objects.into_iter().map(Reply::json).collect()).await;
        let shop = Fixture::new(vec![]).await;
        let result = run(&shop, &provider, CompletionMode::CompleteIfOpen).await;
        assert_eq!(
            result.is_ok(),
            matches!(field, "pending" | "expired"),
            "{field}"
        );
        assert!(shop.requests().is_empty(), "{field}");
    }
}
#[tokio::test]
async fn rejects_mismatched_money_and_conflicting_or_unconfirmed_owned_tags() {
    for field in [
        "draft_amount",
        "draft_currency",
        "order_amount",
        "cancelled",
        "unpaid",
        "conflict",
        "unconfirmed",
    ] {
        let provider = Fixture::new(paid().into_iter().map(Reply::json).collect()).await;
        let mut d = draft(true);
        let mut o = order(tags());
        match field {
            "draft_amount" => {
                d["data"]["draftOrder"]["totalPriceSet"]["presentmentMoney"]["amount"] =
                    json!("12.51");
            }
            "draft_currency" => {
                d["data"]["draftOrder"]["totalPriceSet"]["presentmentMoney"]["currencyCode"] =
                    json!("USD");
            }
            "order_amount" => {
                o["data"]["order"]["totalPriceSet"]["presentmentMoney"]["amount"] = json!("12.51");
            }
            "cancelled" => o["data"]["order"]["cancelledAt"] = json!("2026-10-23"),
            "unpaid" => o["data"]["order"]["displayFinancialStatus"] = json!("REFUNDED"),
            "conflict" => o["data"]["order"]["tags"] = json!(["payment:stripe:charge:ch_other"]),
            _ => o["data"]["order"]["tags"] = json!([]),
        }
        let shop = Fixture::new(
            vec![d, o.clone(), tagged(), o]
                .into_iter()
                .map(Reply::json)
                .collect(),
        )
        .await;
        assert!(
            run(&shop, &provider, CompletionMode::CompleteIfOpen)
                .await
                .is_err(),
            "{field}"
        );
        assert!(!operations(&shop).contains(&"CompleteCheckoutDraft".into()));
    }
}

#[tokio::test]
async fn paypal_verified_paid_completion_reuses_authoritative_verifier_and_real_tags() {
    let provider = Fixture::new(paypal_objects().into_iter().map(Reply::json).collect()).await;
    let wanted = vec![
        "payment:paypal:order:ORDER".into(),
        "payment:paypal:capture:CAPTURE".into(),
    ];
    let shop = Fixture::new(
        vec![draft(true), order(wanted)]
            .into_iter()
            .map(Reply::json)
            .collect(),
    )
    .await;
    let result = run_paypal(&shop, &provider).await.unwrap();
    assert!(matches!(result, CompletionOutcome::Completed { .. }));
    assert_eq!(operations(&shop), ["CheckoutDraft", "CheckoutOrder"]);
    assert_eq!(
        provider
            .requests()
            .iter()
            .map(|r| r.0.as_str())
            .collect::<Vec<_>>(),
        [
            "/v1/oauth2/token",
            "/v2/checkout/orders/ORDER",
            "/v2/payments/captures/CAPTURE"
        ]
    );
}
#[tokio::test]
async fn paypal_pending_and_invalid_provider_proof_never_mutate_shopify() {
    for field in ["pending", "merchant", "amount", "refunded", "reference"] {
        let mut objects = paypal_objects();
        match field {
            "pending" => {
                objects[1]["status"] = json!("APPROVED");
                objects[1]["purchase_units"][0]["payments"] = Value::Null;
            }
            "merchant" => objects[1]["purchase_units"][0]["payee"]["merchant_id"] = json!("OTHER"),
            "amount" => objects[2]["amount"]["value"] = json!("12.51"),
            "refunded" => objects[2]["status"] = json!("REFUNDED"),
            _ => objects[1]["purchase_units"][0]["custom_id"] = json!("OTHER"),
        }
        let provider = Fixture::new(objects.into_iter().map(Reply::json).collect()).await;
        let shop = Fixture::new(vec![]).await;
        assert_eq!(
            run_paypal(&shop, &provider).await.is_ok(),
            field == "pending",
            "{field}"
        );
        assert!(shop.requests().is_empty(), "{field}");
    }
}
async fn run_paypal(shop: &Fixture, provider: &Fixture) -> Result<CompletionOutcome, AppError> {
    run_paypal_currency(shop, provider, "THB").await
}
async fn run_paypal_currency(
    shop: &Fixture,
    provider: &Fixture,
    currency: &str,
) -> Result<CompletionOutcome, AppError> {
    let client = PaypalClient::new(&crate::config::PaypalConfig {
        base_url: provider.url.join("/").unwrap().to_string(),
        timeout_secs: 1,
        client_id: Some("fixture".into()),
        client_secret: Some("fixture".into()),
    })
    .unwrap();
    let expected = paypal::payment::ExpectedPayment {
        order_id: "ORDER".into(),
        merchant_id: "MERCHANT".into(),
        api_origin: provider.url.origin().ascii_serialization(),
        reference_id: "unit".into(),
        custom_id: "draft-ref".into(),
        amount: 1250,
        currency: currency.into(),
    };
    complete_paid_draft(
        &crate::services::common::client(shop),
        &id("DraftOrder", 1),
        PaymentContext::Paypal {
            client: &client,
            expected: &expected,
        },
        CompletionMode::CompleteIfOpen,
    )
    .await
}
fn paypal_objects() -> Vec<Value> {
    let capture = json!({"id":"CAPTURE","status":"COMPLETED","amount":{"currency_code":"THB","value":"12.50"},"final_capture":true,"payee":{"merchant_id":"MERCHANT"},"custom_id":"draft-ref","supplementary_data":{"related_ids":{"order_id":"ORDER","capture_id":"CAPTURE"}}});
    vec![
        json!({"access_token":"fixture-token","token_type":"Bearer","expires_in":3600}),
        json!({"id":"ORDER","status":"COMPLETED","purchase_units":[{"reference_id":"unit","custom_id":"draft-ref","amount":{"currency_code":"THB","value":"12.50"},"payee":{"merchant_id":"MERCHANT"},"payments":{"captures":[capture.clone()]}}]}),
        capture,
    ]
}

#[tokio::test]
async fn paypal_zero_decimal_units_are_converted_exactly_to_shopify_currency_precision() {
    for currency in ["HUF", "TWD", "JPY"] {
        let mut objects = paypal_objects();
        for money in [
            "/purchase_units/0/amount",
            "/purchase_units/0/payments/captures/0/amount",
        ] {
            let value = objects[1].pointer_mut(money).unwrap();
            value["currency_code"] = json!(currency);
            value["value"] = json!("1250");
        }
        objects[2]["amount"] = json!({"currency_code":currency,"value":"1250"});
        let provider = Fixture::new(objects.into_iter().map(Reply::json).collect()).await;
        let mut d = draft(true);
        let mut o = order(vec![
            "payment:paypal:order:ORDER".into(),
            "payment:paypal:capture:CAPTURE".into(),
        ]);
        d["data"]["draftOrder"]["totalPriceSet"]["presentmentMoney"] =
            json!({"amount":"1250.00","currencyCode":currency});
        o["data"]["order"]["totalPriceSet"]["presentmentMoney"] =
            json!({"amount":"1250.00","currencyCode":currency});
        let shop = Fixture::new(vec![d, o].into_iter().map(Reply::json).collect()).await;
        run_paypal_currency(&shop, &provider, currency)
            .await
            .unwrap();
    }
}

#[tokio::test]
async fn unusable_completion_payload_reconciles_without_replaying_the_mutation() {
    for reply in [
        json!({"data":{"draftOrderComplete":{"draftOrder":null,"userErrors":[]}}}),
        json!({"data":{"draftOrderComplete":{"draftOrder":{"id":"gid://shopify/DraftOrder/1","order":null},"userErrors":[]}}}),
        json!({"data":{}}),
    ] {
        for visible in [true, false] {
            let provider = Fixture::new(paid().into_iter().map(Reply::json).collect()).await;
            let mut replies = vec![draft(false), reply.clone(), draft(visible)];
            if visible {
                replies.push(order(tags()));
            }
            let shop = Fixture::new(replies.into_iter().map(Reply::json).collect()).await;
            let result = run(&shop, &provider, CompletionMode::CompleteIfOpen).await;
            assert_eq!(result.is_ok(), visible);
            if let Err(error) = result {
                assert!(error.to_string().contains("ReconcileOnly"));
                assert!(
                    error
                        .find_source::<crate::client::shopify::CheckoutIdentityMismatch>()
                        .is_none()
                );
                assert!(error.find_source::<crate::models::checkout_completion::CompletionEvidenceConflict>().is_none());
            }
            assert_eq!(
                operations(&shop)
                    .iter()
                    .filter(|name| name.as_str() == "CompleteCheckoutDraft")
                    .count(),
                1
            );
        }
    }
}
#[tokio::test]
async fn completed_draft_with_wrong_linked_resource_keeps_upstream_identity_failure() {
    use axum::response::IntoResponse;

    for mode in [
        CompletionMode::CompleteIfOpen,
        CompletionMode::ReconcileOnly,
    ] {
        let provider = Fixture::new(paid().into_iter().map(Reply::json).collect()).await;
        let mut response = draft(true);
        response["data"]["draftOrder"]["order"]["id"] = json!("gid://shopify/Product/2");
        let shop = Fixture::new(vec![Reply::json(response)]).await;
        let error = run(&shop, &provider, mode).await.unwrap_err();
        assert!(
            error
                .find_source::<crate::client::shopify::CheckoutIdentityMismatch>()
                .is_some()
        );
        let response = error.into_response();
        assert_eq!(response.status(), axum::http::StatusCode::BAD_GATEWAY);
        let body = axum::body::to_bytes(response.into_body(), 1024)
            .await
            .unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&body).unwrap(),
            json!({"error":"Upstream request failed"})
        );
        assert_eq!(operations(&shop), ["CheckoutDraft"]);
    }
}

#[tokio::test]
async fn wrong_returned_draft_identity_never_authorizes_an_order() {
    let provider = Fixture::new(paid().into_iter().map(Reply::json).collect()).await;
    let mut response = completed();
    response["data"]["draftOrderComplete"]["draftOrder"]["id"] =
        json!("gid://shopify/DraftOrder/3");
    let shop = Fixture::new(
        vec![draft(false), response]
            .into_iter()
            .map(Reply::json)
            .collect(),
    )
    .await;
    let error = run(&shop, &provider, CompletionMode::CompleteIfOpen)
        .await
        .unwrap_err();
    assert!(
        error
            .find_source::<crate::client::shopify::CheckoutIdentityMismatch>()
            .is_some()
    );
    assert_eq!(
        operations(&shop),
        ["CheckoutDraft", "CompleteCheckoutDraft"]
    );
}

#[tokio::test]
async fn conflicting_open_draft_references_prevent_irreversible_completion() {
    for status in ["OPEN", "INVOICE_SENT"] {
        let provider = Fixture::new(paid().into_iter().map(Reply::json).collect()).await;
        let mut d = draft(false);
        d["data"]["draftOrder"]["status"] = json!(status);
        d["data"]["draftOrder"]["tags"] = json!(["OTHER", "payment:stripe:charge:ch_other"]);
        let shop = Fixture::new(vec![Reply::json(d)]).await;
        assert!(
            run(&shop, &provider, CompletionMode::CompleteIfOpen)
                .await
                .is_err()
        );
        assert_eq!(operations(&shop), ["CheckoutDraft"]);
    }
}
#[tokio::test]
async fn completed_replay_uses_real_order_tags_even_when_draft_tags_are_stale() {
    let provider = Fixture::new(paid().into_iter().map(Reply::json).collect()).await;
    let mut d = draft(true);
    d["data"]["draftOrder"]["tags"] = json!(["payment:stripe:charge:ch_stale"]);
    let shop = Fixture::new(
        vec![d, order(tags())]
            .into_iter()
            .map(Reply::json)
            .collect(),
    )
    .await;
    run(&shop, &provider, CompletionMode::ReconcileOnly)
        .await
        .unwrap();
    assert_eq!(operations(&shop), ["CheckoutDraft", "CheckoutOrder"]);
}

#[tokio::test]
async fn before_completion_hook_runs_only_after_reads_and_immediately_before_mutation() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let calls = AtomicUsize::new(0);
    let provider = Fixture::new(paid().into_iter().map(Reply::json).collect()).await;
    let shop = Fixture::new(vec![Reply::json(json!({"data":{"draftOrder":null}}))]).await;
    let result = complete_paid_draft_with_before_completion(
        &client(&shop),
        &id("DraftOrder", 1),
        PaymentContext::Stripe {
            client: &stripe_client(&provider),
            expected: &expected(),
        },
        CompletionMode::CompleteIfOpen,
        || async {
            calls.fetch_add(1, Ordering::SeqCst);
            Ok(())
        },
    )
    .await;
    assert!(result.is_err());
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(operations(&shop), ["CheckoutDraft"]);

    let provider = Fixture::new(paid().into_iter().map(Reply::json).collect()).await;
    let shop = Fixture::new(
        vec![draft(false), completed(), order(tags())]
            .into_iter()
            .map(Reply::json)
            .collect(),
    )
    .await;
    let result = complete_paid_draft_with_before_completion(
        &client(&shop),
        &id("DraftOrder", 1),
        PaymentContext::Stripe {
            client: &stripe_client(&provider),
            expected: &expected(),
        },
        CompletionMode::CompleteIfOpen,
        || async {
            assert_eq!(operations(&shop), ["CheckoutDraft"]);
            calls.fetch_add(1, Ordering::SeqCst);
            Ok(())
        },
    )
    .await;
    assert!(matches!(
        result.unwrap(),
        CompletionOutcome::Completed { .. }
    ));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn failed_before_completion_hook_prevents_provider_mutation() {
    let provider = Fixture::new(paid().into_iter().map(Reply::json).collect()).await;
    let shop = Fixture::new(vec![Reply::json(draft(false))]).await;
    let result = complete_paid_draft_with_before_completion(
        &client(&shop),
        &id("DraftOrder", 1),
        PaymentContext::Stripe {
            client: &stripe_client(&provider),
            expected: &expected(),
        },
        CompletionMode::CompleteIfOpen,
        || async { Err(conflict("Claim no longer owned")) },
    )
    .await;
    assert!(result.is_err());
    assert_eq!(operations(&shop), ["CheckoutDraft"]);
}

#[tokio::test]
async fn linked_completed_order_never_calls_before_completion_hook() {
    let provider = Fixture::new(paid().into_iter().map(Reply::json).collect()).await;
    let shop = Fixture::new(
        vec![draft(true), order(tags())]
            .into_iter()
            .map(Reply::json)
            .collect(),
    )
    .await;
    complete_paid_draft_with_before_completion(
        &client(&shop),
        &id("DraftOrder", 1),
        PaymentContext::Stripe {
            client: &stripe_client(&provider),
            expected: &expected(),
        },
        CompletionMode::ReconcileOnly,
        || async { panic!("A linked completed order must not invoke the mutation hook") },
    )
    .await
    .unwrap();
    assert_eq!(operations(&shop), ["CheckoutDraft", "CheckoutOrder"]);
}
