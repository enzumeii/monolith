#![allow(clippy::unwrap_used)]
use super::*;
use crate::{
    client::{
        ClientErrorKind,
        tests::{Fixture, Reply},
    },
    config::StripeConfig,
    error::upstream,
};
use axum::response::IntoResponse;
use serde_json::{Value, json};

fn expected() -> ExpectedPayment {
    ExpectedPayment {
        session_id: "cs_test_real".into(),
        account_id: "acct_real".into(),
        livemode: false,
        amount: 1250,
        currency: "THB".into(),
        metadata: BTreeMap::from([("payment_id".into(), "local_43".into())]),
    }
}
fn objects() -> Vec<Value> {
    vec![
        json!({"id":"acct_real","object":"account"}),
        json!({"id":"cs_test_real","object":"checkout.session","mode":"payment","status":"complete","payment_status":"paid","livemode":false,"amount_total":1250,"currency":"thb","metadata":{"payment_id":"local_43"},"payment_intent":"pi_real","url":null}),
        json!({"id":"pi_real","object":"payment_intent","status":"succeeded","amount":1250,"amount_received":1250,"currency":"thb","livemode":false,"metadata":{"payment_id":"local_43"},"latest_charge":"ch_real"}),
        json!({"id":"ch_real","object":"charge","amount":1250,"livemode":false,"status":"succeeded","amount_captured":1250,"amount_refunded":0,"captured":true,"paid":true,"refunded":false,"created":1,"currency":"thb","disputed":false,"metadata":{},"payment_intent":"pi_real","balance_transaction":null}),
    ]
}
fn client(fixture: &Fixture) -> StripeClient {
    StripeClient::new(&StripeConfig {
        base_url: fixture.url.join("/v1/").unwrap().to_string(),
        timeout_secs: 1,
        secret_key: Some("sk_test_fixture".into()),
    })
    .unwrap()
}

#[tokio::test]
async fn verifies_paid_from_authoritative_objects_and_returns_real_references() {
    let fixture = Fixture::new(objects().into_iter().map(Reply::json).collect()).await;
    let outcome = verify_payment(&client(&fixture), &expected())
        .await
        .unwrap();
    assert_eq!(
        outcome,
        PaymentOutcome::VerifiedPaid(VerifiedPayment {
            account_id: "acct_real".into(),
            session_id: "cs_test_real".into(),
            payment_intent_id: "pi_real".into(),
            charge_id: "ch_real".into(),
            amount: 1250,
            currency: "thb".into(),
            livemode: false,
            refunded: false,
            amount_refunded: 0,
            disputed: false
        })
    );
    assert_eq!(
        fixture
            .requests()
            .iter()
            .map(|r| r.0.as_str())
            .collect::<Vec<_>>(),
        [
            "/v1/account",
            "/v1/checkout/sessions/cs_test_real",
            "/v1/payment_intents/pi_real",
            "/v1/charges/ch_real"
        ]
    );
}

#[tokio::test]
async fn rejects_context_and_paid_object_mismatches_before_following_the_next_reference() {
    let cases = [
        (0, "id", json!("acct_other")),
        (0, "object", json!("customer")),
        (1, "id", json!("cs_other")),
        (1, "object", json!("customer")),
        (1, "mode", json!("subscription")),
        (1, "livemode", json!(true)),
        (1, "amount_total", json!(1251)),
        (1, "amount_total", Value::Null),
        (1, "currency", json!("usd")),
        (1, "currency", Value::Null),
        (1, "metadata", json!({"payment_id":"other"})),
        (1, "status", json!("future_status")),
        (1, "payment_status", json!("future_status")),
        (1, "payment_status", json!("no_payment_required")),
        (1, "payment_intent", Value::Null),
        (1, "payment_intent", json!("")),
        (1, "payment_intent", json!("cs_wrong_kind")),
        (2, "id", json!("pi_other")),
        (2, "object", json!("charge")),
        (2, "status", json!("processing")),
        (2, "status", json!("future_status")),
        (2, "amount", json!(1251)),
        (2, "amount_received", json!(1249)),
        (2, "currency", json!("usd")),
        (2, "livemode", json!(true)),
        (2, "metadata", json!({})),
        (2, "latest_charge", Value::Null),
        (2, "latest_charge", json!("")),
        (2, "latest_charge", json!("pi_wrong_kind")),
        (3, "id", json!("ch_other")),
        (3, "object", json!("payment_intent")),
        (3, "object", Value::Null),
        (3, "amount", json!(1251)),
        (3, "amount", Value::Null),
        (3, "amount_captured", json!(1249)),
        (3, "currency", json!("usd")),
        (3, "livemode", json!(true)),
        (3, "livemode", Value::Null),
        (3, "status", json!("failed")),
        (3, "status", json!("pending")),
        (3, "status", Value::Null),
        (3, "status", json!("future_status")),
        (3, "paid", json!(false)),
        (3, "captured", json!(false)),
        (3, "payment_intent", json!("pi_other")),
        (3, "payment_intent", Value::Null),
    ];
    for (index, field, value) in cases {
        let mut replies = objects();
        replies[index][field] = value.clone();
        let fixture = Fixture::new(replies.into_iter().map(Reply::json).collect()).await;
        let error = verify_payment(&client(&fixture), &expected())
            .await
            .unwrap_err();
        assert!(
            matches!(
                error.kind(),
                ClientErrorKind::Protocol("Payment verification failed")
            ),
            "object {index} field {field}: {value}"
        );
        assert!(
            error
                .location()
                .file()
                .ends_with("client/stripe/payment.rs"),
            "object {index} field {field}: origin {}",
            error.location()
        );
        assert_eq!(
            fixture.requests().len(),
            index + 1,
            "object {index} field {field}: {value}"
        );
    }
}

#[tokio::test]
async fn pending_and_expired_unpaid_do_not_require_intent_or_charge() {
    for (status, expired) in [("open", false), ("complete", false), ("expired", true)] {
        let mut replies = objects();
        replies[1]["status"] = json!(status);
        replies[1]["payment_status"] = json!("unpaid");
        replies[1]["payment_intent"] = Value::Null;
        let fixture = Fixture::new(replies.into_iter().map(Reply::json).collect()).await;
        let result = verify_payment(&client(&fixture), &expected())
            .await
            .unwrap();
        let want = if expired {
            PaymentOutcome::ExpiredUnpaid {
                session_id: "cs_test_real".into(),
            }
        } else {
            PaymentOutcome::Pending {
                session_id: "cs_test_real".into(),
            }
        };
        assert_eq!(result, want);
        assert_eq!(fixture.requests().len(), 2);
    }
}

#[tokio::test]
async fn contradictory_paid_session_states_fail_closed() {
    for status in ["open", "expired"] {
        let mut replies = objects();
        replies[1]["status"] = json!(status);
        let fixture = Fixture::new(replies.into_iter().map(Reply::json).collect()).await;
        assert!(
            verify_payment(&client(&fixture), &expected())
                .await
                .is_err()
        );
        assert_eq!(fixture.requests().len(), 2);
    }
}

#[tokio::test]
async fn expanded_references_are_retrieved_again_and_do_not_supply_paid_evidence() {
    let mut replies = objects();
    let mut expanded_charge = replies[3].clone();
    expanded_charge["amount"] = json!(999);
    let mut expanded_intent = replies[2].clone();
    expanded_intent["amount_received"] = json!(999);
    expanded_intent["latest_charge"] = expanded_charge.clone();
    replies[1]["payment_intent"] = expanded_intent;
    replies[2]["latest_charge"] = expanded_charge;
    let fixture = Fixture::new(replies.into_iter().map(Reply::json).collect()).await;
    assert!(matches!(
        verify_payment(&client(&fixture), &expected())
            .await
            .unwrap(),
        PaymentOutcome::VerifiedPaid(_)
    ));
    assert_eq!(fixture.requests().len(), 4);
}

#[tokio::test]
async fn every_upstream_failure_and_malformed_object_remains_an_error() {
    for index in 0..4 {
        for reply in [
            Reply::status(500),
            Reply {
                status: 200,
                body: "malformed".into(),
                headers: vec![],
            },
            Reply::json(json!({})),
            Reply::json(Value::Null),
        ] {
            let mut replies = objects().into_iter().map(Reply::json).collect::<Vec<_>>();
            replies[index] = reply;
            let fixture = Fixture::new(replies).await;
            let error = verify_payment(&client(&fixture), &expected())
                .await
                .unwrap_err();
            assert_eq!(fixture.requests().len(), index + 1);
            let origin = error.location();
            assert!(origin.file().ends_with("client.rs"));
            match error.kind() {
                ClientErrorKind::Http { status, .. } => assert_eq!(*status, 500),
                ClientErrorKind::Decode { source } => {
                    assert!(std::ptr::eq(
                        std::error::Error::source(&error)
                            .unwrap()
                            .downcast_ref()
                            .unwrap(),
                        source
                    ));
                }
                _ => panic!("provider failure lost its typed cause"),
            }
            let mapped = upstream(error);
            let retained = std::error::Error::source(&mapped)
                .unwrap()
                .downcast_ref::<crate::client::ClientError>()
                .unwrap();
            assert_eq!(retained.location(), origin);
            let response = mapped.into_response();
            assert_eq!(response.status(), axum::http::StatusCode::BAD_GATEWAY);
            let body = axum::body::to_bytes(response.into_body(), 1024)
                .await
                .unwrap();
            assert_eq!(
                serde_json::from_slice::<Value>(&body).unwrap(),
                json!({"error":"Upstream request failed"})
            );
        }
    }
}

#[tokio::test]
async fn refund_does_not_erase_original_payment_success() {
    let mut replies = objects();
    replies[3]["refunded"] = json!(true);
    replies[3]["amount_refunded"] = json!(1250);
    let fixture = Fixture::new(replies.into_iter().map(Reply::json).collect()).await;
    let PaymentOutcome::VerifiedPaid(payment) = verify_payment(&client(&fixture), &expected())
        .await
        .unwrap()
    else {
        panic!("original paid fact must remain verified")
    };
    assert!(payment.refunded);
    assert_eq!(payment.amount_refunded, 1250);
    assert_eq!(fixture.requests().len(), 4);
}

#[tokio::test]
async fn invalid_trusted_expectations_fail_before_provider_calls() {
    for field in [
        "session_id",
        "session_prefix_only",
        "session_too_long",
        "session_invalid_character",
        "account_id",
        "account_prefix_only",
        "amount",
        "currency",
        "metadata",
    ] {
        let mut expectation = expected();
        match field {
            "session_id" => expectation.session_id = "pi_other".into(),
            "session_prefix_only" => expectation.session_id = "cs_".into(),
            "session_too_long" => expectation.session_id = format!("cs_{}", "a".repeat(253)),
            "session_invalid_character" => expectation.session_id = "cs_test/invalid".into(),
            "account_id" => expectation.account_id = String::new(),
            "account_prefix_only" => expectation.account_id = "acct_".into(),
            "amount" => expectation.amount = 0,
            "currency" => expectation.currency = "???".into(),
            _ => expectation.metadata.clear(),
        }
        let fixture = Fixture::new(vec![]).await;
        let error = verify_payment(&client(&fixture), &expectation)
            .await
            .unwrap_err();
        assert!(matches!(
            error.kind(),
            ClientErrorKind::InvalidInput("Invalid expected payment context")
        ));
        assert_eq!(fixture.requests().len(), 0);
    }
}

#[tokio::test]
async fn provider_timeout_keeps_transport_source_and_origin_for_safe_http_mapping() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let client = StripeClient::new(&StripeConfig {
        base_url: format!("http://{}/v1/", listener.local_addr().unwrap()),
        timeout_secs: 1,
        secret_key: Some("sk_test_fixture".into()),
    })
    .unwrap();
    let expectation = expected();
    let (error, connection) =
        tokio::join!(verify_payment(&client, &expectation), listener.accept());
    let _connection = connection.unwrap();
    let error = error.unwrap_err();
    let ClientErrorKind::Transport { source } = error.kind() else {
        panic!("timeout must retain the original transport failure");
    };
    assert!(source.is_timeout());
    assert!(std::ptr::eq(
        std::error::Error::source(&error)
            .unwrap()
            .downcast_ref()
            .unwrap(),
        source
    ));
    let origin = error.location();
    assert!(origin.file().ends_with("client.rs"));
    let mapped = upstream(error);
    let retained = std::error::Error::source(&mapped)
        .unwrap()
        .downcast_ref::<crate::client::ClientError>()
        .unwrap();
    assert_eq!(retained.location(), origin);
    let response = mapped.into_response();
    assert_eq!(response.status(), axum::http::StatusCode::GATEWAY_TIMEOUT);
    let body = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .unwrap();
    assert_eq!(
        serde_json::from_slice::<Value>(&body).unwrap(),
        json!({"error":"Upstream request failed"})
    );
}
