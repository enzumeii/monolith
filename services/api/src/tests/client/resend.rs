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

pub(crate) fn client_with_url(url: reqwest::Url) -> ResendClient {
    ResendClient {
        timeout: std::time::Duration::from_secs(10),
        inner: Resend::with_config(
            resend_rs::ConfigBuilder::new("re_private-fixture")
                .base_url(url)
                .build(),
        ),
    }
}

fn client(fixture: &Fixture) -> ResendClient {
    client_with_url(fixture.url.join("/").unwrap())
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
                timeout_secs: Some(10),
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

#[test]
fn missing_or_zero_deadlines_are_retained_as_client_initialization_errors() {
    for timeout_secs in [None, Some(0)] {
        let result = ResendClient::new(&ResendConfig {
            api_key: Some("fixture-key".into()),
            timeout_secs,
        });
        assert!(matches!(
            result.err().unwrap().kind(),
            ClientErrorKind::InvalidInput("Resend timeout is missing or zero")
        ));
    }
}

#[tokio::test]
async fn report_attachments_keep_raw_bytes_and_selected_mime_types() {
    let fixture = Fixture::new(vec![Reply::json(json!({"id":"report-email"}))]).await;
    let bytes = vec![0, 1, 2, 255];
    client(&fixture)
        .send_report_email(
            "sender@example.test",
            "recipient@example.test",
            "Report",
            "<p>Report</p>",
            vec![
                ("inventory.csv".into(), bytes.clone()),
                ("inventory.xlsx".into(), bytes.clone()),
            ],
            "report-result-fixture",
        )
        .await
        .unwrap();
    let requests = fixture.requests();
    assert_eq!(requests[0].1["idempotency-key"], "report-result-fixture");
    let attachments = &requests[0].2["attachments"];
    assert_eq!(attachments[0]["content"], json!(bytes));
    assert_eq!(attachments[1]["content"], json!(bytes));
    assert_eq!(attachments[0]["content_type"], "text/csv");
    assert_eq!(
        attachments[1]["content_type"],
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
    );
}
