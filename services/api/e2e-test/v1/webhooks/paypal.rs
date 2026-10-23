#![allow(clippy::unwrap_used)]
use crate::{
    common::{body_json, fixture},
    v1::payments::paypal::{PaypalProvider, call, create, input},
};
use axum::{body::Body, http::Request};
use serde_json::{Value, json};
const ROOT: &str = "/api/v1/webhooks/paypal";
const HEADERS: [(&str, &str); 5] = [
    ("paypal-auth-algo", "SHA256withRSA"),
    (
        "paypal-cert-url",
        "https://api.sandbox.paypal.com/v1/notifications/certs/synthetic",
    ),
    ("paypal-transmission-id", "synthetic-transmission"),
    ("paypal-transmission-sig", "synthetic-signature"),
    ("paypal-transmission-time", "2026-10-23T00:00:00Z"),
];
fn request(event: &Value, exclude: Option<&str>) -> Request<Body> {
    let mut request = Request::builder()
        .method("POST")
        .uri(ROOT)
        .header("content-type", "application/json");
    for (name, value) in HEADERS {
        if Some(name) != exclude {
            request = request.header(name, value);
        }
    }
    request.body(Body::from(event.to_string())).unwrap()
}
fn event(prefix: &str, kind: &str, order: &str) -> Value {
    json!({"id":format!("{prefix}-{}",uuid::Uuid::new_v4()),"event_type":kind,"resource":{"id":format!("CAP-{order}"),"supplementary_data":{"related_ids":{"order_id":order}}}})
}

#[tokio::test]
async fn paypal_webhook_verifies_before_retrieval_and_deduplicates_completed_evidence() {
    let fixture = fixture().await;
    let provider = PaypalProvider::start().await;
    let client = provider.client().await;
    let created = create(&client, &input(&fixture)).await;
    let order = created["paypal_order_id"].as_str().unwrap();
    let id = created["id"].as_str().unwrap();
    provider.completed();
    let event = event(fixture.prefix(), "PAYMENT.CAPTURE.COMPLETED", order);
    let count = provider.calls().len();
    assert_eq!(
        client.send(request(&event, None)).await.unwrap().status(),
        200
    );
    let calls = provider.calls();
    assert_eq!(calls[count].1, "/v1/notifications/verify-webhook-signature");
    assert_eq!(calls[count].3["webhook_event"], event);
    assert!(
        calls[count..]
            .iter()
            .any(|(m, p, _, _)| m == "GET" && p.contains("/v2/payments/captures/"))
    );
    assert!(
        !calls[count..]
            .iter()
            .any(|(m, p, _, _)| m == "POST" && p.ends_with("/capture"))
    );
    let saved: (String, Option<uuid::Uuid>) =
        sqlx::query_as("SELECT state,payment_id FROM paypal_payment_events WHERE event_id=$1")
            .bind(event["id"].as_str().unwrap())
            .fetch_one(&fixture.pool())
            .await
            .unwrap();
    assert_eq!(
        saved,
        ("PROCESSED".into(), Some(uuid::Uuid::parse_str(id).unwrap()))
    );
    let count = provider.calls().len();
    assert_eq!(
        client.send(request(&event, None)).await.unwrap().status(),
        200
    );
    assert_eq!(provider.calls().len(), count + 1);
    assert_eq!(
        body_json(call(&client, "GET", id).await).await["status"],
        "COMPLETED"
    );
    let mut reordered = event.clone();
    reordered["id"] = json!(format!("{}-old-event", fixture.prefix()));
    reordered["event_type"] = json!("CHECKOUT.ORDER.APPROVED");
    reordered["resource"]["id"] = json!(order);
    assert_eq!(
        client
            .send(request(&reordered, None))
            .await
            .unwrap()
            .status(),
        200
    );
    assert_eq!(
        body_json(call(&client, "GET", id).await).await["status"],
        "COMPLETED"
    );
    let mut altered = event.clone();
    altered["resource"]["note"] = json!("different signed payload");
    assert_eq!(
        client.send(request(&altered, None)).await.unwrap().status(),
        409
    );
}

#[tokio::test]
async fn paypal_webhook_each_header_and_event_rejection_prevents_business_calls() {
    let fixture = fixture().await;
    let provider = PaypalProvider::start().await;
    let client = provider.client().await;
    let valid = event(
        fixture.prefix(),
        "PAYMENT.CAPTURE.COMPLETED",
        "unknown-order",
    );
    for (header, _) in HEADERS {
        assert_eq!(
            client
                .send(request(&valid, Some(header)))
                .await
                .unwrap()
                .status(),
            400,
            "missing {header}"
        );
        for value in [" ".to_owned(), "x".repeat(4097)] {
            let mut invalid = request(&valid, None);
            invalid.headers_mut().insert(header, value.parse().unwrap());
            assert_eq!(
                client.send(invalid).await.unwrap().status(),
                400,
                "invalid {header}"
            );
        }
        let mut invalid = request(&valid, None);
        invalid
            .headers_mut()
            .append(header, "duplicate".parse().unwrap());
        assert_eq!(
            client.send(invalid).await.unwrap().status(),
            400,
            "duplicate {header}"
        );
    }
    for (header, value) in [
        ("paypal-transmission-time", "invalid-time"),
        ("paypal-cert-url", "http://example.test/cert"),
        ("paypal-cert-url", "https://user:password@example.test/cert"),
        ("paypal-cert-url", "invalid-url"),
    ] {
        let mut invalid = request(&valid, None);
        invalid.headers_mut().insert(header, value.parse().unwrap());
        assert_eq!(client.send(invalid).await.unwrap().status(), 400);
    }
    for field in ["id", "event_type", "resource"] {
        let mut invalid = valid.clone();
        invalid.as_object_mut().unwrap().remove(field);
        assert_eq!(
            client.send(request(&invalid, None)).await.unwrap().status(),
            400,
            "missing {field}"
        );
        let mut invalid = valid.clone();
        invalid[field] = json!(false);
        assert_eq!(
            client.send(request(&invalid, None)).await.unwrap().status(),
            400,
            "wrongtype {field}"
        );
    }
    for (field, value) in [
        ("id", json!("")),
        ("id", json!("x".repeat(256))),
        ("id", json!("control\n")),
        ("event_type", json!("")),
        ("event_type", json!("x".repeat(256))),
        ("event_type", json!("control\n")),
        ("resource", json!({})),
        ("resource", json!({"id":""})),
        ("resource", json!({"id":"x".repeat(256)})),
        (
            "resource",
            json!({"id":"capture","supplementary_data":{"related_ids":{"order_id":"bad\n"}}}),
        ),
    ] {
        let mut invalid = valid.clone();
        invalid[field] = value;
        assert_eq!(
            client.send(request(&invalid, None)).await.unwrap().status(),
            400,
            "{invalid}"
        );
    }
    for body in [String::new(), "{".into(), " ".repeat(262_145)] {
        let mut invalid = request(&valid, None);
        *invalid.body_mut() = Body::from(body);
        assert_eq!(client.send(invalid).await.unwrap().status(), 400);
    }
    assert_eq!(provider.calls().len(), 0);
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM paypal_payment_events WHERE left(event_id,12)=$1")
            .bind(fixture.prefix())
            .fetch_one(&fixture.pool())
            .await
            .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn paypal_webhook_signature_rejections_and_provider_failures_do_not_complete() {
    let fixture = fixture().await;
    for signature in ["FAILURE", "UNRECOGNIZED"] {
        let provider = PaypalProvider::start().await;
        let client = provider.client().await;
        provider.signature(signature);
        let event = event(
            fixture.prefix(),
            "PAYMENT.CAPTURE.COMPLETED",
            "unknown-order",
        );
        assert_eq!(
            client.send(request(&event, None)).await.unwrap().status(),
            400
        );
        let count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM paypal_payment_events WHERE event_id=$1")
                .bind(event["id"].as_str().unwrap())
                .fetch_one(&fixture.pool())
                .await
                .unwrap();
        assert_eq!(count, 0);
    }
    for path in [
        "/v1/notifications/verify-webhook-signature",
        "/v2/checkout/orders/",
        "/v2/payments/captures/",
    ] {
        for fault in ["http", "malformed", "timeout", "disconnect"] {
            let provider = PaypalProvider::start().await;
            let client = provider.client().await;
            let created = create(&client, &input(&fixture)).await;
            provider.completed();
            provider.fault(path, fault);
            let before = provider.calls().len();
            let event = event(
                fixture.prefix(),
                "PAYMENT.CAPTURE.COMPLETED",
                created["paypal_order_id"].as_str().unwrap(),
            );
            let response = client.send(request(&event, None)).await.unwrap();
            let status = response.status();
            let body = body_json(response).await;
            assert_eq!(
                status,
                if fault == "timeout" { 504 } else { 502 },
                "{path}/{fault}: {body}"
            );
            assert!(!body.to_string().contains("private"));
            let verified: Option<String> =
                sqlx::query_scalar("SELECT verified_capture_id FROM paypal_payments WHERE id=$1")
                    .bind(uuid::Uuid::parse_str(created["id"].as_str().unwrap()).unwrap())
                    .fetch_one(&fixture.pool())
                    .await
                    .unwrap();
            assert!(verified.is_none());
            let calls = provider.calls();
            assert!(
                calls[before..]
                    .iter()
                    .any(|(method, target, _, _)| target.starts_with(path)
                        && method
                            == if path.contains("verify-webhook") {
                                "POST"
                            } else {
                                "GET"
                            }),
                "targeted {path}/{fault} was not reached"
            );
            if !path.contains("verify-webhook") {
                assert_receipt(&fixture, &event, "PENDING", false).await;
                provider.clear_fault();
                assert_eq!(
                    client.send(request(&event, None)).await.unwrap().status(),
                    200
                );
                assert_receipt(&fixture, &event, "PROCESSED", true).await;
                let status =
                    body_json(call(&client, "GET", created["id"].as_str().unwrap()).await).await;
                assert_eq!(status["status"], "COMPLETED");
                assert!(status["capture_id"].is_string());
            }
        }
    }
}

#[tokio::test]
async fn paypal_webhook_verified_unknown_and_unrelated_events_are_ignored() {
    let fixture = fixture().await;
    let provider = PaypalProvider::start().await;
    let client = provider.client().await;
    for kind in ["PAYMENT.CAPTURE.COMPLETED", "UNRELATED.EVENT"] {
        let event = event(fixture.prefix(), kind, "unowned-order");
        assert_eq!(
            client.send(request(&event, None)).await.unwrap().status(),
            200
        );
        let state: String =
            sqlx::query_scalar("SELECT state FROM paypal_payment_events WHERE event_id=$1")
                .bind(event["id"].as_str().unwrap())
                .fetch_one(&fixture.pool())
                .await
                .unwrap();
        assert_eq!(state, "IGNORED");
    }
    assert!(
        provider
            .calls()
            .iter()
            .all(|(_, p, _, _)| p == "/v1/oauth2/token"
                || p == "/v1/notifications/verify-webhook-signature")
    );
}

#[tokio::test]
async fn paypal_webhook_database_failure_and_missing_configuration_are_fail_closed() {
    let fixture = fixture().await;
    let provider = PaypalProvider::start().await;
    let client = provider.client().await;
    let event = event(fixture.prefix(), "UNRELATED.EVENT", "unowned-order");
    let condition = format!("NEW.event_id = '{}'", event["id"].as_str().unwrap());
    let fault = crate::v1::payments::paypal::database_fault(
        &fixture,
        "paypal_payment_events",
        "INSERT",
        &condition,
    )
    .await;
    let response = client.send(request(&event, None)).await.unwrap();
    assert_eq!(response.status(), 500);
    assert!(!body_json(response).await.to_string().contains("private"));
    drop(fault);
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM paypal_payment_events WHERE event_id=$1")
            .bind(event["id"].as_str().unwrap())
            .fetch_one(&fixture.pool())
            .await
            .unwrap();
    assert_eq!(count, 0);
    let fault = crate::v1::payments::paypal::database_fault(
        &fixture,
        "paypal_payment_events",
        "UPDATE",
        &condition,
    )
    .await;
    assert_eq!(
        client.send(request(&event, None)).await.unwrap().status(),
        500
    );
    drop(fault);
    let state: String =
        sqlx::query_scalar("SELECT state FROM paypal_payment_events WHERE event_id=$1")
            .bind(event["id"].as_str().unwrap())
            .fetch_one(&fixture.pool())
            .await
            .unwrap();
    assert_eq!(state, "PENDING");
    assert_eq!(
        client.send(request(&event, None)).await.unwrap().status(),
        200
    );
    let unconfigured = crate::common::client_with_providers(crate::common::ProviderConfig {
        paypal_url: Some(&provider.url),
        ..Default::default()
    })
    .await;
    let count = provider.calls().len();
    assert_eq!(
        unconfigured
            .send(request(&event, None))
            .await
            .unwrap()
            .status(),
        503
    );
    assert_eq!(provider.calls().len(), count);
}

#[tokio::test]
async fn paypal_webhook_capture_without_order_binding_resolves_authoritatively() {
    let fixture = fixture().await;
    let provider = PaypalProvider::start().await;
    let client = provider.client().await;
    let created = create(&client, &input(&fixture)).await;
    let order = created["paypal_order_id"].as_str().unwrap();
    provider.completed();
    let mut event = event(fixture.prefix(), "PAYMENT.CAPTURE.COMPLETED", order);
    event["resource"]
        .as_object_mut()
        .unwrap()
        .remove("supplementary_data");
    assert_eq!(
        client.send(request(&event, None)).await.unwrap().status(),
        200
    );
    assert_eq!(
        body_json(call(&client, "GET", created["id"].as_str().unwrap()).await).await["status"],
        "COMPLETED"
    );
    assert!(
        !provider
            .calls()
            .iter()
            .any(|(m, p, _, _)| m == "POST" && p.ends_with("/capture"))
    );
}

#[tokio::test]
async fn paypal_webhook_and_capture_overlap_reconcile_without_duplicate_mutation() {
    let fixture = fixture().await;
    let provider = PaypalProvider::start().await;
    let client = provider.client().await;
    let created = create(&client, &input(&fixture)).await;
    provider.completed();
    let event = event(
        fixture.prefix(),
        "PAYMENT.CAPTURE.COMPLETED",
        created["paypal_order_id"].as_str().unwrap(),
    );
    let id = created["id"].as_str().unwrap();
    let (capture, webhook) = tokio::join!(
        call(&client, "POST", id),
        client.send(request(&event, None))
    );
    assert!([200, 202].contains(&capture.status().as_u16()));
    assert!([200, 503].contains(&webhook.unwrap().status().as_u16()));
    assert_eq!(
        client.send(request(&event, None)).await.unwrap().status(),
        200
    );
    assert_eq!(
        body_json(call(&client, "GET", id).await).await["status"],
        "COMPLETED"
    );
    assert!(
        !provider
            .calls()
            .iter()
            .any(|(m, p, _, _)| m == "POST" && p.ends_with("/capture"))
    );
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM paypal_payment_captures WHERE payment_id=$1")
            .bind(uuid::Uuid::parse_str(id).unwrap())
            .fetch_one(&fixture.pool())
            .await
            .unwrap();
    assert_eq!(count, 1);
}

#[tokio::test]
async fn paypal_webhook_oauth_failure_prevents_signature_and_event_persistence() {
    let fixture = fixture().await;
    for kind in ["http", "malformed", "timeout", "disconnect"] {
        let provider = PaypalProvider::start().await;
        let client = provider.client().await;
        provider.fault("/v1/oauth2/token", kind);
        let event = event(fixture.prefix(), "UNRELATED.EVENT", "unknown");
        let response = client.send(request(&event, None)).await.unwrap();
        assert_eq!(response.status(), if kind == "timeout" { 504 } else { 502 });
        assert_eq!(provider.calls().len(), 1);
        let count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM paypal_payment_events WHERE event_id=$1")
                .bind(event["id"].as_str().unwrap())
                .fetch_one(&fixture.pool())
                .await
                .unwrap();
        assert_eq!(count, 0);
    }
}

#[tokio::test]
async fn paypal_regression_completed_webhook_busy_or_pending_remains_retryable() {
    let fixture = fixture().await;
    for busy in [true, false] {
        let provider = PaypalProvider::start().await;
        let client = provider.client().await;
        let created = create(&client, &input(&fixture)).await;
        provider.completed();
        let id = uuid::Uuid::parse_str(created["id"].as_str().unwrap()).unwrap();
        if busy {
            sqlx::query("UPDATE paypal_payments SET operation='RECONCILE',lease_token=$2,lease_until=clock_timestamp()+interval '5 minutes' WHERE id=$1").bind(id).bind(uuid::Uuid::new_v4()).execute(&fixture.pool()).await.unwrap();
        } else {
            provider.capture_status("PENDING");
        }
        let event = event(
            fixture.prefix(),
            "PAYMENT.CAPTURE.COMPLETED",
            created["paypal_order_id"].as_str().unwrap(),
        );
        let response = client.send(request(&event, None)).await.unwrap();
        assert_eq!(response.status(), 503, "busy={busy}");
        let (state, processed): (String, Option<chrono::DateTime<chrono::Utc>>) = sqlx::query_as(
            "SELECT state,processed_at FROM paypal_payment_events WHERE event_id=$1",
        )
        .bind(event["id"].as_str().unwrap())
        .fetch_one(&fixture.pool())
        .await
        .unwrap();
        assert_eq!(state, "PENDING");
        assert!(processed.is_none());
        let verified: Option<String> =
            sqlx::query_scalar("SELECT verified_capture_id FROM paypal_payments WHERE id=$1")
                .bind(id)
                .fetch_one(&fixture.pool())
                .await
                .unwrap();
        assert!(verified.is_none());
        if busy {
            sqlx::query("UPDATE paypal_payments SET operation=NULL,lease_token=NULL,lease_until=NULL WHERE id=$1").bind(id).execute(&fixture.pool()).await.unwrap();
        }
        provider.capture_status("COMPLETED");
        assert_eq!(
            client.send(request(&event, None)).await.unwrap().status(),
            200
        );
        assert_eq!(
            body_json(call(&client, "GET", created["id"].as_str().unwrap()).await).await["status"],
            "COMPLETED"
        );
    }
}

async fn assert_receipt(
    fixture: &crate::common::TestFixture,
    event: &Value,
    expected: &str,
    processed: bool,
) {
    let (state, at): (String, Option<chrono::DateTime<chrono::Utc>>) =
        sqlx::query_as("SELECT state,processed_at FROM paypal_payment_events WHERE event_id=$1")
            .bind(event["id"].as_str().unwrap())
            .fetch_one(&fixture.pool())
            .await
            .unwrap();
    assert_eq!(state, expected);
    assert_eq!(at.is_some(), processed);
}

#[tokio::test]
async fn paypal_review_wrong_capture_event_stays_unprocessed_without_paid_evidence() {
    let fixture = fixture().await;
    let provider = PaypalProvider::start().await;
    let client = provider.client().await;
    let created = create(&client, &input(&fixture)).await;
    provider.completed();
    let order = created["paypal_order_id"].as_str().unwrap();
    let mut event = event(fixture.prefix(), "PAYMENT.CAPTURE.COMPLETED", order);
    event["resource"]["id"] = json!("WRONG-CAPTURE-IDENTITY");
    let before = provider.calls().len();
    let response = client.send(request(&event, None)).await.unwrap();
    assert_eq!(response.status(), 502);
    assert_eq!(
        body_json(response).await,
        json!({"error":"PayPal payment evidence does not match"})
    );
    assert_receipt(&fixture, &event, "PENDING", false).await;
    let (exception,verified,count):(Option<String>,Option<String>,i64)=sqlx::query_as("SELECT exception,verified_capture_id,(SELECT count(*) FROM paypal_payment_captures WHERE payment_id=p.id) FROM paypal_payments p WHERE id=$1").bind(uuid::Uuid::parse_str(created["id"].as_str().unwrap()).unwrap()).fetch_one(&fixture.pool()).await.unwrap();
    assert_eq!(exception.as_deref(), Some("WEBHOOK_CAPTURE_MISMATCH"));
    assert!(verified.is_none());
    assert_eq!(count, 0);
    let calls = provider.calls();
    assert_eq!(
        calls[before].1,
        "/v1/notifications/verify-webhook-signature"
    );
    assert!(
        calls[before..]
            .iter()
            .any(|(method, path, _, _)| method == "GET"
                && path == &format!("/v2/checkout/orders/{order}"))
    );
    assert!(
        !calls[before..]
            .iter()
            .any(|(method, path, _, _)| method == "POST"
                && path != "/v1/notifications/verify-webhook-signature")
    );
}

#[tokio::test]
async fn paypal_review_second_successful_capture_on_same_payment_retains_both_and_flags_review() {
    let fixture = fixture().await;
    for kind in ["CHECKOUT.ORDER.COMPLETED", "PAYMENT.CAPTURE.COMPLETED"] {
        let provider = PaypalProvider::start().await;
        let client = provider.client().await;
        let created = create(&client, &input(&fixture)).await;
        let id = created["id"].as_str().unwrap();
        let first = call(&client, "POST", id).await;
        assert_eq!(first.status(), 200);
        let first = body_json(first).await;
        let first_capture = first["capture_id"].as_str().unwrap();
        let order = created["paypal_order_id"].as_str().unwrap();
        let second_capture = format!("SECOND-{order}");
        provider.capture_identity(order, &second_capture);
        let mut event = event(fixture.prefix(), kind, order);
        event["resource"]["id"] = json!(if kind == "CHECKOUT.ORDER.COMPLETED" {
            order
        } else {
            &second_capture
        });
        let before = provider.calls().len();
        let response = client.send(request(&event, None)).await.unwrap();
        let status = response.status();
        let body = if status == 200 {
            Value::Null
        } else {
            body_json(response).await
        };
        let (exception, verified): (Option<String>, Option<String>) =
            sqlx::query_as("SELECT exception,verified_capture_id FROM paypal_payments WHERE id=$1")
                .bind(uuid::Uuid::parse_str(id).unwrap())
                .fetch_one(&fixture.pool())
                .await
                .unwrap();
        let captures:Vec<String>=sqlx::query_scalar("SELECT capture_id FROM paypal_payment_captures WHERE payment_id=$1 ORDER BY capture_id").bind(uuid::Uuid::parse_str(id).unwrap()).fetch_all(&fixture.pool()).await.unwrap();
        assert_eq!(captures.len(), 2, "{kind}: {status} {body}");
        assert!(captures.iter().any(|capture| capture == first_capture));
        assert!(captures.contains(&second_capture));
        assert_eq!(verified.as_deref(), Some(first_capture));
        assert_eq!(
            exception.as_deref(),
            Some("MULTIPLE_SUCCESSFUL_CAPTURES"),
            "{kind}: {status} {body}"
        );
        assert_eq!(status, 200, "{kind}: {body}");
        assert_receipt(&fixture, &event, "PROCESSED", true).await;
        assert_eq!(call(&client, "POST", id).await.status(), 409);
        let owned = body_json(call(&client, "GET", id).await).await;
        assert_eq!(owned["capture_id"], first_capture);
        assert_eq!(owned["exception"], "MULTIPLE_SUCCESSFUL_CAPTURES");
        assert!(
            !provider.calls()[before..]
                .iter()
                .any(|(method, path, _, _)| method == "POST" && path.ends_with("/capture"))
        );
    }
}
