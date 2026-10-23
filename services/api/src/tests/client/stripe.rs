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
    assert_eq!(p.data[0].r#type.as_str(), "future_type");
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

fn checkout_request() -> crate::models::stripe::CreateCheckoutSession {
    crate::models::stripe::CreateCheckoutSession {
        amount: 1200,
        currency: "THB".into(),
        product_name: "Test purchase".into(),
        success_url: "https://example.com/success?session_id={CHECKOUT_SESSION_ID}".into(),
        cancel_url: "https://example.com/cancel".into(),
        idempotency_key: "checkout_fixture_1".into(),
        metadata: [("reference".into(), "fixture_1".into())].into(),
    }
}
fn session_response() -> serde_json::Value {
    json!({"id":"cs_test_one","object":"checkout.session","mode":"payment","status":"open","payment_status":"unpaid","livemode":false,"amount_total":1200,"currency":"thb","metadata":{"reference":"fixture_1"},"payment_intent":null,"url":"https://checkout.stripe.com/test"})
}
#[tokio::test]
async fn creates_card_checkout_with_exact_form_money_and_stable_key() {
    let fixture = Fixture::new(vec![Reply::json(session_response())]).await;
    let result = client(&fixture)
        .create_checkout_session(&checkout_request())
        .await
        .unwrap();
    assert_eq!(result.id, "cs_test_one");
    assert_eq!(
        result.url.as_deref(),
        Some("https://checkout.stripe.com/test")
    );
    let requests = fixture.requests();
    assert_eq!(requests[0].0, "/v1/checkout/sessions");
    assert_eq!(fixture.methods()[0], reqwest::Method::POST);
    assert_eq!(requests[0].1["idempotency-key"], "checkout_fixture_1");
    assert_eq!(
        requests[0].1["content-type"],
        "application/x-www-form-urlencoded"
    );
    let raw = fixture.raw_bodies();
    let form: std::collections::BTreeMap<_, _> = reqwest::Url::parse(&format!(
        "https://fixture/?{}",
        String::from_utf8_lossy(&raw[0])
    ))
    .unwrap()
    .query_pairs()
    .map(|(k, v)| (k.into_owned(), v.into_owned()))
    .collect();
    assert_eq!(form["mode"], "payment");
    assert_eq!(form["payment_method_types[0]"], "card");
    assert_eq!(form["line_items[0][price_data][unit_amount]"], "1200");
    assert_eq!(form["line_items[0][price_data][currency]"], "thb");
    assert_eq!(form["line_items[0][quantity]"], "1");
    assert_eq!(form["metadata[reference]"], "fixture_1");
    assert_eq!(
        form["payment_intent_data[metadata][reference]"],
        "fixture_1"
    );
    assert_eq!(form["adaptive_pricing[enabled]"], "false");
    assert_eq!(form["success_url"], checkout_request().success_url);
}
#[tokio::test]
async fn checkout_mutations_never_automatically_replay_rejections() {
    for status in [400, 401, 402, 404, 409, 429, 500] {
        let fixture =
            Fixture::new(vec![Reply::status(status), Reply::json(session_response())]).await;
        let error = client(&fixture)
            .create_checkout_session(&checkout_request())
            .await
            .unwrap_err();
        assert!(
            matches!(error.kind(), crate::client::ClientErrorKind::Http {status: actual,..} if *actual == status)
        );
        assert_eq!(fixture.requests().len(), 1);
        let fixture =
            Fixture::new(vec![Reply::status(status), Reply::json(session_response())]).await;
        assert!(
            client(&fixture)
                .expire_checkout_session("cs_test_one", "expire_fixture")
                .await
                .is_err()
        );
        assert_eq!(fixture.requests().len(), 1);
    }
}
#[tokio::test]
async fn retrieves_sessions_intents_account_and_expires_by_real_paths() {
    let fixture = Fixture::new(vec![Reply::json(session_response()), Reply::json(json!({"id":"pi_one","object":"payment_intent","status":"succeeded","amount":1200,"amount_received":1200,"currency":"thb","livemode":false,"metadata":{},"latest_charge":"ch_one"})), Reply::json(json!({"id":"acct_one","object":"account"})), Reply::json(session_response())]).await;
    let c = client(&fixture);
    c.get_checkout_session("cs_test_one").await.unwrap();
    assert_eq!(
        c.get_payment_intent("pi_one")
            .await
            .unwrap()
            .latest_charge
            .unwrap()
            .id(),
        "ch_one"
    );
    assert_eq!(c.get_account().await.unwrap().id, "acct_one");
    c.expire_checkout_session("cs_test_one", "expire_fixture")
        .await
        .unwrap();
    let requests = fixture.requests();
    assert_eq!(
        requests.iter().map(|r| r.0.as_str()).collect::<Vec<_>>(),
        [
            "/v1/checkout/sessions/cs_test_one",
            "/v1/payment_intents/pi_one",
            "/v1/account",
            "/v1/checkout/sessions/cs_test_one/expire"
        ]
    );
    assert_eq!(
        fixture.methods(),
        [
            reqwest::Method::GET,
            reqwest::Method::GET,
            reqwest::Method::GET,
            reqwest::Method::POST
        ]
    );
    assert_eq!(requests[3].1["idempotency-key"], "expire_fixture");
}
#[tokio::test]
async fn rejects_invalid_checkout_inputs_before_sending() {
    let fixture = Fixture::new(vec![]).await;
    let c = client(&fixture);
    for amount in [0, -1, 100_000_000] {
        let mut r = checkout_request();
        r.amount = amount;
        assert!(c.create_checkout_session(&r).await.is_err());
    }
    for currency in ["", "usd!", "TH", "123"] {
        let mut r = checkout_request();
        r.currency = currency.into();
        assert!(c.create_checkout_session(&r).await.is_err());
    }
    for url in [
        "javascript:alert(1)",
        "http://example.com/",
        "https://user:pass@example.com/",
        "invalid",
    ] {
        let mut r = checkout_request();
        r.success_url = url.into();
        assert!(c.create_checkout_session(&r).await.is_err());
    }
    for key in [String::new(), "x".repeat(256), "key\ninvalid".into()] {
        let mut r = checkout_request();
        r.idempotency_key = key;
        assert!(c.create_checkout_session(&r).await.is_err());
    }
    let mut r = checkout_request();
    r.metadata.insert("bad[key]".into(), "value".into());
    assert!(c.create_checkout_session(&r).await.is_err());
    for id in ["", "../x", &"x".repeat(256)] {
        assert!(c.get_checkout_session(id).await.is_err());
        assert!(c.get_payment_intent(id).await.is_err());
        assert!(
            c.expire_checkout_session(id, "expire_fixture")
                .await
                .is_err()
        );
    }
    assert_eq!(fixture.requests(), []);
}

#[tokio::test]
async fn checkout_operations_reject_malformed_success_without_replay() {
    for body in ["invalid", "{}", "{\"id\":null}"] {
        for operation in 0..5 {
            let fixture = Fixture::new(vec![
                Reply {
                    status: 200,
                    body: body.into(),
                    headers: vec![],
                },
                Reply::json(session_response()),
            ])
            .await;
            let c = client(&fixture);
            let result = match operation {
                0 => c
                    .create_checkout_session(&checkout_request())
                    .await
                    .map(|_| ()),
                1 => c.get_checkout_session("cs_test_one").await.map(|_| ()),
                2 => c
                    .expire_checkout_session("cs_test_one", "expire_fixture")
                    .await
                    .map(|_| ()),
                3 => c.get_payment_intent("pi_one").await.map(|_| ()),
                _ => c.get_account().await.map(|_| ()),
            };
            assert!(matches!(
                result.unwrap_err().kind(),
                crate::client::ClientErrorKind::Decode { .. }
            ));
            assert_eq!(fixture.requests().len(), 1);
        }
    }
}
#[tokio::test]
async fn checkout_mutation_timeouts_and_connection_failures_are_not_replayed() {
    for expire in [false, true] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let c = StripeClient::new(&StripeConfig {
            base_url: format!("http://{}/v1/", listener.local_addr().unwrap()),
            timeout_secs: 1,
            secret_key: Some("fixture".into()),
        })
        .unwrap();
        let error = if expire {
            c.expire_checkout_session("cs_test_one", "expire_fixture")
                .await
                .unwrap_err()
        } else {
            c.create_checkout_session(&checkout_request())
                .await
                .unwrap_err()
        };
        assert!(
            matches!(error.kind(),crate::client::ClientErrorKind::Transport {source} if source.is_timeout())
        );
        listener.accept().await.unwrap();
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(30), listener.accept())
                .await
                .is_err()
        );
        drop(listener);
        let error = if expire {
            c.expire_checkout_session("cs_test_one", "expire_fixture")
                .await
                .unwrap_err()
        } else {
            c.create_checkout_session(&checkout_request())
                .await
                .unwrap_err()
        };
        assert!(
            matches!(error.kind(),crate::client::ClientErrorKind::Transport {source} if source.is_connect())
        );
    }
}

#[tokio::test]
async fn creation_requires_an_actual_hosted_session_url() {
    for url in [json!(null), json!(""), json!("javascript:alert(1)")] {
        let mut response = session_response();
        response["url"] = url;
        let fixture = Fixture::new(vec![Reply::json(response)]).await;
        assert!(
            client(&fixture)
                .create_checkout_session(&checkout_request())
                .await
                .is_err()
        );
        assert_eq!(fixture.requests().len(), 1);
    }
}

#[tokio::test]
async fn creation_rejects_provider_money_mode_and_correlation_mismatches() {
    for (field, value) in [
        ("amount_total", json!(1199)),
        ("amount_total", json!(null)),
        ("currency", json!("usd")),
        ("mode", json!("subscription")),
        ("metadata", json!({"reference":"another_attempt"})),
        ("object", json!("payment_intent")),
        ("id", json!("")),
    ] {
        let mut response = session_response();
        response[field] = value;
        let fixture = Fixture::new(vec![Reply::json(response)]).await;
        assert!(
            client(&fixture)
                .create_checkout_session(&checkout_request())
                .await
                .is_err(),
            "{field}"
        );
        assert_eq!(fixture.requests().len(), 1);
    }
}

#[tokio::test]
async fn retrieves_checkout_and_payment_intent_and_authenticated_account() {
    use crate::models::stripe::{
        CheckoutMode, CheckoutPaymentStatus, CheckoutStatus, PaymentIntentStatus,
    };
    let f = Fixture::new(vec![
        Reply::json(json!({"id":"cs_test_one","object":"checkout.session","mode":"payment","status":"complete","payment_status":"paid","livemode":false,"amount_total":1234,"currency":"thb","client_reference_id":"checkout-one","metadata":{"attempt_id":"attempt-one"},"payment_intent":"pi_one"})),
        Reply::json(json!({"id":"pi_one","object":"payment_intent","status":"succeeded","amount":1234,"amount_received":1234,"currency":"thb","livemode":false,"latest_charge":"ch_one","metadata":{"checkout_id":"checkout-one"}})),
        Reply::json(json!({"id":"acct_one","object":"account"})),
    ]).await;
    let c = client(&f);
    let session = c.get_checkout_session("cs_test_one").await.unwrap();
    assert_eq!(session.mode, CheckoutMode::Payment);
    assert_eq!(session.status, CheckoutStatus::Complete);
    assert_eq!(session.payment_status, CheckoutPaymentStatus::Paid);
    assert!(!session.livemode);
    assert_eq!(session.amount_total, Some(1234));
    assert_eq!(session.currency.as_deref(), Some("thb"));
    assert_eq!(session.client_reference_id.as_deref(), Some("checkout-one"));
    assert_eq!(session.metadata["attempt_id"], "attempt-one");
    assert_eq!(
        session
            .payment_intent
            .as_ref()
            .map(crate::models::stripe::StripeReference::id),
        Some("pi_one")
    );
    let intent = c.get_payment_intent("pi_one").await.unwrap();
    assert_eq!(intent.status, PaymentIntentStatus::Succeeded);
    assert_eq!(intent.amount_received, 1234);
    assert_eq!(
        intent
            .latest_charge
            .as_ref()
            .map(crate::models::stripe::StripeReference::id),
        Some("ch_one")
    );
    assert_eq!(intent.metadata["checkout_id"], "checkout-one");
    assert_eq!(c.get_account().await.unwrap().id, "acct_one");
    let paths: Vec<_> = f.requests().into_iter().map(|r| r.0).collect();
    assert_eq!(
        paths,
        [
            "/v1/checkout/sessions/cs_test_one",
            "/v1/payment_intents/pi_one",
            "/v1/account"
        ]
    );
    assert!(f.methods().iter().all(|m| m == reqwest::Method::GET));
    for request in f.requests() {
        assert_eq!(request.1["authorization"], "Bearer sk_test_private");
    }
}

#[tokio::test]
async fn new_stripe_reads_reject_path_injection_without_requests() {
    let f = Fixture::new(vec![]).await;
    let c = client(&f);
    for id in ["", "../secret", "one/two", "one?expand[]=x"] {
        assert!(c.get_checkout_session(id).await.is_err());
        assert!(c.get_payment_intent(id).await.is_err());
    }
    assert_eq!(f.requests(), []);
}

#[tokio::test]
async fn verification_reads_preserve_safe_decode_and_http_errors() {
    use crate::client::ClientErrorKind;
    for operation in ["session", "intent", "account"] {
        for reply in [Reply::json(json!({})), Reply::status(500)] {
            let decode = reply.status == 200;
            let f = Fixture::new(vec![reply]).await;
            let c = client(&f);
            let error = match operation {
                "session" => c.get_checkout_session("cs_test_one").await.map(|_| ()),
                "intent" => c.get_payment_intent("pi_one").await.map(|_| ()),
                _ => c.get_account().await.map(|_| ()),
            }
            .unwrap_err();
            if decode {
                assert!(matches!(error.kind(), ClientErrorKind::Decode { .. }));
                assert!(std::error::Error::source(&error).is_some());
            } else {
                assert!(matches!(
                    error.kind(),
                    ClientErrorKind::Http { status: 500, .. }
                ));
            }
            assert!(!format!("{error} {error:?}").contains("sk_test_private"));
            assert_eq!(f.requests().len(), 1);
        }
    }
}

#[tokio::test]
async fn nested_checkout_path_preserves_base_without_trailing_slash() {
    for base_path in ["/v1", "/v1/"] {
        let f = Fixture::new(vec![
            Reply::status(500),
            Reply::json(json!({"id":"acct_one","object":"account"})),
        ])
        .await;
        let c = StripeClient::new(&StripeConfig {
            base_url: f.url.join(base_path).unwrap().to_string(),
            timeout_secs: 1,
            secret_key: Some("fixture".into()),
        })
        .unwrap();
        assert!(c.get_checkout_session("cs_test_one").await.is_err());
        c.get_account().await.unwrap();
        assert_eq!(f.requests()[0].0, "/v1/checkout/sessions/cs_test_one");
        assert_eq!(f.requests()[1].0, "/v1/account");
    }
}

#[tokio::test]
async fn charge_retrieval_retains_environment_and_unrecognized_association_metadata() {
    let f = Fixture::new(vec![Reply::json(json!({"id":"ch_one","amount_captured":1234,"amount_refunded":0,"captured":true,"created":3,"currency":"thb","disputed":false,"metadata":{"checkout_id":"checkout-one","attempt_id":"attempt-one","order_id":"draft-one"},"paid":true,"payment_intent":"pi_one","refunded":false,"livemode":false,"balance_transaction":null}))]).await;
    let charge = client(&f).get_charge("ch_one").await.unwrap();
    assert_eq!(charge.livemode, Some(false));
    assert_eq!(charge.metadata.extra["checkout_id"], "checkout-one");
    assert_eq!(charge.metadata.extra["attempt_id"], "attempt-one");
    assert_eq!(charge.metadata.order_id.as_deref(), Some("draft-one"));
}
