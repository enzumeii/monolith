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

fn config(f: &Fixture) -> PaypalConfig {
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
fn token(value: &str) -> Reply {
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
async fn invalid_inputs_do_not_contact_provider() {
    let f = Fixture::new(vec![]).await;
    let client = PaypalClient::new(&config(&f)).unwrap();
    let (start, end) = dates();
    for (a, b, size, page) in [
        (start, end, 0, 1),
        (start, end, 501, 1),
        (start, end, 1, 0),
        (start, end, 1, 2_147_483_648),
        (end, start, 500, 1),
        (start, start + chrono::Duration::days(32), 500, 1),
    ] {
        assert!(matches!(
            client
                .get_transactions(a, b, size, page)
                .await
                .unwrap_err()
                .kind(),
            ClientErrorKind::InvalidInput(_)
        ));
    }
    assert_eq!(f.requests(), []);
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
async fn malformed_reports_and_wrong_page_metadata_are_rejected() {
    for body in [
        json!({}),
        json!({"transaction_details":[],"page":2,"total_items":1,"total_pages":1}),
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
async fn construction_rejects_invalid_settings() {
    let f = Fixture::new(vec![]).await;
    for url in [
        "invalid",
        "http://example.com/",
        "https://user:secret@example.com/",
        "https://example.com/?token=private",
        "https://example.com/#private",
    ] {
        let mut cfg = config(&f);
        cfg.base_url = url.into();
        assert!(PaypalClient::new(&cfg).is_err());
    }
    for key in [None, Some(""), Some(" ")] {
        let mut cfg = config(&f);
        cfg.client_id = key.map(str::to_owned);
        assert!(PaypalClient::new(&cfg).is_err());
        let mut cfg = config(&f);
        cfg.client_secret = key.map(str::to_owned);
        assert!(PaypalClient::new(&cfg).is_err());
    }
    let mut cfg = config(&f);
    cfg.timeout_secs = 0;
    assert!(PaypalClient::new(&cfg).is_err());
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
