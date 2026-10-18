#![allow(clippy::unwrap_used)]
use super::*;
use crate::{
    client::{
        ClientError, ClientErrorKind,
        tests::{Fixture, Reply},
    },
    config::ResendConfig,
};
use serde_json::json;
use std::error::Error;

fn client(fixture: &Fixture) -> ResendClient {
    ResendClient {
        inner: Resend::with_config(
            resend_rs::ConfigBuilder::new("re_private-fixture")
                .base_url(fixture.url.join("/").unwrap())
                .build(),
        ),
    }
}

#[tokio::test]
async fn sends_html_with_default_or_explicit_sender_and_returns_provider_id() {
    for (from, expected) in [
        (None, "shipping@enzumeii.com"),
        (Some("override@example.com"), "override@example.com"),
    ] {
        let fixture = Fixture::new(vec![Reply::json(json!({"id":"fixture-email-id"}))]).await;
        let id = client(&fixture)
            .send_email(
                from,
                &["delivered@resend.dev", "second@example.com"],
                "Shipment",
                "<p>On the way</p>",
            )
            .await
            .unwrap();
        assert_eq!(id.as_ref(), "fixture-email-id");
        let requests = fixture.requests();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].0, "/emails");
        assert_eq!(fixture.methods(), [reqwest::Method::POST]);
        assert_eq!(requests[0].1["authorization"], "Bearer re_private-fixture");
        assert_eq!(requests[0].1["content-type"], "application/json");
        assert_eq!(
            requests[0].2,
            json!({"from":expected,"to":["delivered@resend.dev","second@example.com"],"subject":"Shipment","html":"<p>On the way</p>"})
        );
    }
}

#[tokio::test]
async fn provider_failures_are_not_retried_and_preserve_sanitized_sources() {
    for status in [401, 422, 429, 500, 503] {
        let mut reply = Reply::json(
            json!({"statusCode":status,"name":"validation_error","message":"re_private-fixture private-recipient@example.com <p>private email</p>"}),
        );
        reply.status = status;
        reply.headers.push(("retry-after".into(), "1".into()));
        let fixture = Fixture::new(vec![reply, Reply::json(json!({"id":"must-not-retry"}))]).await;
        let error = client(&fixture)
            .send_email(None, &["delivered@resend.dev"], "Hi", "private email")
            .await
            .unwrap_err();
        assert_eq!(fixture.requests().len(), 1);
        assert!(matches!(error.kind(), ClientErrorKind::Upstream { .. }));
        let source = error
            .source()
            .unwrap()
            .downcast_ref::<resend_rs::Error>()
            .unwrap();
        if status == 429 {
            assert!(matches!(source, resend_rs::Error::RateLimit { .. }));
        } else {
            assert!(matches!(source, resend_rs::Error::Resend(_)));
        }
        assert_safe(&error);
        assert!(error.location().file().ends_with("client/resend.rs"));
    }
}

#[tokio::test]
async fn unusable_responses_are_rejected_without_retry_or_payload_leaks() {
    for body in ["not json private email", "{}"] {
        let fixture = Fixture::new(vec![Reply {
            status: 200,
            body: body.into(),
            headers: vec![],
        }])
        .await;
        let error = client(&fixture)
            .send_email(None, &["delivered@resend.dev"], "Hi", "private email")
            .await
            .unwrap_err();
        assert_eq!(fixture.requests().len(), 1);
        assert!(error.source().is_some());
        assert_safe(&error);
    }
    let mut reply = Reply::json(json!({"message":"private email"}));
    reply.status = 502;
    let fixture = Fixture::new(vec![reply]).await;
    let error = client(&fixture)
        .send_email(None, &["delivered@resend.dev"], "Hi", "private email")
        .await
        .unwrap_err();
    assert!(matches!(
        error
            .source()
            .unwrap()
            .downcast_ref::<resend_rs::Error>()
            .unwrap(),
        resend_rs::Error::Parse { .. }
    ));
    assert_safe(&error);
}

#[tokio::test]
async fn transport_failure_retains_its_cause() {
    let fixture = Fixture::new(vec![]).await;
    let client = client(&fixture);
    drop(fixture);
    let error = client
        .send_email(None, &["delivered@resend.dev"], "Hi", "private email")
        .await
        .unwrap_err();
    assert!(!crate::error::client_timeout(&error));
    assert!(matches!(
        error
            .source()
            .unwrap()
            .downcast_ref::<resend_rs::Error>()
            .unwrap(),
        resend_rs::Error::Http(_)
    ));
    assert_safe(&error);
}

#[test]
fn missing_or_empty_api_keys_are_rejected() {
    for key in [None, Some(""), Some(" ")] {
        assert!(
            ResendClient::new(&ResendConfig {
                api_key: key.map(str::to_owned)
            })
            .is_err()
        );
    }
}

fn assert_safe(error: &ClientError) {
    let rendered = format!("{error} {error:?}");
    for secret in ["re_private-fixture", "private-recipient", "private email"] {
        assert!(!rendered.contains(secret));
    }
}
