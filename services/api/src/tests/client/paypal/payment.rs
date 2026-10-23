#![allow(clippy::unwrap_used)]
use super::*;
use crate::{
    client::tests::{Fixture, Reply},
    config::PaypalConfig,
};
use axum::http::Method;
use serde_json::{Value, json};

fn expected(fixture: &Fixture) -> ExpectedPayment {
    ExpectedPayment {
        order_id: "ORDER".into(),
        merchant_id: "MERCHANT".into(),
        api_origin: fixture.url.origin().ascii_serialization(),
        reference_id: "purchase-unit".into(),
        custom_id: "draft-ref".into(),
        amount: 1000,
        currency: "USD".into(),
    }
}
fn client(fixture: &Fixture) -> PaypalClient {
    PaypalClient::new(&PaypalConfig {
        base_url: fixture.url.join("/").unwrap().to_string(),
        timeout_secs: 1,
        client_id: Some("fixture-client".into()),
        client_secret: Some("fixture-secret".into()),
    })
    .unwrap()
}
fn objects() -> Vec<Value> {
    let capture = json!({"id":"CAPTURE","status":"COMPLETED","amount":{"currency_code":"USD","value":"10.00"},"final_capture":true,"payee":{"merchant_id":"MERCHANT"},"custom_id":"draft-ref","supplementary_data":{"related_ids":{"order_id":"ORDER","capture_id":"CAPTURE"}}});
    vec![
        json!({"access_token":"fixture-token","token_type":"Bearer","expires_in":3600}),
        json!({"id":"ORDER","status":"COMPLETED","purchase_units":[{"reference_id":"purchase-unit","custom_id":"draft-ref","amount":{"currency_code":"USD","value":"10.00"},"payee":{"merchant_id":"MERCHANT"},"payments":{"captures":[capture.clone()]}}]}),
        capture,
    ]
}
async fn provider_fixture(objects: Vec<Value>) -> Fixture {
    Fixture::new(objects.into_iter().map(Reply::json).collect()).await
}

#[tokio::test]
async fn verifies_authoritative_payment_with_gets_and_never_captures() {
    let fixture = provider_fixture(objects()).await;
    let outcome = verify_payment(&client(&fixture), &expected(&fixture))
        .await
        .unwrap();
    assert_eq!(
        outcome,
        PaymentOutcome::VerifiedPaid(VerifiedPayment {
            order_id: "ORDER".into(),
            capture_id: "CAPTURE".into(),
            amount: 1000,
            currency: "USD".into(),
        })
    );
    assert_eq!(fixture.methods(), [Method::POST, Method::GET, Method::GET]);
    assert_eq!(
        fixture
            .requests()
            .iter()
            .map(|request| request.0.as_str())
            .collect::<Vec<_>>(),
        [
            "/v1/oauth2/token",
            "/v2/checkout/orders/ORDER",
            "/v2/payments/captures/CAPTURE"
        ]
    );
    assert!(
        fixture.requests()[1..]
            .iter()
            .all(|request| request.2 == Value::Null)
    );
}

#[tokio::test]
async fn approved_and_other_uncaptured_orders_stay_unpaid_without_provider_mutation() {
    for (status, terminal) in [
        ("CREATED", false),
        ("SAVED", false),
        ("APPROVED", false),
        ("PAYER_ACTION_REQUIRED", false),
        ("VOIDED", true),
    ] {
        let mut replies = objects();
        replies[1]["status"] = json!(status);
        replies[1]["purchase_units"][0]["payments"] = Value::Null;
        let fixture = provider_fixture(replies).await;
        let outcome = verify_payment(&client(&fixture), &expected(&fixture))
            .await
            .unwrap();
        let want = if terminal {
            PaymentOutcome::TerminalUnpaid
        } else {
            PaymentOutcome::Pending
        };
        assert_eq!(outcome, want, "{status}");
        assert_eq!(fixture.methods(), [Method::POST, Method::GET]);
    }
}

#[tokio::test]
async fn independently_retrieved_pending_and_denied_captures_are_unpaid() {
    for (status, terminal) in [
        ("PENDING", false),
        ("DENIED", true),
        ("DECLINED", true),
        ("FAILED", true),
    ] {
        let mut replies = objects();
        replies[1]["purchase_units"][0]["payments"]["captures"][0]["status"] = json!(status);
        replies[2]["status"] = json!(status);
        let fixture = provider_fixture(replies).await;
        let want = if terminal {
            PaymentOutcome::TerminalUnpaid
        } else {
            PaymentOutcome::Pending
        };
        assert_eq!(
            verify_payment(&client(&fixture), &expected(&fixture))
                .await
                .unwrap(),
            want,
            "{status}"
        );
        assert_eq!(fixture.methods(), [Method::POST, Method::GET, Method::GET]);
    }
}

#[tokio::test]
async fn order_and_purchase_unit_mismatches_fail_before_capture_retrieval() {
    for (pointer, value) in [
        ("/id", json!("OTHER")),
        ("/id", json!("")),
        ("/status", json!("FUTURE")),
        ("/purchase_units", json!([])),
        ("/purchase_units/0/reference_id", json!("other")),
        ("/purchase_units/0/reference_id", Value::Null),
        ("/purchase_units/0/custom_id", json!("other")),
        ("/purchase_units/0/custom_id", Value::Null),
        ("/purchase_units/0/payee", Value::Null),
        ("/purchase_units/0/payee/merchant_id", json!("OTHER")),
        ("/purchase_units/0/amount/value", json!("9.99")),
        ("/purchase_units/0/amount/currency_code", json!("EUR")),
        ("/purchase_units/0/payments", Value::Null),
        ("/purchase_units/0/payments/captures/0/id", json!("")),
    ] {
        let mut replies = objects();
        *replies[1].pointer_mut(pointer).unwrap() = value.clone();
        let fixture = provider_fixture(replies).await;
        assert!(
            verify_payment(&client(&fixture), &expected(&fixture))
                .await
                .is_err(),
            "{pointer}: {value}"
        );
        assert_eq!(fixture.requests().len(), 2, "{pointer}: {value}");
    }
    for pointer in ["/purchase_units", "/purchase_units/0/payments/captures"] {
        let mut replies = objects();
        let array = replies[1]
            .pointer_mut(pointer)
            .unwrap()
            .as_array_mut()
            .unwrap();
        array.push(array[0].clone());
        let fixture = provider_fixture(replies).await;
        assert!(
            verify_payment(&client(&fixture), &expected(&fixture))
                .await
                .is_err()
        );
        assert_eq!(fixture.requests().len(), 2);
    }
}

#[tokio::test]
async fn capture_evidence_rejects_mismatched_identity_money_status_and_nonfinal_payment() {
    for index in [1, 2] {
        for (pointer, value) in [
            ("/id", json!("OTHER")),
            ("/amount/value", json!("9.99")),
            ("/amount/value", json!("1e1")),
            ("/amount/currency_code", json!("EUR")),
            ("/final_capture", json!(false)),
            ("/final_capture", Value::Null),
            ("/status", json!("PENDING")),
            ("/status", json!("DENIED")),
            ("/status", json!("REFUNDED")),
            ("/status", json!("PARTIALLY_REFUNDED")),
            ("/status", json!("REVERSED")),
            ("/status", json!("FUTURE")),
            ("/payee/merchant_id", json!("OTHER")),
            ("/custom_id", json!("other")),
            ("/supplementary_data/related_ids/order_id", json!("OTHER")),
            ("/supplementary_data/related_ids/capture_id", json!("OTHER")),
        ] {
            let mut replies = objects();
            let capture = if index == 1 {
                &mut replies[1]["purchase_units"][0]["payments"]["captures"][0]
            } else {
                &mut replies[2]
            };
            *capture.pointer_mut(pointer).unwrap() = value.clone();
            let fixture = provider_fixture(replies).await;
            assert!(
                verify_payment(&client(&fixture), &expected(&fixture))
                    .await
                    .is_err(),
                "object {index} {pointer}: {value}"
            );
        }
    }
    for status in ["REFUNDED", "PARTIALLY_REFUNDED", "REVERSED", "FUTURE"] {
        let mut replies = objects();
        replies[1]["purchase_units"][0]["payments"]["captures"][0]["status"] = json!(status);
        replies[2]["status"] = json!(status);
        let fixture = provider_fixture(replies).await;
        assert!(
            verify_payment(&client(&fixture), &expected(&fixture))
                .await
                .is_err(),
            "{status}"
        );
    }
}

#[tokio::test]
async fn absent_redundant_capture_identity_is_allowed_but_embedded_contradictions_still_fail() {
    let mut replies = objects();
    for index in [1, 2] {
        let capture = if index == 1 {
            &mut replies[1]["purchase_units"][0]["payments"]["captures"][0]
        } else {
            &mut replies[2]
        };
        for field in ["payee", "custom_id", "supplementary_data"] {
            capture.as_object_mut().unwrap().remove(field);
        }
    }
    let fixture = provider_fixture(replies).await;
    assert!(matches!(
        verify_payment(&client(&fixture), &expected(&fixture))
            .await
            .unwrap(),
        PaymentOutcome::VerifiedPaid(_)
    ));
    for (pointer, value) in [
        ("/payee/merchant_id", json!("WRONG")),
        ("/custom_id", json!("wrong")),
        ("/supplementary_data/related_ids/order_id", json!("WRONG")),
        ("/supplementary_data/related_ids/capture_id", json!("WRONG")),
    ] {
        let mut replies = objects();
        for field in ["payee", "custom_id", "supplementary_data"] {
            replies[2].as_object_mut().unwrap().remove(field);
        }
        *replies[1]["purchase_units"][0]["payments"]["captures"][0]
            .pointer_mut(pointer)
            .unwrap() = value;
        let fixture = provider_fixture(replies).await;
        assert!(
            verify_payment(&client(&fixture), &expected(&fixture))
                .await
                .is_err(),
            "{pointer}"
        );
    }
}

#[tokio::test]
async fn completed_capture_never_proves_paid_for_incomplete_or_unknown_order() {
    for status in ["APPROVED", "CREATED", "VOIDED", "FUTURE"] {
        let mut replies = objects();
        replies[1]["status"] = json!(status);
        let fixture = provider_fixture(replies).await;
        assert!(
            verify_payment(&client(&fixture), &expected(&fixture))
                .await
                .is_err(),
            "{status}"
        );
    }
}

#[tokio::test]
async fn invalid_trusted_expectations_never_contact_provider() {
    for field in [
        "order_id",
        "merchant_id",
        "reference_id",
        "custom_id",
        "amount",
        "currency",
        "origin",
        "origin_path",
        "origin_userinfo",
        "origin_query",
        "origin_fragment",
    ] {
        let fixture = Fixture::new(vec![]).await;
        let mut expected = expected(&fixture);
        match field {
            "order_id" => expected.order_id.clear(),
            "merchant_id" => expected.merchant_id.clear(),
            "reference_id" => expected.reference_id.clear(),
            "custom_id" => expected.custom_id = "a\nb".into(),
            "amount" => expected.amount = 0,
            "currency" => expected.currency = "usd".into(),
            "origin" => expected.api_origin = "https://api-m.paypal.com".into(),
            "origin_path" => expected.api_origin.push_str("/v2"),
            "origin_query" => expected.api_origin.push_str("?a=b"),
            "origin_fragment" => expected.api_origin.push_str("#a"),
            _ => expected.api_origin = expected.api_origin.replace("://", "://user@"),
        }
        let error = verify_payment(&client(&fixture), &expected)
            .await
            .unwrap_err();
        assert!(
            matches!(
                error.kind(),
                ClientErrorKind::InvalidInput("Invalid expected payment context")
            ),
            "{field}"
        );
        assert!(
            error
                .location()
                .file()
                .ends_with("client/paypal/payment.rs")
        );
        assert!(fixture.requests().is_empty(), "{field}");
    }
}

#[tokio::test]
async fn upstream_failure_and_malformed_objects_are_safe_errors_at_each_retrieval() {
    for index in 0..3 {
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
            let error = verify_payment(&client(&fixture), &expected(&fixture))
                .await
                .unwrap_err();
            assert_eq!(fixture.requests().len(), index + 1);
            assert!(error.location().file().ends_with("client.rs"));
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
            assert!(!format!("{error:?}").contains("fixture-token"));
        }
    }
}

#[tokio::test]
async fn exact_integer_money_verifies_zero_decimal_and_large_minor_amounts() {
    for (amount, currency, minor) in [
        ("42", "JPY", 42),
        ("7", "TWD", 7),
        ("42", "HUF", 42),
        ("90071992547409.91", "USD", 9_007_199_254_740_991),
        ("92233720368547758.07", "USD", i64::MAX),
    ] {
        let mut replies = objects();
        let money = json!({"value":amount,"currency_code":currency});
        replies[1]["purchase_units"][0]["amount"] = money.clone();
        replies[1]["purchase_units"][0]["payments"]["captures"][0]["amount"] = money.clone();
        replies[2]["amount"] = money;
        let fixture = provider_fixture(replies).await;
        let mut expected = expected(&fixture);
        expected.amount = minor;
        expected.currency = currency.into();
        assert!(matches!(
            verify_payment(&client(&fixture), &expected).await.unwrap(),
            PaymentOutcome::VerifiedPaid(_)
        ));
    }
}

#[tokio::test]
async fn contradictory_provider_evidence_has_a_safe_protocol_error_and_original_location() {
    for (index, pointer) in [(1, "/id"), (2, "/amount/value")] {
        let mut replies = objects();
        *replies[index].pointer_mut(pointer).unwrap() = json!("untrusted-provider-value");
        let fixture = provider_fixture(replies).await;
        let error = verify_payment(&client(&fixture), &expected(&fixture))
            .await
            .unwrap_err();
        assert!(matches!(
            error.kind(),
            ClientErrorKind::Protocol("PayPal payment evidence does not match")
        ));
        assert!(
            error
                .location()
                .file()
                .ends_with("client/paypal/payment/evidence.rs")
        );
        assert!(!format!("{error:?}").contains("untrusted-provider-value"));
        assert!(std::error::Error::source(&error).is_none());
    }
}

#[tokio::test]
async fn provider_timeout_retains_the_transport_cause_and_client_origin() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let paypal = PaypalClient::new(&PaypalConfig {
        base_url: format!("{origin}/"),
        timeout_secs: 1,
        client_id: Some("fixture-client".into()),
        client_secret: Some("fixture-secret".into()),
    })
    .unwrap();
    let expectation = ExpectedPayment {
        order_id: "ORDER".into(),
        merchant_id: "MERCHANT".into(),
        api_origin: origin,
        reference_id: "purchase-unit".into(),
        custom_id: "draft-ref".into(),
        amount: 1000,
        currency: "USD".into(),
    };
    let (result, connection) =
        tokio::join!(verify_payment(&paypal, &expectation), listener.accept());
    let _connection = connection.unwrap();
    let error = result.unwrap_err();
    let ClientErrorKind::Transport { source } = error.kind() else {
        panic!("timeout lost transport cause");
    };
    assert!(source.is_timeout());
    assert!(std::ptr::eq(
        std::error::Error::source(&error)
            .unwrap()
            .downcast_ref()
            .unwrap(),
        source
    ));
    assert!(error.location().file().ends_with("client.rs"));
    assert_eq!(error.to_string(), "HTTP transport failed");
}

#[tokio::test]
async fn provider_connection_failure_retains_transport_source_and_origin() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    let fixture = Fixture::new(vec![]).await;
    let mut expectation = expected(&fixture);
    expectation.api_origin = origin.clone();
    let paypal = PaypalClient::new(&PaypalConfig {
        base_url: format!("{origin}/"),
        timeout_secs: 1,
        client_id: Some("fixture-client".into()),
        client_secret: Some("fixture-secret".into()),
    })
    .unwrap();
    let error = verify_payment(&paypal, &expectation).await.unwrap_err();
    let ClientErrorKind::Transport { source } = error.kind() else {
        panic!("connection failure lost transport cause");
    };
    assert!(source.is_connect());
    assert!(std::ptr::eq(
        std::error::Error::source(&error)
            .unwrap()
            .downcast_ref()
            .unwrap(),
        source
    ));
    assert!(error.location().file().ends_with("client.rs"));
    assert_eq!(error.to_string(), "HTTP transport failed");
}
