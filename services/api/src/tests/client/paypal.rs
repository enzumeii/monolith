#![allow(clippy::unwrap_used)]
use crate::{
    client::{
        ClientError, ClientErrorKind,
        paypal::PaypalClient,
        tests::{Fixture, Reply},
    },
    config::PaypalConfig,
};
use chrono::{DateTime, Utc};
use serde_json::json;
use std::{sync::Arc, time::Duration};

pub(super) fn config(f: &Fixture) -> PaypalConfig {
    PaypalConfig {
        base_url: f.url.join("/").unwrap().to_string(),
        timeout_secs: 1,
        client_id: Some("fixture-id".into()),
        client_secret: Some("fixture-secret".into()),
    }
}
fn dates() -> (DateTime<Utc>, DateTime<Utc>) {
    (
        "2026-10-01T00:00:00Z".parse().unwrap(),
        "2026-10-02T00:00:00Z".parse().unwrap(),
    )
}
pub(super) fn token(value: &str) -> Reply {
    Reply::json(json!({"access_token":value,"token_type":"Bearer","expires_in":3600}))
}
fn page() -> Reply {
    Reply::json(json!({"transaction_details":[],"page":1,"total_items":0,"total_pages":0}))
}
async fn report(
    client: &PaypalClient,
) -> Result<crate::models::paypal::TransactionPage, ClientError> {
    let (start, end) = dates();
    client.get_transactions(start, end, 500, 1).await
}
fn safe(error: &ClientError) {
    let rendered = format!("{error} {error:?}");
    for private in ["fixture-secret", "fixture-token", "private payer"] {
        assert!(!rendered.contains(private));
    }
}

#[tokio::test]
async fn authenticates_with_form_and_basic_auth_then_reuses_bearer_token() {
    let f = Fixture::new(vec![token("fixture-token"), page(), page()]).await;
    let client = PaypalClient::new(&config(&f)).unwrap();
    assert_eq!(f.requests(), []);
    assert_eq!(report(&client).await.unwrap().total_items, 0);
    report(&client).await.unwrap();
    let requests = f.requests();
    assert_eq!(requests.len(), 3);
    assert_eq!(
        f.methods(),
        [
            reqwest::Method::POST,
            reqwest::Method::GET,
            reqwest::Method::GET
        ]
    );
    assert_eq!(requests[0].0, "/v1/oauth2/token");
    assert_eq!(
        requests[0].1["authorization"],
        "Basic Zml4dHVyZS1pZDpmaXh0dXJlLXNlY3JldA=="
    );
    assert_eq!(
        requests[0].1["content-type"],
        "application/x-www-form-urlencoded"
    );
    assert_eq!(f.raw_bodies()[0], b"grant_type=client_credentials");
    assert_eq!(requests[1].1["authorization"], "Bearer fixture-token");
    let url = f.url.join(&requests[1].0).unwrap();
    let query: std::collections::BTreeMap<_, _> = url.query_pairs().into_owned().collect();
    assert_eq!(url.path(), "/v1/reporting/transactions");
    assert_eq!(query["start_date"], "2026-10-01T00:00:00Z");
    assert_eq!(query["end_date"], "2026-10-02T00:00:00Z");
    assert_eq!(query["fields"], "all");
    assert_eq!(query["page_size"], "500");
    assert_eq!(query["page"], "1");
}
#[tokio::test]
async fn concurrent_requests_share_initial_authentication() {
    let f = Fixture::new(vec![token("fixture-token"), page(), page()]).await;
    let client = Arc::new(PaypalClient::new(&config(&f)).unwrap());
    let (a, b) = tokio::join!(report(&client), report(&client));
    a.unwrap();
    b.unwrap();
    assert_eq!(
        f.requests()
            .iter()
            .filter(|r| r.0 == "/v1/oauth2/token")
            .count(),
        1
    );
}
#[tokio::test]
async fn expired_token_is_renewed() {
    let f = Fixture::new(vec![
        Reply::json(json!({"access_token":"old","token_type":"Bearer","expires_in":1})),
        page(),
        token("new"),
        page(),
    ])
    .await;
    let client = PaypalClient::new(&config(&f)).unwrap();
    report(&client).await.unwrap();
    tokio::time::sleep(Duration::from_millis(1100)).await;
    report(&client).await.unwrap();
    assert_eq!(f.requests()[3].1["authorization"], "Bearer new");
}
#[tokio::test]
async fn unauthorized_report_renews_once_and_second_unauthorized_is_terminal() {
    for status in [200, 401] {
        let reply = if status == 200 {
            page()
        } else {
            Reply::status(status)
        };
        let f = Fixture::new(vec![token("old"), Reply::status(401), token("new"), reply]).await;
        let client = PaypalClient::new(&config(&f)).unwrap();
        let result = report(&client).await;
        if status == 200 {
            result.unwrap();
        } else {
            assert!(matches!(
                result.unwrap_err().kind(),
                ClientErrorKind::Http { status: 401, .. }
            ));
        }
        assert_eq!(f.requests().len(), 4);
        assert_eq!(f.requests()[3].1["authorization"], "Bearer new");
    }
}
#[tokio::test]
async fn rejected_oauth_and_report_requests_are_not_retried() {
    for status in [400, 401, 403, 500] {
        let f = Fixture::new(vec![Reply::status(status)]).await;
        let error = report(&PaypalClient::new(&config(&f)).unwrap())
            .await
            .unwrap_err();
        assert!(matches!(error.kind(),ClientErrorKind::Http{status:s,..} if *s==status));
        safe(&error);
        assert_eq!(f.requests().len(), 1);
    }
    let f = Fixture::new(vec![token("old"), Reply::status(401), Reply::status(403)]).await;
    assert!(
        report(&PaypalClient::new(&config(&f)).unwrap())
            .await
            .is_err()
    );
    assert_eq!(f.requests().len(), 3);
    for status in [400, 403, 500] {
        let f = Fixture::new(vec![token("fixture-token"), Reply::status(status)]).await;
        let error = report(&PaypalClient::new(&config(&f)).unwrap())
            .await
            .unwrap_err();
        safe(&error);
        assert_eq!(f.requests().len(), 2);
    }
}
#[tokio::test]
async fn rate_limit_uses_shared_bounded_retry_budget() {
    let mut limited = Reply::status(429);
    limited.headers.push(("retry-after".into(), "0".into()));
    let f = Fixture::new(vec![token("fixture-token"), limited.clone(), page()]).await;
    report(&PaypalClient::new(&config(&f)).unwrap())
        .await
        .unwrap();
    assert_eq!(f.requests().len(), 3);
    let f = Fixture::new(vec![
        token("fixture-token"),
        limited.clone(),
        limited.clone(),
        limited.clone(),
        limited,
    ])
    .await;
    let error = report(&PaypalClient::new(&config(&f)).unwrap())
        .await
        .unwrap_err();
    assert!(matches!(error.kind(), ClientErrorKind::RetryExhausted));
    assert_eq!(f.requests().len(), 5);
}
#[tokio::test]
async fn transaction_inputs_are_forwarded_and_provider_rejections_propagate() {
    let (start, end) = dates();
    for (a, b, size, page_number) in [
        (start, end, 0, 1),
        (start, end, 501, 1),
        (start, end, 1, 0),
        (start, end, 1, 2_147_483_648),
        (end, start, 500, 1),
        (start, start + chrono::Duration::days(32), 500, 1),
    ] {
        let f = Fixture::new(vec![token("fixture-token"), Reply::status(400)]).await;
        let client = PaypalClient::new(&config(&f)).unwrap();
        let error = client
            .get_transactions(a, b, size, page_number)
            .await
            .unwrap_err();
        assert!(matches!(
            error.kind(),
            ClientErrorKind::Http { status: 400, .. }
        ));
        let requests = f.requests();
        assert_eq!(requests.len(), 2);
        let url = f.url.join(&requests[1].0).unwrap();
        let query: std::collections::BTreeMap<_, _> = url.query_pairs().into_owned().collect();
        assert_eq!(query["page_size"], size.to_string());
        assert_eq!(query["page"], page_number.to_string());
        if a == end {
            assert_eq!(query["start_date"], "2026-10-02T00:00:00Z");
        }
        if b == start {
            assert_eq!(query["end_date"], "2026-10-01T00:00:00Z");
        }
        if b > end {
            assert_eq!(query["end_date"], "2026-11-02T00:00:00Z");
        }
        safe(&error);
    }
}

#[tokio::test]
async fn decoded_page_metadata_is_returned_without_local_consistency_checks() {
    for body in [
        json!({"transaction_details":[],"page":2,"total_items":1,"total_pages":1}),
        json!({"transaction_details":[],"page":1,"total_items":10,"total_pages":0}),
        json!({"transaction_details":[],"page":1,"total_items":0,"total_pages":2}),
    ] {
        let f = Fixture::new(vec![token("fixture-token"), Reply::json(body.clone())]).await;
        let response = report(&PaypalClient::new(&config(&f)).unwrap())
            .await
            .unwrap();
        assert_eq!(u64::from(response.page), body["page"].as_u64().unwrap());
        assert_eq!(response.total_items, body["total_items"].as_u64().unwrap());
        assert_eq!(
            u64::from(response.total_pages),
            body["total_pages"].as_u64().unwrap()
        );
    }
}

#[tokio::test]
async fn unusable_token_responses_are_errors_and_never_leak_payloads() {
    for body in [
        json!({}),
        json!({"access_token":"","token_type":"Bearer","expires_in":3600}),
        json!({"access_token":"fixture-token","token_type":"MAC","expires_in":3600}),
        json!({"access_token":"fixture-token","token_type":"Bearer","expires_in":0}),
        json!({"access_token":"fixture-token","token_type":"Bearer","expires_in":u64::MAX}),
    ] {
        let f = Fixture::new(vec![Reply::json(body)]).await;
        let error = report(&PaypalClient::new(&config(&f)).unwrap())
            .await
            .unwrap_err();
        safe(&error);
        assert_eq!(f.requests().len(), 1);
    }
}
#[tokio::test]
async fn malformed_reports_are_rejected_by_typed_decoding() {
    for body in [
        json!({}),
        json!({"transaction_details":[{"transaction_info":{}}],"page":1,"total_items":1,"total_pages":1}),
    ] {
        let f = Fixture::new(vec![token("fixture-token"), Reply::json(body)]).await;
        let error = report(&PaypalClient::new(&config(&f)).unwrap())
            .await
            .unwrap_err();
        safe(&error);
    }
    let f = Fixture::new(vec![
        token("fixture-token"),
        Reply {
            status: 200,
            body: "not json private payer".into(),
            headers: vec![],
        },
    ])
    .await;
    let error = report(&PaypalClient::new(&config(&f)).unwrap())
        .await
        .unwrap_err();
    assert!(matches!(error.kind(), ClientErrorKind::Decode { .. }));
    assert!(std::error::Error::source(&error).is_some());
    safe(&error);
}
#[tokio::test]
async fn connection_failures_retain_transport_cause() {
    let f = Fixture::new(vec![]).await;
    let config = config(&f);
    drop(f);
    let error = report(&PaypalClient::new(&config).unwrap())
        .await
        .unwrap_err();
    assert!(matches!(error.kind(), ClientErrorKind::Transport { .. }));
    assert!(std::error::Error::source(&error).is_some());
    safe(&error);
}
#[tokio::test]
async fn timeout_is_a_transport_error() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let f = Fixture::new(vec![]).await;
    let mut config = config(&f);
    config.base_url = format!("http://{}/", listener.local_addr().unwrap());
    let error = report(&PaypalClient::new(&config).unwrap())
        .await
        .unwrap_err();
    assert!(matches!(error.kind(),ClientErrorKind::Transport{source} if source.is_timeout()));
    safe(&error);
}
#[tokio::test]
async fn construction_keeps_only_required_parsing_presence_and_transport_failures() {
    let f = Fixture::new(vec![]).await;
    let mut cfg = config(&f);
    cfg.base_url = "invalid".into();
    assert!(PaypalClient::new(&cfg).is_err());
    let mut cfg = config(&f);
    cfg.client_id = None;
    assert!(PaypalClient::new(&cfg).is_err());
    let mut cfg = config(&f);
    cfg.client_secret = None;
    assert!(PaypalClient::new(&cfg).is_err());
    let mut cfg = config(&f);
    cfg.timeout_secs = 0;
    assert!(PaypalClient::new(&cfg).is_err());
}

#[tokio::test]
async fn construction_trusts_syntactically_valid_url_configuration() {
    let f = Fixture::new(vec![]).await;
    for url in [
        "http://example.test/",
        "https://user:secret@example.test/",
        "https://example.test/custom/",
        "https://example.test/?configuration=value",
        "https://example.test/#configured-fragment",
    ] {
        let mut cfg = config(&f);
        cfg.base_url = url.into();
        assert!(PaypalClient::new(&cfg).is_ok());
    }
    assert_eq!(f.requests(), []);
}

#[tokio::test]
async fn configured_credential_contents_are_forwarded_to_basic_auth() {
    for (id, secret, expected) in [
        ("", "", "Basic Og=="),
        (" ", " ", "Basic IDog"),
        (
            "fixture:id",
            "fixture-secret",
            "Basic Zml4dHVyZTppZDpmaXh0dXJlLXNlY3JldA==",
        ),
    ] {
        let f = Fixture::new(vec![Reply::status(401)]).await;
        let mut cfg = config(&f);
        cfg.client_id = Some(id.into());
        cfg.client_secret = Some(secret.into());
        let error = report(&PaypalClient::new(&cfg).unwrap()).await.unwrap_err();
        assert!(matches!(
            error.kind(),
            ClientErrorKind::Http { status: 401, .. }
        ));
        let requests = f.requests();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].1["authorization"], expected);
    }
}

#[tokio::test]
async fn forwards_requested_page_and_returns_completeness_metadata() {
    let f = Fixture::new(vec![token("fixture-token"), Reply::json(json!({
        "transaction_details":[{"transaction_info":{
            "transaction_id":"fixture-id", "transaction_event_code":"T0006", "transaction_status":"S",
            "transaction_amount":{"currency_code":"THB","value":"12.50"},
            "transaction_initiation_date":"2026-10-01T00:00:00Z",
            "transaction_updated_date":"2026-10-01T00:00:00Z"
        }}], "page":2, "total_items":2, "total_pages":2
    }))]).await;
    let client = PaypalClient::new(&config(&f)).unwrap();
    let (start, _) = dates();
    let response = client
        .get_transactions(start, start + chrono::Duration::days(31), 1, 2)
        .await
        .unwrap();
    assert_eq!(response.page, 2);
    assert_eq!(response.total_items, 2);
    assert_eq!(response.total_pages, 2);
    assert_eq!(response.transaction_details.len(), 1);
    assert_eq!(
        response.transaction_details[0]
            .transaction
            .transaction_amount
            .value,
        "12.50"
    );
    let requests = f.requests();
    let url = f.url.join(&requests[1].0).unwrap();
    let query: std::collections::BTreeMap<_, _> = url.query_pairs().into_owned().collect();
    assert_eq!(query["page"], "2");
    assert_eq!(query["page_size"], "1");
    assert_eq!(query["end_date"], "2026-11-01T00:00:00Z");
    assert_eq!(requests.len(), 2);
}

fn order_body() -> serde_json::Value {
    json!({"id":"ORDER123", "status":"COMPLETED", "purchase_units":[{
        "reference_id":"payment-uuid", "custom_id":"draft-correlation",
        "amount":{"currency_code":"USD","value":"12.50"},
        "payee":{"merchant_id":"MERCHANT123"},
        "payments":{"captures":[capture_body()]}
    }], "links":[{"href":"https://untrusted.example/never-follow", "rel":"self", "method":"GET"}]})
}

fn capture_body() -> serde_json::Value {
    json!({"id":"CAPTURE123", "status":"COMPLETED", "amount":{"currency_code":"USD","value":"12.50"},
        "final_capture":true, "payee":{"merchant_id":"MERCHANT123"},
        "supplementary_data":{"related_ids":{"order_id":"ORDER123"}}})
}

fn create_payload() -> crate::models::paypal::CreateOrderRequest {
    serde_json::from_value(json!({"intent":"CAPTURE", "purchase_units":[{
        "reference_id":"payment-uuid", "custom_id":"draft-correlation",
        "amount":{"currency_code":"USD","value":"12.50"}, "payee":{"merchant_id":"MERCHANT123"}
    }], "payment_source":{"paypal":{"experience_context":{
        "return_url":"https://merchant.example/return", "cancel_url":"https://merchant.example/cancel", "shipping_preference":"NO_SHIPPING"
    }}}})).unwrap()
}

fn verification_payload() -> crate::models::paypal::VerifyWebhookSignatureRequest {
    serde_json::from_value(json!({
        "auth_algo":"SHA256withRSA", "cert_url":"https://untrusted.example/never-follow",
        "transmission_id":"tx-123", "transmission_sig":"synthetic-signature",
        "transmission_time":"2026-10-23T00:00:00Z", "webhook_id":"WH123",
        "webhook_event":{"id":"EVENT123","event_type":"PAYMENT.CAPTURE.COMPLETED", "resource":capture_body()}
    })).unwrap()
}

#[tokio::test]
async fn payment_operations_forward_exact_payloads_keys_and_trusted_paths() {
    use crate::models::paypal::{CaptureStatus, OrderStatus, WebhookVerificationStatus};
    let f = Fixture::new(vec![
        token("fixture-token"),
        Reply::json(order_body()),
        Reply::json(order_body()),
        Reply::json(order_body()),
        Reply::json(capture_body()),
        Reply::json(json!({"verification_status":"SUCCESS"})),
    ])
    .await;
    let client = PaypalClient::new(&config(&f)).unwrap();
    let payload = create_payload();
    assert_eq!(
        client
            .create_order("create-stable-id", &payload)
            .await
            .unwrap()
            .status,
        OrderStatus::Completed
    );
    let order = client.get_order("ORDER123").await.unwrap();
    assert_eq!(order.purchase_units[0].amount.value, "12.50");
    assert_eq!(
        order.purchase_units[0].payments.as_ref().unwrap().captures[0].id,
        "CAPTURE123"
    );
    client
        .capture_order("ORDER123", "capture-stable-id")
        .await
        .unwrap();
    let capture = client.get_capture("CAPTURE123").await.unwrap();
    assert_eq!(capture.status, CaptureStatus::Completed);
    assert_eq!(
        capture
            .supplementary_data
            .unwrap()
            .related_ids
            .order_id
            .as_deref(),
        Some("ORDER123")
    );
    assert_eq!(
        client
            .verify_webhook_signature(&verification_payload())
            .await
            .unwrap()
            .verification_status,
        WebhookVerificationStatus::Success
    );
    let requests = f.requests();
    assert_eq!(
        requests
            .iter()
            .skip(1)
            .map(|r| r.0.as_str())
            .collect::<Vec<_>>(),
        [
            "/v2/checkout/orders",
            "/v2/checkout/orders/ORDER123",
            "/v2/checkout/orders/ORDER123/capture",
            "/v2/payments/captures/CAPTURE123",
            "/v1/notifications/verify-webhook-signature"
        ]
    );
    assert_eq!(requests[1].1["paypal-request-id"], "create-stable-id");
    assert_eq!(requests[3].1["paypal-request-id"], "capture-stable-id");
    assert_eq!(requests[1].1["prefer"], "return=representation");
    assert_eq!(requests[3].1["prefer"], "return=representation");
    assert_eq!(requests[1].2, serde_json::to_value(payload).unwrap());
    assert_eq!(requests[3].2, json!({}));
    assert_eq!(
        requests[5].2,
        serde_json::to_value(verification_payload()).unwrap()
    );
    for request in &requests[1..] {
        assert_eq!(request.1["authorization"], "Bearer fixture-token");
    }
    assert_eq!(
        f.methods(),
        [
            reqwest::Method::POST,
            reqwest::Method::POST,
            reqwest::Method::GET,
            reqwest::Method::POST,
            reqwest::Method::GET,
            reqwest::Method::POST
        ]
    );
}

#[tokio::test]
async fn payment_auth_refresh_and_rate_limit_preserve_operation_and_idempotency() {
    let mut limited = Reply::status(429);
    limited.headers.push(("retry-after".into(), "0".into()));
    let f = Fixture::new(vec![
        token("old"),
        Reply::status(401),
        token("new"),
        limited,
        Reply::json(order_body()),
    ])
    .await;
    let client = PaypalClient::new(&config(&f)).unwrap();
    client
        .create_order("stable-id", &create_payload())
        .await
        .unwrap();
    let requests = f.requests();
    assert_eq!(requests.len(), 5);
    for i in [1, 3, 4] {
        assert_eq!(requests[i].1["paypal-request-id"], "stable-id");
        assert_eq!(
            requests[i].2,
            serde_json::to_value(create_payload()).unwrap()
        );
    }
    assert_eq!(requests[4].1["authorization"], "Bearer new");
}

#[tokio::test]
async fn payment_operations_reject_http_and_malformed_responses_without_leaking_payloads() {
    for operation in 0..5 {
        for reply in [
            Reply::status(422),
            Reply::json(json!({})),
            Reply {
                status: 200,
                body: "private payer not json".into(),
                headers: vec![],
            },
        ] {
            let f = Fixture::new(vec![token("fixture-token"), reply]).await;
            let client = PaypalClient::new(&config(&f)).unwrap();
            let result = match operation {
                0 => client
                    .create_order("stable", &create_payload())
                    .await
                    .map(|_| ()),
                1 => client.get_order("ORDER123").await.map(|_| ()),
                2 => client.capture_order("ORDER123", "stable").await.map(|_| ()),
                3 => client.get_capture("CAPTURE123").await.map(|_| ()),
                _ => client
                    .verify_webhook_signature(&verification_payload())
                    .await
                    .map(|_| ()),
            };
            safe(&result.unwrap_err());
            assert_eq!(f.requests().len(), 2);
        }
    }
}

#[tokio::test]
async fn provider_resource_ids_are_encoded_as_single_path_segments() {
    let f = Fixture::new(vec![
        token("fixture-token"),
        Reply::json(order_body()),
        Reply::json(capture_body()),
    ])
    .await;
    let client = PaypalClient::new(&config(&f)).unwrap();
    client.get_order("ORDER/with?fragment#").await.unwrap();
    client.get_capture("CAPTURE/with?fragment#").await.unwrap();
    assert_eq!(
        f.requests()[1].0,
        "/v2/checkout/orders/ORDER%2Fwith%3Ffragment%23"
    );
    assert_eq!(
        f.requests()[2].0,
        "/v2/payments/captures/CAPTURE%2Fwith%3Ffragment%23"
    );
}

#[test]
fn unknown_payment_protocol_codes_round_trip_and_keep_distinct_identity() {
    use crate::models::paypal::{CaptureStatus, OrderStatus, WebhookVerificationStatus};
    let status: OrderStatus = serde_json::from_value(json!("FUTURE_ORDER_STATE")).unwrap();
    assert_eq!(
        serde_json::to_value(&status).unwrap(),
        json!("FUTURE_ORDER_STATE")
    );
    assert_ne!(status, OrderStatus::Completed);
    let status: CaptureStatus = serde_json::from_value(json!("FUTURE_CAPTURE_STATE")).unwrap();
    assert_eq!(
        serde_json::to_value(&status).unwrap(),
        json!("FUTURE_CAPTURE_STATE")
    );
    assert_ne!(status, CaptureStatus::Completed);
    let status: WebhookVerificationStatus = serde_json::from_value(json!("UNRECOGNIZED")).unwrap();
    assert_eq!(
        serde_json::to_value(&status).unwrap(),
        json!("UNRECOGNIZED")
    );
    assert_ne!(status, WebhookVerificationStatus::Success);
}

#[tokio::test]
async fn payment_operations_keep_transport_causes_for_connection_and_timeout_failures() {
    for operation in 0..5 {
        for timeout in [false, true] {
            let f = Fixture::new(vec![token("fixture-token")]).await;
            let mut client = PaypalClient::new(&config(&f)).unwrap();
            client.access_token(None).await.unwrap();
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            client.base_url =
                reqwest::Url::parse(&format!("http://{}/", listener.local_addr().unwrap()))
                    .unwrap();
            let listener = timeout.then_some(listener);
            let result = match operation {
                0 => client
                    .create_order("stable", &create_payload())
                    .await
                    .map(|_| ()),
                1 => client.get_order("ORDER123").await.map(|_| ()),
                2 => client.capture_order("ORDER123", "stable").await.map(|_| ()),
                3 => client.get_capture("CAPTURE123").await.map(|_| ()),
                _ => client
                    .verify_webhook_signature(&verification_payload())
                    .await
                    .map(|_| ()),
            };
            let error = result.unwrap_err();
            assert!(
                matches!(error.kind(),ClientErrorKind::Transport{source} if source.is_timeout()==timeout)
            );
            assert!(std::error::Error::source(&error).is_some());
            safe(&error);
            drop(listener);
        }
    }
}
