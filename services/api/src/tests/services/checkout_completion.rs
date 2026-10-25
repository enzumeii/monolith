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

#[test]
fn payment_references_fit_shopify_tags_without_losing_kind_or_id_binding() {
    let boundary = "s".repeat(17);
    assert_eq!(
        reference("stripe:session", &boundary).unwrap(),
        format!("payment:stripe:session:{boundary}")
    );
    let session_id = format!("cs_test_{}", "s".repeat(90));
    let tag = reference("stripe:session", &session_id).unwrap();
    assert_eq!(tag, "payment:XvGzmVJpPMv7bbJ-aojDbsffsYUoKToE");
    assert_eq!(tag.len(), 40);
    assert_eq!(reference("stripe:session", &session_id).unwrap(), tag);
    assert_ne!(reference("stripe:intent", &session_id).unwrap(), tag);
    assert_ne!(
        reference("stripe:session", &format!("{session_id}x")).unwrap(),
        tag
    );
    assert!(validate_references(std::slice::from_ref(&tag), std::slice::from_ref(&tag)).is_ok());
    assert!(
        validate_references(
            &[reference("stripe:intent", &session_id).unwrap()],
            &[reference("stripe:session", &session_id).unwrap()],
        )
        .is_err()
    );
    for id in [
        String::new(),
        "bad id".into(),
        "bad:id".into(),
        "bad,id".into(),
        "bad\nid".into(),
        "s".repeat(234),
    ] {
        assert!(reference("stripe:session", &id).is_err());
    }
}

#[tokio::test]
async fn real_length_stripe_references_reconcile_completed_order_without_completing_again() {
    let session_id = format!("cs_test_{}", "s".repeat(90));
    let intent_id = format!("pi_{}", "i".repeat(24));
    let charge_id = format!("ch_{}", "c".repeat(24));
    let mut payment = paid();
    payment[1]["id"] = json!(session_id);
    payment[1]["payment_intent"] = json!(intent_id);
    payment[2]["id"] = json!(intent_id);
    payment[2]["latest_charge"] = json!(charge_id);
    payment[3]["id"] = json!(charge_id);
    payment[3]["payment_intent"] = json!(intent_id);
    let expected_tags: Vec<String> = [
        "payment:XvGzmVJpPMv7bbJ-aojDbsffsYUoKToE",
        "payment:2-Ef_Z49_CRUW64jCSl5pQ6TW-Qr5vXs",
        "payment:ooiDKpiL97rsciUSdw0YAQXYcjotae4X",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    let mut expected = expected();
    expected.session_id = session_id;
    for mode in [
        CompletionMode::CompleteIfOpen,
        CompletionMode::ReconcileOnly,
    ] {
        let provider = Fixture::new(payment.iter().cloned().map(Reply::json).collect()).await;
        let shop = Fixture::new(
            [
                draft(true),
                order(vec![]),
                tagged(),
                order(expected_tags.clone()),
            ]
            .into_iter()
            .map(Reply::json)
            .collect(),
        )
        .await;
        let outcome = complete_paid_draft(
            &client(&shop),
            &id("DraftOrder", 1),
            PaymentContext::Stripe {
                client: &stripe_client(&provider),
                expected: &expected,
            },
            mode,
        )
        .await
        .unwrap();
        assert!(matches!(outcome, CompletionOutcome::Completed { .. }));
        assert_eq!(
            operations(&shop),
            [
                "CheckoutDraft",
                "CheckoutOrder",
                "EditOrderTags",
                "CheckoutOrder"
            ]
        );
        assert_eq!(
            shop.requests()[2].2["variables"]["tags"],
            json!(expected_tags)
        );
    }
}

#[derive(Default)]
struct Progress(Vec<CompletionStage>);
impl CompletionProgress for Progress {
    fn before_mutation(
        &mut self,
        stage: CompletionStage,
    ) -> impl std::future::Future<Output = Result<(), AppError>> + Send {
        self.0.push(stage);
        std::future::ready(Ok(()))
    }
}
fn external_order(paid: bool) -> Value {
    let mut value = order(tags());
    let order = &mut value["data"]["order"];
    order["tags"]
        .as_array_mut()
        .unwrap()
        .push(json!("qa-convention"));
    order["customAttributes"] = json!([{"key":"convention_slug","value":"qa-convention"}]);
    order["displayFinancialStatus"] = json!(if paid { "PAID" } else { "PENDING" });
    order["totalOutstandingSet"] =
        json!({"presentmentMoney":{"amount":if paid {"0.00"} else {"12.50"},"currencyCode":"THB"}});
    order["transactions"] = if paid {
        json!([{"id":"gid://shopify/OrderTransaction/3","gateway":"Stripe Card Payments","kind":"SALE","status":"SUCCESS","amountSet":{"presentmentMoney":{"amount":"12.50","currencyCode":"THB"}}}])
    } else {
        json!([])
    };
    value
}
fn external_draft(completed: bool) -> Value {
    let mut value = draft(completed);
    value["data"]["draftOrder"]["tags"] = json!(["qa-convention"]);
    value["data"]["draftOrder"]["customAttributes"] =
        json!([{"key":"convention_slug","value":"qa-convention"}]);
    value
}
async fn run_external(
    shop: &Fixture,
    state: crate::models::checkout::PaymentRecordingState,
    mode: CompletionMode,
) -> (Result<CompletionOutcome, AppError>, Progress) {
    let provider = Fixture::new(paid().into_iter().map(Reply::json).collect()).await;
    let hashes = vec![Sha256::digest(b"qa-convention").to_vec()];
    let mut progress = Progress::default();
    let result = complete_external_paid_draft(
        &client(shop),
        &id("DraftOrder", 1),
        PaymentContext::Stripe {
            client: &stripe_client(&provider),
            expected: &expected(),
        },
        mode,
        ExternalPaymentRecording {
            money_snapshots: None,
            state,
            required_tag_hashes: &hashes,
            convention_slug: "qa-convention",
            identity: None,
            provider_resource_id: Some("cs_real"),
        },
        &mut progress,
    )
    .await;
    (result, progress)
}
#[tokio::test]
async fn external_payment_converts_pending_then_records_named_payment_with_two_checkpoints() {
    let shop=Fixture::new(vec![external_draft(false),completed(),external_order(false),json!({"data":{"orderCreateManualPayment":{"order":{"id":"gid://shopify/Order/2"},"userErrors":[]}}}),external_order(true)].into_iter().map(Reply::json).collect()).await;
    let (result, progress) = run_external(
        &shop,
        crate::models::checkout::PaymentRecordingState::NotStarted,
        CompletionMode::CompleteIfOpen,
    )
    .await;
    assert!(matches!(
        result.unwrap(),
        CompletionOutcome::Completed { .. }
    ));
    assert_eq!(
        progress.0,
        vec![
            CompletionStage::ConvertDraft,
            CompletionStage::RecordExternalPayment
        ]
    );
    let requests = shop.requests();
    assert_eq!(requests[1].2["variables"]["paymentPending"], true);
    assert_eq!(requests[3].2["operationName"], "RecordCheckoutPayment");
    assert_eq!(
        requests[3].2["variables"],
        json!({"id":"gid://shopify/Order/2","paymentMethodName":"Stripe Card Payments"})
    );
}
#[tokio::test]
async fn external_lost_recording_response_reconciles_paid_order_without_another_mutation() {
    let shop = Fixture::new(vec![
        Reply::json(external_draft(true)),
        Reply::json(external_order(true)),
    ])
    .await;
    let (result, progress) = run_external(
        &shop,
        crate::models::checkout::PaymentRecordingState::Recording,
        CompletionMode::ReconcileOnly,
    )
    .await;
    assert!(matches!(
        result.unwrap(),
        CompletionOutcome::Completed { .. }
    ));
    assert_eq!(progress.0, Vec::new());
    assert_eq!(operations(&shop), vec!["CheckoutDraft", "CheckoutOrder"]);
}
#[tokio::test]
async fn external_uncertain_recording_with_unpaid_order_never_records_twice() {
    let shop = Fixture::new(vec![
        Reply::json(external_draft(true)),
        Reply::json(external_order(false)),
    ])
    .await;
    let (result, progress) = run_external(
        &shop,
        crate::models::checkout::PaymentRecordingState::Uncertain,
        CompletionMode::ReconcileOnly,
    )
    .await;
    assert!(result.is_err());
    assert_eq!(progress.0, Vec::new());
    assert_eq!(operations(&shop), vec!["CheckoutDraft", "CheckoutOrder"]);
}
#[tokio::test]
async fn external_wrong_gateway_or_money_never_reports_completion() {
    for (field, replacement) in [
        ("gateway", json!("manual")),
        (
            "amountSet",
            json!({"presentmentMoney":{"amount":"12.51","currencyCode":"THB"}}),
        ),
    ] {
        let mut value = external_order(true);
        value["data"]["order"]["transactions"][0][field] = replacement;
        let shop = Fixture::new(vec![Reply::json(external_draft(true)), Reply::json(value)]).await;
        let (result, progress) = run_external(
            &shop,
            crate::models::checkout::PaymentRecordingState::Recording,
            CompletionMode::ReconcileOnly,
        )
        .await;
        assert!(result.is_err());
        assert_eq!(progress.0, Vec::new());
    }
}

#[tokio::test]
async fn external_lost_conversion_response_recovers_only_the_linked_order() {
    let shop=Fixture::new(vec![Reply::json(external_draft(false)),Reply::status(500),Reply::json(external_draft(true)),Reply::json(external_order(false)),Reply::json(json!({"data":{"orderCreateManualPayment":{"order":{"id":"gid://shopify/Order/2"},"userErrors":[]}}})),Reply::json(external_order(true))]).await;
    let (result, progress) = run_external(
        &shop,
        crate::models::checkout::PaymentRecordingState::NotStarted,
        CompletionMode::CompleteIfOpen,
    )
    .await;
    assert!(matches!(
        result.unwrap(),
        CompletionOutcome::Completed { .. }
    ));
    assert_eq!(
        progress.0,
        vec![
            CompletionStage::ConvertDraft,
            CompletionStage::RecordExternalPayment
        ]
    );
    assert_eq!(
        operations(&shop)
            .iter()
            .filter(|operation| operation.as_str() == "CompleteCheckoutDraft")
            .count(),
        1
    );
}
#[tokio::test]
async fn external_unlinked_uncertain_conversion_never_repeats_conversion() {
    let shop = Fixture::new(vec![Reply::json(external_draft(false))]).await;
    let (result, progress) = run_external(
        &shop,
        crate::models::checkout::PaymentRecordingState::NotStarted,
        CompletionMode::ReconcileOnly,
    )
    .await;
    assert!(result.is_err());
    assert_eq!(progress.0, Vec::new());
    assert_eq!(operations(&shop), vec!["CheckoutDraft"]);
}
#[test]
fn external_paid_confirmation_rejects_conflicting_truth_and_requires_full_named_sale() {
    let money = exact_money(1250, "THB").unwrap();
    let hashes = vec![Sha256::digest(b"qa-convention").to_vec()];
    let recording = ExternalPaymentRecording {
        money_snapshots: None,
        state: crate::models::checkout::PaymentRecordingState::Recording,
        required_tag_hashes: &hashes,
        convention_slug: "qa-convention",
        identity: None,
        provider_resource_id: Some("cs_real"),
    };
    let value = external_order(true)["data"]["order"].clone();
    let order = serde_json::from_value(value.clone()).unwrap();
    assert!(
        validate_external_paid_order(&order, &money, &recording, "Stripe Card Payments").is_ok()
    );
    for (pointer, replacement) in [
        ("/transactions/0/gateway", json!("manual")),
        ("/transactions/0/kind", json!("REFUND")),
        ("/transactions/0/status", json!("PENDING")),
        ("/transactions/0/id", json!("gid://shopify/Order/3")),
        (
            "/transactions/0/amountSet/presentmentMoney/amount",
            json!("12.51"),
        ),
        (
            "/transactions/0/amountSet/presentmentMoney/currencyCode",
            json!("AUD"),
        ),
        (
            "/totalOutstandingSet/presentmentMoney/amount",
            json!("0.01"),
        ),
        ("/totalPriceSet/presentmentMoney/amount", json!("12.49")),
        ("/cancelledAt", json!("2026-10-24T00:00:00Z")),
        ("/customAttributes/0/value", json!("changed-slug")),
        ("/tags", json!([])),
        ("/transactions", json!([])),
        ("/totalOutstandingSet", json!(null)),
    ] {
        let mut changed = value.clone();
        *changed.pointer_mut(pointer).unwrap() = replacement;
        assert!(
            validate_external_paid_order(
                &serde_json::from_value(changed).unwrap(),
                &money,
                &recording,
                "Stripe Card Payments"
            )
            .is_err(),
            "accepted {pointer}"
        );
    }
    let mut duplicate = value.clone();
    duplicate["transactions"]
        .as_array_mut()
        .unwrap()
        .push(value["transactions"][0].clone());
    assert!(
        validate_external_paid_order(
            &serde_json::from_value(duplicate).unwrap(),
            &money,
            &recording,
            "Stripe Card Payments"
        )
        .is_err()
    );
    let mut paypal = value;
    paypal["transactions"][0]["gateway"] = json!("paypal");
    assert!(
        validate_external_paid_order(
            &serde_json::from_value(paypal).unwrap(),
            &money,
            &recording,
            "paypal"
        )
        .is_ok()
    );
}

#[tokio::test]
async fn external_recording_rejects_wrong_mutation_order_identity_even_if_readback_is_paid() {
    let shop=Fixture::new(vec![external_draft(true),external_order(false),json!({"data":{"orderCreateManualPayment":{"order":{"id":"gid://shopify/Order/999"},"userErrors":[]}}}),external_order(true)].into_iter().map(Reply::json).collect()).await;
    let (result, progress) = run_external(
        &shop,
        crate::models::checkout::PaymentRecordingState::NotStarted,
        CompletionMode::CompleteIfOpen,
    )
    .await;
    assert!(result.is_err());
    assert_eq!(progress.0, vec![CompletionStage::RecordExternalPayment]);
}

#[tokio::test]
async fn external_lost_recording_response_reads_success_without_retrying_the_mutation() {
    let shop = Fixture::new(vec![
        Reply::json(external_draft(true)),
        Reply::json(external_order(false)),
        Reply::status(500),
        Reply::json(external_order(true)),
    ])
    .await;
    let (result, progress) = run_external(
        &shop,
        crate::models::checkout::PaymentRecordingState::NotStarted,
        CompletionMode::CompleteIfOpen,
    )
    .await;
    assert!(matches!(
        result.unwrap(),
        CompletionOutcome::Completed { .. }
    ));
    assert_eq!(progress.0, vec![CompletionStage::RecordExternalPayment]);
    assert_eq!(
        operations(&shop)
            .iter()
            .filter(|operation| operation.as_str() == "RecordCheckoutPayment")
            .count(),
        1
    );
}

fn metadata_identity(provider: &str) -> correlation::Identity {
    correlation::Identity {
        checkout_id: uuid::Uuid::parse_str("2081e145-a446-411b-b86c-48aad58c5255").unwrap(),
        convention_id: uuid::Uuid::parse_str("edb37c06-23c6-498e-906a-28efb0ef40f5").unwrap(),
        convention_slug: "qa-convention".into(),
        provider: provider.into(),
    }
}
fn metadata_references(provider: &str) -> Vec<CustomAttribute> {
    if provider == "stripe" {
        vec![
            payment_attribute(correlation::STRIPE_SESSION_KEY, "cs_real"),
            payment_attribute(correlation::STRIPE_INTENT_KEY, "pi_real"),
            payment_attribute(correlation::STRIPE_CHARGE_KEY, "ch_real"),
        ]
    } else {
        vec![
            payment_attribute(correlation::PAYPAL_ORDER_KEY, "ORDER"),
            payment_attribute(correlation::PAYPAL_CAPTURE_KEY, "CAPTURE"),
        ]
    }
}
fn metadata_draft(completed: bool, provider: &str) -> Value {
    let mut value = external_draft(completed);
    value["data"]["draftOrder"]["customAttributes"] =
        serde_json::to_value(metadata_identity(provider).attributes()).unwrap();
    value
}
fn metadata_order(paid: bool, references: bool, provider: &str) -> Value {
    let mut value = external_order(paid);
    let order = &mut value["data"]["order"];
    order["tags"] = json!(["qa-convention"]);
    let mut attributes = metadata_identity(provider).attributes();
    attributes.push(payment_attribute("merchant_note", "Preserve this note"));
    if references {
        attributes.extend(metadata_references(provider));
    }
    order["customAttributes"] = serde_json::to_value(attributes).unwrap();
    if paid {
        order["transactions"][0]["gateway"] = json!("manual");
    }
    value
}
fn attributes_updated() -> Value {
    json!({"data":{"orderUpdate":{"order":{"id":"gid://shopify/Order/2"},"userErrors":[]}}})
}
fn payment_recorded() -> Value {
    json!({"data":{"orderCreateManualPayment":{"order":{"id":"gid://shopify/Order/2"},"userErrors":[]}}})
}
async fn run_metadata(
    shop: &Fixture,
    state: crate::models::checkout::PaymentRecordingState,
    mode: CompletionMode,
) -> (Result<CompletionOutcome, AppError>, Progress) {
    let provider = Fixture::new(paid().into_iter().map(Reply::json).collect()).await;
    let mut progress = Progress::default();
    let result = complete_external_paid_draft(
        &client(shop),
        &id("DraftOrder", 1),
        PaymentContext::Stripe {
            client: &stripe_client(&provider),
            expected: &expected(),
        },
        mode,
        ExternalPaymentRecording {
            money_snapshots: None,
            state,
            required_tag_hashes: &[],
            convention_slug: "qa-convention",
            identity: Some(metadata_identity("stripe")),
            provider_resource_id: Some("cs_real"),
        },
        &mut progress,
    )
    .await;
    (result, progress)
}
#[tokio::test]
async fn metadata_completion_records_default_manual_once_and_preserves_attributes_without_tag_writes()
 {
    let shop = Fixture::new(
        [
            metadata_draft(false, "stripe"),
            completed(),
            metadata_order(false, false, "stripe"),
            payment_recorded(),
            metadata_order(true, false, "stripe"),
            attributes_updated(),
            metadata_order(true, true, "stripe"),
        ]
        .into_iter()
        .map(Reply::json)
        .collect(),
    )
    .await;
    let (result, progress) = run_metadata(
        &shop,
        crate::models::checkout::PaymentRecordingState::NotStarted,
        CompletionMode::CompleteIfOpen,
    )
    .await;
    assert!(matches!(
        result.unwrap(),
        CompletionOutcome::Completed { .. }
    ));
    assert_eq!(
        progress.0,
        [
            CompletionStage::ConvertDraft,
            CompletionStage::RecordExternalPayment
        ]
    );
    assert_eq!(
        operations(&shop),
        [
            "CheckoutDraft",
            "CompleteCheckoutDraft",
            "CheckoutOrder",
            "RecordCheckoutPayment",
            "CheckoutOrder",
            "UpdateOrderCustomAttributes",
            "CheckoutOrder"
        ]
    );
    let requests = shop.requests();
    assert_eq!(
        requests[3].2["variables"],
        json!({"id":"gid://shopify/Order/2"})
    );
    let attributes = requests[5].2["variables"]["input"]["customAttributes"]
        .as_array()
        .unwrap();
    assert_eq!(
        attributes.len(),
        metadata_identity("stripe").attributes().len() + 4
    );
    assert!(attributes.contains(&json!({"key":"merchant_note","value":"Preserve this note"})));
    for attribute in metadata_references("stripe") {
        assert!(attributes.contains(&serde_json::to_value(attribute).unwrap()));
    }
}
#[tokio::test]
async fn metadata_lost_attribute_response_reconciles_exact_readback_without_another_financial_write()
 {
    let shop = Fixture::new(vec![
        Reply::json(metadata_draft(true, "stripe")),
        Reply::json(metadata_order(true, false, "stripe")),
        Reply::status(500),
        Reply::json(metadata_order(true, true, "stripe")),
    ])
    .await;
    let (result, progress) = run_metadata(
        &shop,
        crate::models::checkout::PaymentRecordingState::Uncertain,
        CompletionMode::ReconcileOnly,
    )
    .await;
    assert!(matches!(
        result.unwrap(),
        CompletionOutcome::Completed { .. }
    ));
    assert_eq!(progress.0, Vec::new());
    assert_eq!(
        operations(&shop),
        [
            "CheckoutDraft",
            "CheckoutOrder",
            "UpdateOrderCustomAttributes",
            "CheckoutOrder"
        ]
    );
}
#[tokio::test]
async fn metadata_missing_attribute_readback_retries_only_the_idempotent_reference_write() {
    let shop = Fixture::new(
        [
            metadata_draft(true, "stripe"),
            metadata_order(true, false, "stripe"),
            attributes_updated(),
            metadata_order(true, false, "stripe"),
            metadata_draft(true, "stripe"),
            metadata_order(true, false, "stripe"),
            attributes_updated(),
            metadata_order(true, true, "stripe"),
        ]
        .into_iter()
        .map(Reply::json)
        .collect(),
    )
    .await;
    let (first, _) = run_metadata(
        &shop,
        crate::models::checkout::PaymentRecordingState::Uncertain,
        CompletionMode::ReconcileOnly,
    )
    .await;
    assert!(first.is_err());
    let (second, progress) = run_metadata(
        &shop,
        crate::models::checkout::PaymentRecordingState::Uncertain,
        CompletionMode::ReconcileOnly,
    )
    .await;
    assert!(matches!(
        second.unwrap(),
        CompletionOutcome::Completed { .. }
    ));
    assert_eq!(progress.0, Vec::new());
    assert!(operations(&shop).iter().all(|operation| {
        ![
            "RecordCheckoutPayment",
            "CompleteCheckoutDraft",
            "EditOrderTags",
        ]
        .contains(&operation.as_str())
    }));
}
#[tokio::test]
async fn metadata_conflicting_payment_references_stop_before_any_financial_or_attribute_write() {
    for extra in [
        payment_attribute(correlation::STRIPE_CHARGE_KEY, "ch_other"),
        payment_attribute(correlation::PAYPAL_CAPTURE_KEY, "CAPTURE"),
        payment_attribute(correlation::STRIPE_CHARGE_KEY, "ch_real"),
    ] {
        let mut value = metadata_order(true, true, "stripe");
        value["data"]["order"]["customAttributes"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::to_value(extra).unwrap());
        let shop = Fixture::new(
            [metadata_draft(true, "stripe"), value]
                .into_iter()
                .map(Reply::json)
                .collect(),
        )
        .await;
        let (result, progress) = run_metadata(
            &shop,
            crate::models::checkout::PaymentRecordingState::Uncertain,
            CompletionMode::ReconcileOnly,
        )
        .await;
        assert!(result.is_err());
        assert_eq!(progress.0, Vec::new());
        assert_eq!(operations(&shop), ["CheckoutDraft", "CheckoutOrder"]);
    }
}
#[test]
fn metadata_manual_paid_proof_requires_exact_identity_full_references_and_original_resource() {
    let money = exact_money(1250, "THB").unwrap();
    let mut recording = ExternalPaymentRecording {
        money_snapshots: None,
        state: crate::models::checkout::PaymentRecordingState::Uncertain,
        required_tag_hashes: &[],
        convention_slug: "qa-convention",
        identity: Some(metadata_identity("stripe")),
        provider_resource_id: Some("cs_real"),
    };
    let value = metadata_order(true, true, "stripe")["data"]["order"].clone();
    let order = serde_json::from_value(value.clone()).unwrap();
    assert!(
        validate_external_paid_order(&order, &money, &recording, "Stripe Card Payments").is_ok()
    );
    recording.provider_resource_id = Some("cs_other");
    assert!(
        validate_external_paid_order(&order, &money, &recording, "Stripe Card Payments").is_err()
    );
    recording.provider_resource_id = Some("cs_real");
    for (pointer, replacement) in [
        ("/transactions/0/gateway", json!("Stripe Card Payments")),
        ("/transactions/0/kind", json!("REFUND")),
        ("/tags", json!(["qa-convention", "co:old-clutter"])),
        ("/customAttributes", json!([])),
    ] {
        let mut changed = value.clone();
        *changed.pointer_mut(pointer).unwrap() = replacement;
        assert!(
            validate_external_paid_order(
                &serde_json::from_value(changed).unwrap(),
                &money,
                &recording,
                "Stripe Card Payments"
            )
            .is_err(),
            "accepted {pointer}"
        );
    }
    let absent = metadata_order(true, false, "stripe")["data"]["order"].clone();
    assert!(
        validate_external_paid_order(
            &serde_json::from_value(absent).unwrap(),
            &money,
            &recording,
            "Stripe Card Payments"
        )
        .is_err()
    );
    let mut refund = value.clone();
    let mut transaction = value["transactions"][0].clone();
    transaction["kind"] = json!("REFUND");
    transaction["status"] = json!("PENDING");
    refund["transactions"]
        .as_array_mut()
        .unwrap()
        .push(transaction);
    assert!(
        validate_external_paid_order(
            &serde_json::from_value(refund).unwrap(),
            &money,
            &recording,
            "Stripe Card Payments"
        )
        .is_err()
    );
}
#[tokio::test]
async fn metadata_paypal_records_manual_and_keeps_exact_order_capture_attributes() {
    let provider = Fixture::new(paypal_objects().into_iter().map(Reply::json).collect()).await;
    let paypal = PaypalClient::new(&crate::config::PaypalConfig {
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
        currency: "THB".into(),
    };
    let shop = Fixture::new(
        [
            metadata_draft(true, "paypal"),
            metadata_order(false, false, "paypal"),
            payment_recorded(),
            metadata_order(true, false, "paypal"),
            attributes_updated(),
            metadata_order(true, true, "paypal"),
        ]
        .into_iter()
        .map(Reply::json)
        .collect(),
    )
    .await;
    let mut progress = Progress::default();
    let result = complete_external_paid_draft(
        &client(&shop),
        &id("DraftOrder", 1),
        PaymentContext::Paypal {
            client: &paypal,
            expected: &expected,
        },
        CompletionMode::CompleteIfOpen,
        ExternalPaymentRecording {
            money_snapshots: None,
            state: crate::models::checkout::PaymentRecordingState::NotStarted,
            required_tag_hashes: &[],
            convention_slug: "qa-convention",
            identity: Some(metadata_identity("paypal")),
            provider_resource_id: Some("ORDER"),
        },
        &mut progress,
    )
    .await;
    assert!(matches!(
        result.unwrap(),
        CompletionOutcome::Completed { .. }
    ));
    assert_eq!(progress.0, [CompletionStage::RecordExternalPayment]);
    assert_eq!(
        shop.requests()[2].2["variables"],
        json!({"id":"gid://shopify/Order/2"})
    );
    assert!(
        operations(&shop)
            .iter()
            .all(|operation| operation != "EditOrderTags")
    );
}

#[tokio::test]
async fn metadata_provider_or_resource_mismatch_stops_before_reading_or_mutating_shopify() {
    for (identity_provider, resource) in [("paypal", "cs_real"), ("stripe", "cs_other")] {
        let provider = Fixture::new(paid().into_iter().map(Reply::json).collect()).await;
        let shop = Fixture::new(vec![]).await;
        let mut progress = Progress::default();
        let result = complete_external_paid_draft(
            &client(&shop),
            &id("DraftOrder", 1),
            PaymentContext::Stripe {
                client: &stripe_client(&provider),
                expected: &expected(),
            },
            CompletionMode::CompleteIfOpen,
            ExternalPaymentRecording {
                money_snapshots: None,
                state: crate::models::checkout::PaymentRecordingState::NotStarted,
                required_tag_hashes: &[],
                convention_slug: "qa-convention",
                identity: Some(metadata_identity(identity_provider)),
                provider_resource_id: Some(resource),
            },
            &mut progress,
        )
        .await;
        assert!(result.is_err());
        assert_eq!(progress.0, Vec::new());
        assert_eq!(shop.requests(), Vec::new());
    }
}

#[tokio::test]
async fn metadata_attribute_mutation_wrong_identity_is_rejected_even_with_matching_readback() {
    let mut response = attributes_updated();
    response["data"]["orderUpdate"]["order"]["id"] = json!("gid://shopify/Order/999");
    let shop = Fixture::new(
        [
            metadata_draft(true, "stripe"),
            metadata_order(true, false, "stripe"),
            response,
            metadata_order(true, true, "stripe"),
        ]
        .into_iter()
        .map(Reply::json)
        .collect(),
    )
    .await;
    let (result, progress) = run_metadata(
        &shop,
        crate::models::checkout::PaymentRecordingState::Uncertain,
        CompletionMode::ReconcileOnly,
    )
    .await;
    assert!(result.is_err());
    assert_eq!(progress.0, Vec::new());
    assert_eq!(
        operations(&shop),
        [
            "CheckoutDraft",
            "CheckoutOrder",
            "UpdateOrderCustomAttributes",
            "CheckoutOrder"
        ]
    );
}
#[tokio::test]
async fn metadata_unpaid_uncertain_recording_never_repeats_a_financial_mutation() {
    let shop = Fixture::new(
        [
            metadata_draft(true, "stripe"),
            metadata_order(false, false, "stripe"),
        ]
        .into_iter()
        .map(Reply::json)
        .collect(),
    )
    .await;
    let (result, progress) = run_metadata(
        &shop,
        crate::models::checkout::PaymentRecordingState::Uncertain,
        CompletionMode::ReconcileOnly,
    )
    .await;
    assert!(result.is_err());
    assert_eq!(progress.0, Vec::new());
    assert_eq!(operations(&shop), ["CheckoutDraft", "CheckoutOrder"]);
}

#[tokio::test]
async fn metadata_transaction_cap_cannot_authorize_recording_or_completion() {
    for paid in [false, true] {
        let mut value = metadata_order(paid, true, "stripe");
        let mut transaction = external_order(true)["data"]["order"]["transactions"][0].clone();
        transaction["gateway"] = json!("manual");
        transaction["status"] = json!("PENDING");
        let transactions = value["data"]["order"]["transactions"]
            .as_array_mut()
            .unwrap();
        while transactions.len() < 100 {
            transactions.push(transaction.clone());
        }
        let order = serde_json::from_value(value["data"]["order"].clone()).unwrap();
        let recording = ExternalPaymentRecording {
            money_snapshots: None,
            state: crate::models::checkout::PaymentRecordingState::NotStarted,
            required_tag_hashes: &[],
            convention_slug: "qa-convention",
            identity: Some(metadata_identity("stripe")),
            provider_resource_id: Some("cs_real"),
        };
        assert!(
            validate_external_paid_order(
                &order,
                &exact_money(1250, "THB").unwrap(),
                &recording,
                "Stripe Card Payments"
            )
            .is_err()
        );
        let shop = Fixture::new(
            [metadata_draft(true, "stripe"), value]
                .into_iter()
                .map(Reply::json)
                .collect(),
        )
        .await;
        let (result, progress) = run_metadata(
            &shop,
            crate::models::checkout::PaymentRecordingState::NotStarted,
            CompletionMode::CompleteIfOpen,
        )
        .await;
        assert!(result.is_err());
        assert_eq!(progress.0, Vec::new());
        assert_eq!(operations(&shop), ["CheckoutDraft", "CheckoutOrder"]);
    }
}

#[test]
fn metadata_malformed_reference_is_a_terminal_evidence_conflict() {
    let mut value = metadata_order(true, true, "stripe")["data"]["order"].clone();
    let attributes = value["customAttributes"].as_array_mut().unwrap();
    attributes
        .iter_mut()
        .find(|attribute| attribute["key"] == correlation::STRIPE_CHARGE_KEY)
        .unwrap()["value"] = json!("ch invalid");
    let recording = ExternalPaymentRecording {
        money_snapshots: None,
        state: crate::models::checkout::PaymentRecordingState::Recorded,
        required_tag_hashes: &[],
        convention_slug: "qa-convention",
        identity: Some(metadata_identity("stripe")),
        provider_resource_id: Some("cs_real"),
    };
    let error = validate_external_paid_order(
        &serde_json::from_value(value).unwrap(),
        &exact_money(1250, "THB").unwrap(),
        &recording,
        "Stripe Card Payments",
    )
    .unwrap_err();
    assert!(
        error
            .find_source::<crate::models::checkout_completion::CompletionEvidenceConflict>()
            .is_some()
    );
}

#[test]
fn independent_paid_confirmation_requires_both_money_snapshots_and_exact_shopify_sale() {
    let snapshots = correlation::MoneySnapshots {
        payment: domain::dto::checkout::CheckoutMoney {
            minor: 1250,
            currency: "THB".into(),
        },
        shopify: domain::dto::checkout::CheckoutMoney {
            minor: 700,
            currency: "AUD".into(),
        },
    };
    let recording = ExternalPaymentRecording {
        state: crate::models::checkout::PaymentRecordingState::Recorded,
        required_tag_hashes: &[],
        convention_slug: "qa-convention",
        identity: Some(metadata_identity("stripe")),
        provider_resource_id: Some("cs_real"),
        money_snapshots: Some(snapshots),
    };
    let mut value = metadata_order(true, true, "stripe")["data"]["order"].clone();
    value["totalPriceSet"]["presentmentMoney"] = json!({"amount":"7.00", "currencyCode":"AUD"});
    value["totalOutstandingSet"]["presentmentMoney"] =
        json!({"amount":"0.00", "currencyCode":"AUD"});
    value["transactions"][0]["amountSet"]["presentmentMoney"] =
        json!({"amount":"7.00", "currencyCode":"AUD"});
    let attrs = value["customAttributes"].as_array_mut().unwrap();
    attrs
        .iter_mut()
        .find(|attr| attr["key"] == correlation::VERSION_KEY)
        .unwrap()["value"] = json!("3");
    for (key, value) in [
        (correlation::PAYMENT_AMOUNT_KEY, "1250"),
        (correlation::PAYMENT_CURRENCY_KEY, "THB"),
        (correlation::SHOPIFY_AMOUNT_KEY, "700"),
        (correlation::SHOPIFY_CURRENCY_KEY, "AUD"),
    ] {
        attrs.push(json!({"key":key, "value":value}));
    }
    let money = exact_money(700, "AUD").unwrap();
    assert!(
        validate_external_paid_order(
            &serde_json::from_value(value.clone()).unwrap(),
            &money,
            &recording,
            "Stripe Card Payments"
        )
        .is_ok()
    );
    for key in [
        correlation::PAYMENT_AMOUNT_KEY,
        correlation::PAYMENT_CURRENCY_KEY,
        correlation::SHOPIFY_AMOUNT_KEY,
        correlation::SHOPIFY_CURRENCY_KEY,
    ] {
        let mut changed = value.clone();
        changed["customAttributes"]
            .as_array_mut()
            .unwrap()
            .retain(|attr| attr["key"] != key);
        assert!(
            validate_external_paid_order(
                &serde_json::from_value(changed).unwrap(),
                &money,
                &recording,
                "Stripe Card Payments"
            )
            .is_err()
        );
    }
}

fn independent_shopify_payload(mut value: Value, final_metadata: bool) -> Value {
    let data = value["data"].as_object_mut().unwrap();
    let object = data.values_mut().next().unwrap();
    object["totalPriceSet"]["presentmentMoney"] = json!({"amount":"7.00","currencyCode":"AUD"});
    if object.get("totalOutstandingSet").is_some() {
        let outstanding = if object["displayFinancialStatus"] == "PAID" {
            "0.00"
        } else {
            "7.00"
        };
        object["totalOutstandingSet"]["presentmentMoney"] =
            json!({"amount":outstanding,"currencyCode":"AUD"});
        for transaction in object["transactions"].as_array_mut().unwrap() {
            transaction["amountSet"]["presentmentMoney"] =
                json!({"amount":"7.00","currencyCode":"AUD"});
        }
    }
    let attributes = object["customAttributes"].as_array_mut().unwrap();
    attributes
        .iter_mut()
        .find(|attribute| attribute["key"] == correlation::VERSION_KEY)
        .unwrap()["value"] = json!("3");
    attributes.extend([
        json!({"key":correlation::PAYMENT_AMOUNT_KEY,"value":"1250"}),
        json!({"key":correlation::PAYMENT_CURRENCY_KEY,"value":"THB"}),
    ]);
    if final_metadata {
        attributes.extend([
            json!({"key":correlation::SHOPIFY_AMOUNT_KEY,"value":"700"}),
            json!({"key":correlation::SHOPIFY_CURRENCY_KEY,"value":"AUD"}),
        ]);
    }
    value
}

#[tokio::test]
async fn independent_verified_thb_charge_completes_aud_order_and_records_both_ledgers() {
    let shop = Fixture::new(
        [
            independent_shopify_payload(metadata_draft(false, "stripe"), false),
            completed(),
            independent_shopify_payload(metadata_order(false, false, "stripe"), false),
            payment_recorded(),
            independent_shopify_payload(metadata_order(true, false, "stripe"), false),
            attributes_updated(),
            independent_shopify_payload(metadata_order(true, true, "stripe"), true),
        ]
        .into_iter()
        .map(Reply::json)
        .collect(),
    )
    .await;
    let provider = Fixture::new(paid().into_iter().map(Reply::json).collect()).await;
    let snapshots = correlation::MoneySnapshots {
        payment: domain::dto::checkout::CheckoutMoney {
            minor: 1250,
            currency: "THB".into(),
        },
        shopify: domain::dto::checkout::CheckoutMoney {
            minor: 700,
            currency: "AUD".into(),
        },
    };
    let mut progress = Progress::default();
    let result = complete_external_paid_draft(
        &client(&shop),
        &id("DraftOrder", 1),
        PaymentContext::Stripe {
            client: &stripe_client(&provider),
            expected: &expected(),
        },
        CompletionMode::CompleteIfOpen,
        ExternalPaymentRecording {
            state: crate::models::checkout::PaymentRecordingState::NotStarted,
            required_tag_hashes: &[],
            convention_slug: "qa-convention",
            identity: Some(metadata_identity("stripe")),
            provider_resource_id: Some("cs_real"),
            money_snapshots: Some(snapshots),
        },
        &mut progress,
    )
    .await;
    assert!(matches!(
        result.unwrap(),
        CompletionOutcome::Completed { .. }
    ));
    assert_eq!(
        progress.0,
        [
            CompletionStage::ConvertDraft,
            CompletionStage::RecordExternalPayment
        ]
    );
    assert_eq!(
        operations(&shop)
            .iter()
            .filter(|operation| **operation == "RecordCheckoutPayment")
            .count(),
        1
    );
    let requests = shop.requests();
    let attributes = &requests[5].2["variables"]["input"]["customAttributes"];
    for (key, value) in [
        (correlation::PAYMENT_AMOUNT_KEY, "1250"),
        (correlation::PAYMENT_CURRENCY_KEY, "THB"),
        (correlation::SHOPIFY_AMOUNT_KEY, "700"),
        (correlation::SHOPIFY_CURRENCY_KEY, "AUD"),
    ] {
        assert_eq!(
            attributes
                .as_array()
                .unwrap()
                .iter()
                .filter(|attribute| attribute["key"] == key && attribute["value"] == value)
                .count(),
            1
        );
    }
}
