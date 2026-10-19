#![allow(clippy::unwrap_used)]
use crate::{
    client::{
        stripe::StripeClient,
        tests::{Fixture, Reply},
    },
    config::StripeConfig,
};
use serde_json::json;
fn client(f: &Fixture) -> StripeClient {
    StripeClient::new(&StripeConfig {
        base_url: f.url.join("/v1/").unwrap().to_string(),
        timeout_secs: 1,
        secret_key: Some("sk_test_private".into()),
    })
    .unwrap()
}
#[tokio::test]
async fn lists_payout_pages_with_inclusive_filters_and_cursor() {
    let f = Fixture::new(vec![
        Reply::json(json!({"data":[{"id":"po_one","arrival_date":42}],"has_more":true})),
        Reply::json(json!({"data":[],"has_more":false})),
    ])
    .await;
    let c = client(&f);
    let page = c.list_payouts(1, 99, None, 100, None).await.unwrap();
    assert!(page.has_more);
    assert_eq!(page.data[0].arrival_date, 42);
    assert!(
        !c.list_payouts(1, 99, Some("paid"), 100, Some("po_one"))
            .await
            .unwrap()
            .has_more
    );
    let requests = f.requests();
    for r in &requests {
        let url = reqwest::Url::parse(&format!("http://fixture{}", r.0)).unwrap();
        assert_eq!(url.path(), "/v1/payouts");
        let q: std::collections::HashMap<_, _> = url.query_pairs().collect();
        assert_eq!(q["arrival_date[gte]"], "1");
        assert_eq!(q["arrival_date[lte]"], "99");
        assert_eq!(q["status"], "paid");
        assert_eq!(q["limit"], "100");
        assert_eq!(r.1["authorization"], "Bearer sk_test_private");
    }
    assert!(requests[1].0.contains("starting_after=po_one"));
    assert!(f.methods().iter().all(|m| m == reqwest::Method::GET));
}
#[tokio::test]
async fn retrieves_balance_transactions_charges_and_refunds_without_money_conversion() {
    let f=Fixture::new(vec![Reply::json(json!({"data":[{"id":"txn_one","amount":-500,"available_on":1,"created":2,"currency":"jpy","fee":0,"fee_details":[],"net":-500,"object":"balance_transaction","reporting_category":"refund","source":null,"status":"available","type":"future_type","exchange_rate":null,"description":null}],"has_more":false})),Reply::json(json!({"id":"ch_one","amount_captured":500,"amount_refunded":0,"captured":true,"created":3,"currency":"jpy","disputed":false,"metadata":{},"paid":true,"payment_intent":null,"refunded":false,"balance_transaction":null})),Reply::json(json!({"id":"re_one","amount":500,"created":4,"currency":"jpy","payment_intent":null,"reason":null}))]).await;
    let c = client(&f);
    let p = c
        .list_balance_transactions("po_one", 100, Some("txn_previous"))
        .await
        .unwrap();
    assert_eq!(p.data[0].amount, -500);
    assert_eq!(p.data[0].r#type, "future_type");
    assert!(p.data[0].source.is_none());
    assert_eq!(c.get_charge("ch_one").await.unwrap().amount_captured, 500);
    assert_eq!(c.get_refund("re_one").await.unwrap().created, 4);
    let r = f.requests();
    assert!(r[0].0.starts_with("/v1/balance_transactions?"));
    assert!(r[0].0.contains("payout=po_one"));
    assert!(r[0].0.contains("starting_after=txn_previous"));
    assert_eq!(r[1].0, "/v1/charges/ch_one");
    assert_eq!(r[2].0, "/v1/refunds/re_one");
}
#[tokio::test]
async fn rejects_invalid_inputs_before_network_access() {
    let f = Fixture::new(vec![]).await;
    let c = client(&f);
    assert!(c.list_payouts(2, 1, None, 100, None).await.is_err());
    for limit in [0, 101] {
        assert!(c.list_payouts(1, 2, None, limit, None).await.is_err());
        assert!(
            c.list_balance_transactions("po_one", limit, None)
                .await
                .is_err()
        );
    }
    for id in ["", " ", "../secret", "ch_one?x=1", "ch_one/other"] {
        assert!(c.get_charge(id).await.is_err());
        assert!(c.get_refund(id).await.is_err());
        assert!(c.list_balance_transactions(id, 1, None).await.is_err());
    }
    assert!(c.list_payouts(1, 2, None, 1, Some("")).await.is_err());
    assert!(
        c.list_payouts(1, 2, Some("invalid"), 1, None)
            .await
            .is_err()
    );
    assert_eq!(f.requests().len(), 0);
}
#[tokio::test]
async fn failures_preserve_types_without_exposing_payloads() {
    use crate::client::ClientErrorKind;
    for status in [401, 404, 429, 500] {
        let f = Fixture::new(
            (0..4)
                .map(|_| Reply {
                    status,
                    body: "private Stripe payload".into(),
                    headers: vec![("retry-after".into(), "0".into())],
                })
                .collect(),
        )
        .await;
        let e = client(&f).get_charge("ch_one").await.unwrap_err();
        assert!(!format!("{e:?} {e}").contains("private"));
        assert!(matches!(
            e.kind(),
            ClientErrorKind::Http { .. } | ClientErrorKind::RetryExhausted
        ));
        assert_eq!(f.requests().len(), if status < 500 { 4 } else { 1 });
    }
    for body in ["private invalid json", "{}"] {
        let f = Fixture::new(vec![Reply {
            status: 200,
            body: body.into(),
            headers: vec![],
        }])
        .await;
        let e = client(&f).get_refund("re_one").await.unwrap_err();
        assert!(matches!(e.kind(), ClientErrorKind::Decode { .. }));
        assert!(std::error::Error::source(&e).is_some());
    }
}
#[test]
fn validates_configuration_and_redacts_secret() {
    let mut cfg = StripeConfig {
        base_url: "https://api.stripe.com/v1/".into(),
        timeout_secs: 3,
        secret_key: Some("sk_test_private".into()),
    };
    assert!(!format!("{cfg:?}").contains("sk_test_private"));
    assert!(StripeClient::new(&cfg).is_ok());
    cfg.secret_key = None;
    assert!(StripeClient::new(&cfg).is_err());
    cfg.secret_key = Some(" ".into());
    assert!(StripeClient::new(&cfg).is_err());
    cfg.secret_key = Some("key".into());
    cfg.timeout_secs = 0;
    assert!(StripeClient::new(&cfg).is_err());
    cfg.timeout_secs = 1;
    cfg.base_url = "invalid".into();
    assert!(StripeClient::new(&cfg).is_err());
    for url in [
        "ftp://example.com/",
        "https://user:pass@example.com/",
        "https://example.com/?q=x",
    ] {
        cfg.base_url = url.into();
        assert!(StripeClient::new(&cfg).is_ok());
    }
}

#[tokio::test]
async fn stalled_stripe_connection_times_out() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let c = StripeClient::new(&StripeConfig {
        base_url: format!("http://{}/v1/", listener.local_addr().unwrap()),
        timeout_secs: 1,
        secret_key: Some("fixture".into()),
    })
    .unwrap();
    let error = c.get_charge("ch_one").await.unwrap_err();
    assert!(
        matches!(error.kind(),crate::client::ClientErrorKind::Transport {source} if source.is_timeout())
    );
}

#[tokio::test]
async fn decodes_populated_nested_fields_and_explicit_payout_status() {
    let f=Fixture::new(vec![
        Reply::json(json!({"data":[],"has_more":false})),
        Reply::json(json!({"data":[{"id":"txn_full","amount":1234,"available_on":1,"created":2,"currency":"thb","fee":34,"fee_details":[{"amount":34,"currency":"thb","description":"Processing","type":"stripe_fee","application":"ca_example"}],"net":1200,"object":"balance_transaction","reporting_category":"charge","source":"ch_full","status":"available","type":"charge","exchange_rate":1.5,"description":"Sale"}],"has_more":false})),
        Reply::json(json!({"id":"ch_full","amount_captured":1234,"amount_refunded":234,"captured":true,"created":3,"currency":"thb","disputed":true,"metadata":{"email":"a@example.com","customer_email":"b@example.com","order_id":"order_one"},"paid":true,"payment_intent":"pi_full","refunded":false,"balance_transaction":"txn_full"})),
        Reply::json(json!({"id":"re_full","amount":234,"created":4,"currency":"thb","payment_intent":"pi_full","reason":"requested_by_customer"}))
    ]).await;
    let c = client(&f);
    c.list_payouts(1, 99, Some("failed"), 1, None)
        .await
        .unwrap();
    assert!(f.requests()[0].0.contains("status=failed"));
    let txn = c
        .list_balance_transactions("po_full", 1, None)
        .await
        .unwrap()
        .data
        .pop()
        .unwrap();
    assert_eq!(txn.source.as_deref(), Some("ch_full"));
    assert_eq!(txn.exchange_rate, Some(1.5));
    assert_eq!(txn.net, 1200);
    assert_eq!(txn.fee_details[0].amount, 34);
    assert_eq!(
        txn.fee_details[0].application.as_deref(),
        Some("ca_example")
    );
    let charge = c.get_charge("ch_full").await.unwrap();
    assert!(charge.payment_status.captured);
    assert!(charge.payment_status.paid);
    assert!(!charge.payment_status.refunded);
    assert!(charge.disputed);
    assert_eq!(charge.metadata.order_id.as_deref(), Some("order_one"));
    assert_eq!(charge.metadata.email.as_deref(), Some("a@example.com"));
    assert_eq!(
        charge.metadata.customer_email.as_deref(),
        Some("b@example.com")
    );
    assert_eq!(charge.payment_intent.as_deref(), Some("pi_full"));
    assert_eq!(charge.balance_transaction.as_deref(), Some("txn_full"));
    let refund = c.get_refund("re_full").await.unwrap();
    assert_eq!(refund.payment_intent.as_deref(), Some("pi_full"));
    assert_eq!(refund.reason.as_deref(), Some("requested_by_customer"));
}

#[tokio::test]
async fn request_paths_preserve_base_with_or_without_trailing_slash() {
    for path in ["/v1", "/v1/"] {
        let fixture = Fixture::new(vec![Reply::json(json!({"data":[],"has_more":false})), Reply::json(json!({"id":"re_one","amount":1,"created":1,"currency":"usd","payment_intent":null,"reason":null}))]).await;
        let c = StripeClient::new(&StripeConfig {
            base_url: fixture.url.join(path).unwrap().to_string(),
            timeout_secs: 1,
            secret_key: Some("fixture".into()),
        })
        .unwrap();
        c.list_payouts(1, 2, None, 1, None).await.unwrap();
        c.get_refund("re_one").await.unwrap();
        let requests = fixture.requests();
        assert!(requests[0].0.starts_with("/v1/payouts?"));
        assert_eq!(requests[1].0, "/v1/refunds/re_one");
    }
}
