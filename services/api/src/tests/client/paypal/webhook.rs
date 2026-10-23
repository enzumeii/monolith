#![allow(clippy::unwrap_used)]
use super::*;
use crate::client::{
    ClientErrorKind,
    tests::{Fixture, Reply},
};
use serde_json::{Value, json};

fn request(event_type: &str, resource: Value) -> VerifyWebhookSignatureRequest {
    let mut webhook_event = json!({"id":"EVENT123", "event_type":event_type});
    webhook_event["resource"] = resource;
    VerifyWebhookSignatureRequest {
        auth_algo: "SHA256withRSA".into(),
        cert_url: "https://api.paypal.com/v1/notifications/certs/CERT123".into(),
        transmission_id: "tx-123".into(),
        transmission_sig: "synthetic-signature".into(),
        transmission_time: "2026-10-23T00:00:00Z".into(),
        webhook_id: "WH123".into(),
        webhook_event,
    }
}

fn completed() -> VerifyWebhookSignatureRequest {
    request(
        "PAYMENT.CAPTURE.COMPLETED",
        json!({"id":"CAPTURE123", "supplementary_data":{"related_ids":{"order_id":"ORDER123"}}}),
    )
}

fn verification_reply(status: &str) -> Reply {
    Reply::json(json!({"verification_status":status}))
}

#[tokio::test]
async fn successful_postback_returns_only_notification_identity() {
    let f = Fixture::new(vec![
        super::super::tests::token("fixture-token"),
        verification_reply("SUCCESS"),
    ])
    .await;
    let client = PaypalClient::new(&super::super::tests::config(&f)).unwrap();
    let payload = completed();
    let result = verify_event(&client, &payload).await.unwrap();
    assert_eq!(
        result,
        VerifiedEvent {
            id: "EVENT123".into(),
            event_type: WebhookEventType::CaptureCompleted,
            order_id: Some("ORDER123".into()),
            capture_id: Some("CAPTURE123".into()),
        }
    );
    let requests = f.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[1].0, "/v1/notifications/verify-webhook-signature");
    assert_eq!(requests[1].2, serde_json::to_value(payload).unwrap());
    assert_eq!(f.methods(), [reqwest::Method::POST, reqwest::Method::POST]);
}

#[tokio::test]
async fn unsuccessful_or_unknown_postback_never_returns_identity() {
    for status in ["FAILURE", "FUTURE_STATUS", ""] {
        let f = Fixture::new(vec![
            super::super::tests::token("fixture-token"),
            verification_reply(status),
        ])
        .await;
        let client = PaypalClient::new(&super::super::tests::config(&f)).unwrap();
        assert!(matches!(
            verify_event(&client, &completed()).await,
            Err(WebhookError::InvalidSignature)
        ));
        assert_eq!(f.requests().len(), 2);
    }
}

#[tokio::test]
async fn unknown_notifications_remain_typed_notifications() {
    let f = Fixture::new(vec![
        super::super::tests::token("fixture-token"),
        verification_reply("SUCCESS"),
    ])
    .await;
    let client = PaypalClient::new(&super::super::tests::config(&f)).unwrap();
    let result = verify_event(
        &client,
        &request("FUTURE.EVENT", json!({"anything":"opaque"})),
    )
    .await
    .unwrap();
    assert_eq!(
        result.event_type,
        WebhookEventType::Other("FUTURE.EVENT".into())
    );
    assert_eq!(result.order_id, None);
    assert_eq!(result.capture_id, None);
}

#[tokio::test]
async fn order_notifications_and_refunds_preserve_distinct_resource_identities() {
    for (kind, resource, order, capture) in [
        (
            "CHECKOUT.ORDER.APPROVED",
            json!({"id":"ORDER123"}),
            Some("ORDER123"),
            None,
        ),
        (
            "CHECKOUT.ORDER.COMPLETED",
            json!({"id":"ORDER123"}),
            Some("ORDER123"),
            None,
        ),
        (
            "PAYMENT.CAPTURE.PENDING",
            json!({"id":"CAPTURE123"}),
            None,
            Some("CAPTURE123"),
        ),
        (
            "PAYMENT.CAPTURE.DENIED",
            json!({"id":"CAPTURE123"}),
            None,
            Some("CAPTURE123"),
        ),
        (
            "PAYMENT.CAPTURE.REVERSED",
            json!({"id":"CAPTURE123"}),
            None,
            Some("CAPTURE123"),
        ),
        (
            "PAYMENT.CAPTURE.REFUNDED",
            json!({"id":"REFUND123"}),
            None,
            None,
        ),
        (
            "PAYMENT.CAPTURE.REFUNDED",
            json!({"id":"REFUND123", "supplementary_data":{"related_ids":{"capture_id":"CAPTURE123", "order_id":"ORDER123"}}}),
            Some("ORDER123"),
            Some("CAPTURE123"),
        ),
    ] {
        let f = Fixture::new(vec![
            super::super::tests::token("fixture-token"),
            verification_reply("SUCCESS"),
        ])
        .await;
        let client = PaypalClient::new(&super::super::tests::config(&f)).unwrap();
        let result = verify_event(&client, &request(kind, resource))
            .await
            .unwrap();
        assert_eq!(result.order_id.as_deref(), order, "{kind}");
        assert_eq!(result.capture_id.as_deref(), capture, "{kind}");
        assert_eq!(result.event_type.as_str(), kind);
        assert_eq!(f.requests().len(), 2);
    }
}

#[tokio::test]
async fn malformed_envelopes_and_optional_references_fail_before_postback() {
    let f = Fixture::new(vec![]).await;
    let client = PaypalClient::new(&super::super::tests::config(&f)).unwrap();
    for event in [
        Value::Null,
        json!({"id":"EVENT123", "event_type":"PAYMENT.CAPTURE.COMPLETED"}),
        json!({"id":"EVENT123", "event_type":"PAYMENT.CAPTURE.COMPLETED", "resource":[]}),
        json!({"id":"", "event_type":"PAYMENT.CAPTURE.COMPLETED", "resource":{"id":"CAPTURE123"}}),
        json!({"id":" ", "event_type":"PAYMENT.CAPTURE.COMPLETED", "resource":{"id":"CAPTURE123"}}),
        json!({"id":"EVENT123", "event_type":"", "resource":{"id":"CAPTURE123"}}),
        json!({"id":"EVENT123", "event_type":"CHECKOUT.ORDER.APPROVED", "resource":{"id":4}}),
        json!({"id":"EVENT123", "event_type":"PAYMENT.CAPTURE.COMPLETED", "resource":{}}),
        json!({"id":"EVENT123", "event_type":"PAYMENT.CAPTURE.COMPLETED", "resource":{"id":"CAPTURE\n123"}}),
        json!({"id":"EVENT123", "event_type":"PAYMENT.CAPTURE.COMPLETED", "resource":{"id":"x".repeat(256)}}),
    ] {
        let mut payload = completed();
        payload.webhook_event = event;
        assert!(matches!(
            verify_event(&client, &payload).await,
            Err(WebhookError::InvalidEvent)
        ));
    }
    for related in [
        json!(null),
        json!([]),
        json!({"order_id":null}),
        json!({"order_id":7}),
        json!({"order_id":""}),
        json!({"capture_id":""}),
        json!({"capture_id":"CAPTURE-OTHER"}),
    ] {
        let mut payload = completed();
        payload.webhook_event["resource"]["supplementary_data"]["related_ids"] = related;
        assert!(matches!(
            verify_event(&client, &payload).await,
            Err(WebhookError::InvalidEvent)
        ));
    }
    let mut payload = request(
        "CHECKOUT.ORDER.APPROVED",
        json!({"id":"ORDER123", "supplementary_data":{"related_ids":{"order_id":"OTHER123"}}}),
    );
    assert!(matches!(
        verify_event(&client, &payload).await,
        Err(WebhookError::InvalidEvent)
    ));
    payload = request(
        "PAYMENT.CAPTURE.REFUNDED",
        json!({"id":"REFUND123", "supplementary_data":{"related_ids":{"capture_id":42}}}),
    );
    assert!(matches!(
        verify_event(&client, &payload).await,
        Err(WebhookError::InvalidEvent)
    ));
    assert_eq!(f.requests(), []);
}

#[tokio::test]
async fn malformed_headers_configuration_and_bounds_fail_before_postback() {
    let f = Fixture::new(vec![]).await;
    let client = PaypalClient::new(&super::super::tests::config(&f)).unwrap();
    for field in [
        "auth_algo",
        "cert_url",
        "transmission_id",
        "transmission_sig",
        "transmission_time",
    ] {
        for value in ["", " ", "bad\nheader"] {
            let mut payload = serde_json::to_value(completed()).unwrap();
            payload[field] = json!(value);
            let payload = serde_json::from_value(payload).unwrap();
            assert!(
                matches!(
                    verify_event(&client, &payload).await,
                    Err(WebhookError::InvalidHeader)
                ),
                "{field}"
            );
        }
        let mut payload = serde_json::to_value(completed()).unwrap();
        payload[field] = json!("x".repeat(4097));
        let payload = serde_json::from_value(payload).unwrap();
        assert!(
            matches!(
                verify_event(&client, &payload).await,
                Err(WebhookError::InputTooLarge)
            ),
            "{field}"
        );
    }
    for url in [
        "http://api.paypal.com/cert",
        "https://user:private@api.paypal.com/cert",
        "not-a-url",
        "https://api.paypal.com/cert#fragment",
    ] {
        let mut payload = completed();
        payload.cert_url = url.into();
        assert!(matches!(
            verify_event(&client, &payload).await,
            Err(WebhookError::InvalidHeader)
        ));
    }
    let mut payload = completed();
    payload.transmission_time = "not-a-time".into();
    assert!(matches!(
        verify_event(&client, &payload).await,
        Err(WebhookError::InvalidHeader)
    ));
    for webhook_id in ["", " ", "bad\nid"] {
        let mut payload = completed();
        payload.webhook_id = webhook_id.into();
        assert!(matches!(
            verify_event(&client, &payload).await,
            Err(WebhookError::InvalidConfiguration)
        ));
    }
    let mut payload = completed();
    payload.webhook_event["resource"]["large"] = json!("x".repeat(262_145));
    assert!(matches!(
        verify_event(&client, &payload).await,
        Err(WebhookError::InputTooLarge)
    ));
    assert_eq!(f.requests(), []);
}

#[tokio::test]
async fn provider_http_and_decoding_failures_preserve_typed_sanitized_causes() {
    for reply in [
        Reply::status(403),
        Reply::status(500),
        Reply::json(json!({})),
        Reply {
            status: 200,
            body: "private payer invalid json".into(),
            headers: vec![],
        },
    ] {
        let status = reply.status;
        let f = Fixture::new(vec![super::super::tests::token("fixture-token"), reply]).await;
        let client = PaypalClient::new(&super::super::tests::config(&f)).unwrap();
        let error = verify_event(&client, &completed()).await.unwrap_err();
        let WebhookError::Upstream(source) = &error else {
            panic!("expected upstream error: {error:?}");
        };
        if status == 200 {
            assert!(matches!(source.kind(), ClientErrorKind::Decode { .. }));
            assert!(std::error::Error::source(source).is_some());
        } else {
            assert!(
                matches!(source.kind(), ClientErrorKind::Http { status: actual, .. } if *actual == status)
            );
        }
        assert!(
            std::error::Error::source(&error)
                .unwrap()
                .is::<ClientError>()
        );
        let rendered = format!("{error} {error:?}");
        assert!(!rendered.contains("private payer"));
        assert!(!rendered.contains("fixture-token"));
        assert_eq!(f.requests().len(), 2);
    }
}

#[tokio::test]
async fn provider_connection_and_timeout_failures_remain_typed() {
    let f = Fixture::new(vec![]).await;
    let mut config = super::super::tests::config(&f);
    drop(f);
    let error = verify_event(&PaypalClient::new(&config).unwrap(), &completed())
        .await
        .unwrap_err();
    assert!(
        matches!(error, WebhookError::Upstream(ref source) if matches!(source.kind(), ClientErrorKind::Transport { source } if source.is_connect()))
    );

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    config.base_url = format!("http://{}/", listener.local_addr().unwrap());
    let error = verify_event(&PaypalClient::new(&config).unwrap(), &completed())
        .await
        .unwrap_err();
    assert!(
        matches!(error, WebhookError::Upstream(ref source) if matches!(source.kind(), ClientErrorKind::Transport { source } if source.is_timeout()))
    );
}
