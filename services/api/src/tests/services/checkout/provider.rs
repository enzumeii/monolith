#![allow(clippy::unwrap_used)]
use super::*;
use crate::{
    client::tests::{Fixture, Reply},
    config::StripeConfig,
    services::{
        checkout::tests::{checkout, draft, money},
        common::client,
    },
};
use serde_json::json;

#[test]
fn uncertain_completion_always_reconciles_and_missing_name_is_retryable() {
    let mut row = checkout();
    for state in [CompletionState::Completing, CompletionState::Uncertain] {
        row.completion_state = state;
        assert!(completion_mode(&row, true) == CompletionMode::ReconcileOnly);
    }
    row.completion_state = CompletionState::NotStarted;
    assert!(completion_mode(&row, false) == CompletionMode::ReconcileOnly);
    assert!(completion_mode(&row, true) == CompletionMode::CompleteIfOpen);
    let value = json!({"id":"gid://shopify/Order/2", "name":null,
        "displayFinancialStatus":"PAID", "cancelledAt":null,
        "tags":[format!("checkout:{}", row.id)],
        "totalPriceSet":{"presentmentMoney":{"amount":"40.00", "currencyCode":"SGD"}}});
    for name in [
        serde_json::Value::Null,
        json!(""),
        json!("   "),
        json!("#TEST-2"),
    ] {
        let mut value = value.clone();
        value["name"] = name.clone();
        let order = serde_json::from_value(value).unwrap();
        assert_eq!(
            order_confirmation(&row, &order),
            if name == json!("#TEST-2") {
                None
            } else {
                Some(RecoveryOutcome::Retry(RecoveryCode::CompletionUncertain))
            }
        );
    }
    let mut value = value;
    value["name"] = json!("#TEST-2");
    value["tags"] = json!(["checkout:foreign"]);
    assert_eq!(
        order_confirmation(&row, &serde_json::from_value(value).unwrap()),
        Some(RecoveryOutcome::Manual(RecoveryCode::EvidenceConflict))
    );
}

#[tokio::test]
async fn failed_completion_checkpoint_returns_original_persistence_error_without_mutation() {
    let mut row = checkout();
    row.payment_state = PaymentState::Paid;
    let replies = vec![
        json!({"id":"acct_test","object":"account"}),
        json!({"id":"cs_test","object":"checkout.session","mode":"payment","status":"complete",
            "payment_status":"paid","livemode":false,"amount_total":4000,"currency":"sgd",
            "metadata":{"checkout_id":row.id.to_string()},"payment_intent":"pi_test","url":null}),
        json!({"id":"pi_test","object":"payment_intent","status":"succeeded","amount":4000,
            "amount_received":4000,"currency":"sgd","livemode":false,
            "metadata":{"checkout_id":row.id.to_string()},"latest_charge":"ch_test"}),
        json!({"id":"ch_test","object":"charge","amount":4000,"livemode":false,"status":"succeeded",
            "amount_captured":4000,"amount_refunded":0,"captured":true,"paid":true,"refunded":false,
            "created":1,"currency":"sgd","disputed":false,"metadata":{},"payment_intent":"pi_test",
            "balance_transaction":null}),
    ];
    let gateway = Fixture::new(replies.into_iter().map(Reply::json).collect()).await;
    let stripe = crate::client::stripe::StripeClient::new(&StripeConfig {
        checkout_api_version: "2026-08-26.dahlia".into(),
        base_url: gateway.url.join("/v1/").unwrap().to_string(),
        timeout_secs: 1,
        secret_key: Some("fixture".into()),
    })
    .unwrap();
    let current = json!({"data":{"draftOrder":draft(&row)}});
    let shop = Fixture::new(vec![
        Reply::json(current.clone()),
        Reply::json(current.clone()),
        Reply::json(current),
    ])
    .await;
    let shopify = client(&shop);
    let clients = CheckoutClients {
        shopify: &shopify,
        stripe: Some(&stripe),
        paypal: None,
    };
    let pool = sqlx::postgres::PgPoolOptions::new()
        .connect_lazy("postgres://fixture:fixture@127.0.0.1:1/fixture")
        .unwrap();
    pool.close().await;
    let expected = stripe_expected(&row, &money()).unwrap();
    let result = complete(
        &pool,
        &clients,
        &mut row,
        Uuid::new_v4(),
        true,
        false,
        PaymentContext::Stripe {
            client: &stripe,
            expected: &expected,
        },
    )
    .await;
    let Err(AdvanceError::Persistence(error)) = result else {
        panic!("checkpoint error was swallowed");
    };
    assert!(matches!(
        error.find_source::<sqlx::Error>(),
        Some(sqlx::Error::PoolClosed)
    ));
    assert_eq!(row.completion_state, CompletionState::Completing);
    assert_eq!(shop.requests().len(), 3);
    assert!(
        shop.requests()
            .iter()
            .all(|request| request.2["operationName"] == "CheckoutDraft")
    );
    assert_eq!(gateway.requests().len(), 4);
}

#[tokio::test]
async fn v2_checkpoint_rechecks_current_draft_eligibility_before_conversion() {
    use crate::services::checkout_completion::{CompletionProgress, CompletionStage};
    let mut row = checkout();
    row.completion_version = 2;
    row.convention_slug = Some("qa-convention".into());
    row.evidence_hash = Sha256::digest(
        serde_json::to_vec(&(
            "private@example.test",
            money(),
            BTreeMap::from([("gid://shopify/ProductVariant/2", 2)]),
            "qa-convention",
        ))
        .unwrap(),
    )
    .to_vec();
    let mut current = draft(&row);
    current["customAttributes"] = json!([{"key":"convention_slug","value":"qa-convention"}]);
    current["ready"] = json!(false);
    let shop = Fixture::new(vec![Reply::json(json!({"data":{"draftOrder":current}}))]).await;
    let shopify = client(&shop);
    let clients = CheckoutClients {
        shopify: &shopify,
        stripe: None,
        paypal: None,
    };
    let pool = sqlx::postgres::PgPoolOptions::new()
        .connect_lazy("postgres://fixture:fixture@127.0.0.1:1/fixture")
        .unwrap();
    pool.close().await;
    let mut progress = CheckoutCompletionProgress {
        pool: &pool,
        clients: &clients,
        checkout: &mut row,
        token: Uuid::new_v4(),
        attempted: false,
        checkpoint_error: None,
    };
    assert!(
        progress
            .before_mutation(CompletionStage::ConvertDraft)
            .await
            .is_err()
    );
    assert!(!progress.attempted);
    assert!(progress.checkpoint_error.is_none());
    assert_eq!(row.completion_state, CompletionState::NotStarted);
    assert_eq!(shop.requests().len(), 1);
}

#[tokio::test]
async fn unsupported_version_combination_stops_before_provider_effects_or_checkpoint_changes() {
    let mut row = checkout();
    row.completion_version = 1;
    row.correlation_version = super::super::correlation::METADATA_VERSION;
    row.provider_resource_id = None;
    row.payment_state = PaymentState::NotStarted;
    let shop = Fixture::new(vec![]).await;
    let shopify = client(&shop);
    let clients = CheckoutClients {
        shopify: &shopify,
        stripe: None,
        paypal: None,
    };
    let pool = sqlx::postgres::PgPoolOptions::new()
        .connect_lazy("postgres://fixture:fixture@127.0.0.1:1/fixture")
        .unwrap();
    pool.close().await;
    let result = advance(
        &pool,
        &clients,
        &mut row,
        &money(),
        Uuid::new_v4(),
        true,
        false,
    )
    .await;
    let Err(AdvanceError::Business(error)) = result else {
        panic!("Unsupported versions advanced checkout");
    };
    assert!(
        error
            .find_source::<crate::models::checkout_completion::CompletionEvidenceConflict>()
            .is_some()
    );
    assert_eq!(row.payment_state, PaymentState::NotStarted);
    assert!(row.provider_resource_id.is_none());
    assert_eq!(shop.requests(), Vec::new());
}

#[test]
fn only_fresh_definitive_mutation_rejections_are_terminal() {
    for mode in [InitiationMode::Fresh, InitiationMode::Recover] {
        for status in [400, 401, 403, 422, 408, 409, 429, 500, 503] {
            let error = crate::client::ClientError::new(crate::client::ClientErrorKind::Http {
                status,
                request_id: None,
            });
            let result = initiation_rejection(error, mode);
            assert_eq!(
                matches!(result, Ok(InitiationOutcome::Rejected(_))),
                mode == InitiationMode::Fresh && matches!(status, 400 | 401 | 403 | 422)
            );
        }
        let error = crate::client::ClientError::new(crate::client::ClientErrorKind::Protocol(
            "Synthetic incomplete response",
        ));
        assert!(initiation_rejection(error, mode).is_err());
    }
}
