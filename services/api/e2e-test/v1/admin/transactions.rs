#![allow(
    clippy::unwrap_used,
    clippy::too_many_lines,
    clippy::needless_pass_by_value
)]
use crate::common::{
    JWT_SECRET, ProviderConfig, assert_report_error, authed, body_json, client_with_providers,
    fixture,
};
use axum::{body::Body, http::Request};
use domain::dto::AdminRole;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    task::JoinHandle,
};

const URI: &str = "/v1/admin/transactions?date_from=2026-10-01T00%3A00%3A00%2B07%3A00&date_to=2026-10-31T23%3A59%3A59%2B07%3A00";

#[tokio::test]
async fn transactions_maps_settled_sales_refunds_and_receipt_references_over_tcp() {
    let fixture = fixture().await;
    for (stripe, paypal, paginated, card_gateway) in [
        (true, false, false, false),
        (false, true, false, false),
        (true, true, true, false),
        (true, false, false, true),
    ] {
        let mut steps = flow(stripe, paypal, paginated, false);
        if card_gateway {
            if let Reply::Json(ref mut body) = steps.last_mut().unwrap().reply {
                let order = &mut body["data"]["orders"]["nodes"][0];
                let sale = &mut order["transactions"][0];
                sale["gateway"] = json!("Stripe Card Payments");
                sale["paymentId"] = json!("session_12345");
                sale["receiptJson"] = Value::Null;
                let refund = &mut order["refunds"][0]["transactions"]["nodes"][0];
                refund["gateway"] = json!("Stripe Card Payments");
                refund["paymentId"] = json!("session_12345");
                refund["receiptJson"] = Value::Null;
            }
        }
        if card_gateway {
            if let Reply::Json(ref mut body) = steps[1].reply {
                body["data"][0]["description"] =
                    json!("Paid (gid://shopify/PaymentSession/session_12345)");
                body["data"][1]["description"] =
                    json!("REFUND FOR CHARGE (gid://shopify/PaymentSession/session_12345)");
            }
        }
        let expected_calls = steps.len();
        let server = Server::start(steps).await;
        let client = server.client(stripe, paypal).await;
        let response = client
            .send(authed("GET", URI, fixture.root_id(), AdminRole::Admin))
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let report = body_json(response).await;
        let rows = report["transactions"].as_array().unwrap();
        assert_eq!(
            rows.len(),
            usize::from(stripe) * 2 + usize::from(paypal) * 6
        );
        assert_eq!(
            report["items"].as_array().unwrap().len(),
            usize::from(stripe) * 2 + usize::from(paypal) * 2
        );
        if stripe {
            let sale_reference = if card_gateway {
                "session_12345"
            } else {
                "ch_123"
            };
            let refund_reference = if card_gateway {
                "session_12345"
            } else {
                "re_123"
            };
            let sale = rows
                .iter()
                .find(|row| row["payment_reference"] == sale_reference && row["type"] == "SALE")
                .unwrap();
            assert_eq!(sale["gross_paid"], 3600.0);
            assert_eq!(sale["net_paid"], 3480.0);
            assert_eq!(sale["fee"], 100.0);
            assert_eq!(sale["vat"], 20.0);
            assert_eq!(sale["shipping_fee"], 180.0);
            assert_eq!(sale["shopify_order_id"], 123);
            assert_eq!(sale["customer_name"], "Buyer");
            let refund = rows
                .iter()
                .find(|row| row["payment_reference"] == refund_reference && row["type"] == "REFUND")
                .unwrap();
            assert_eq!(refund["type"], "REFUND");
            assert_eq!(refund["gross_paid"], -360.0);
            assert_eq!(refund["shipping_fee"], -36.0);
        }
        if paypal {
            let sale = rows
                .iter()
                .find(|row| row["payment_reference"] == "pp_sale")
                .unwrap();
            assert_eq!(sale["gross_paid"], 100.0);
            assert_eq!(sale["net_paid"], 98.0);
            assert_eq!(sale["shipping_fee"], 10.0);
            assert_eq!(sale["shopify_order_id"], 124);
            let refund = rows
                .iter()
                .find(|row| row["payment_reference"] == "pp_refund")
                .unwrap();
            assert_eq!(refund["gross_paid"], -20.0);
            assert_eq!(refund["shipping_fee"], -2.0);
            assert_eq!(
                rows.iter()
                    .filter(|row| row["payment_reference"] == "same")
                    .count(),
                2
            );
            let old_sale = rows
                .iter()
                .find(|row| row["payment_reference"] == "usd_sale")
                .unwrap();
            assert_eq!(old_sale["gross_paid"], 360.0);
            assert_eq!(old_sale["exchange_rate"], 36.0);
            assert_eq!(old_sale["po_date"], "2026-10-20T00:00:00+00:00");
            assert_eq!(
                rows.iter()
                    .filter(|row| row["payment_reference"] == "boundary")
                    .count(),
                1
            );
        }
        let items = report["items"].as_array().unwrap();
        assert!(items.iter().all(|item| item["category"] == "KEYCHAIN"));
        assert!(
            items.iter().any(|item| item["kind"] == "REFUND"
                && item["quantity"] == -1
                && item["price"] == -10.0)
                || !stripe
        );
        assert_eq!(server.finish().await.len(), expected_calls);
    }
}

#[tokio::test]
async fn transactions_empty_results_and_both_admin_roles_over_tcp() {
    let fixture = fixture().await;
    for (stripe, paypal, role) in [
        (true, false, AdminRole::Admin),
        (false, true, AdminRole::SuperAdmin),
    ] {
        let steps = flow(stripe, paypal, false, true);
        let count = steps.len();
        let server = Server::start(steps).await;
        let client = server.client(stripe, paypal).await;
        let response = client
            .send(authed("GET", URI, fixture.root_id(), role))
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        assert_eq!(
            body_json(response).await,
            json!({"transactions":[],"items":[]})
        );
        assert_eq!(server.finish().await.len(), count);
    }
}

#[tokio::test]
async fn transactions_rejects_each_query_rule_and_auth_before_provider_calls() {
    let fixture = fixture().await;
    let server = Server::start(vec![]).await;
    let client = server.client(true, true).await;
    for query in [
        "",
        "date_from=2026-10-01T00:00:00Z",
        "date_to=2026-10-31T00:00:00Z",
        "date_from=bad&date_to=2026-10-31T00:00:00Z",
        "date_from=2026-10-01T00:00:00Z&date_to=bad",
        "date_from=2026-10-01T00:00:00Z&date_to=2026-10-01T00:00:00Z",
        "date_from=2026-11-01T00:00:00Z&date_to=2026-10-01T00:00:00Z",
        "date_from=2026-10-01T00:00:00Z&date_to=2026-10-31T00:00:00Z&extra=1",
    ] {
        let response = client
            .send(authed(
                "GET",
                &format!("/v1/admin/transactions?{query}"),
                fixture.root_id(),
                AdminRole::Admin,
            ))
            .await
            .unwrap();
        assert_report_error(
            response,
            400,
            "Expected valid RFC3339 date_from and date_to values with date_from before date_to",
        )
        .await;
    }
    let expired = jsonwebtoken::encode(
        &jsonwebtoken::Header::default(),
        &json!({"sub":1,"username":"fixture","role":"ADMIN","exp":1}),
        &jsonwebtoken::EncodingKey::from_secret(&hex::decode(JWT_SECRET).unwrap()),
    )
    .unwrap();
    for token in [None, Some("invalid".to_owned()), Some(expired)] {
        let mut request = Request::builder().method("GET").uri(URI);
        if let Some(token) = token {
            request = request.header("authorization", format!("Bearer {token}"));
        }
        let response = client
            .send(request.body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.headers()["www-authenticate"], "Bearer");
        assert_report_error(response, 401, "Invalid or expired token").await;
    }
    assert!(server.finish().await.is_empty());
}

#[tokio::test]
async fn transactions_unavailable_configuration_is_safe() {
    let fixture = fixture().await;
    for (stripe, paypal, shopify, message) in [
        (
            None,
            None,
            Some("http://127.0.0.1:1/graphql.json"),
            "Accounting report provider unavailable",
        ),
        (
            Some("invalid-url"),
            None,
            Some("http://127.0.0.1:1/graphql.json"),
            "Stripe client unavailable",
        ),
        (
            None,
            Some("invalid-url"),
            Some("http://127.0.0.1:1/graphql.json"),
            "PayPal client unavailable",
        ),
        (
            Some("http://127.0.0.1:1"),
            None,
            None,
            "Shopify client unavailable",
        ),
        (
            Some("http://127.0.0.1:1"),
            None,
            Some("invalid-url"),
            "Shopify client unavailable",
        ),
    ] {
        let client = client_with_providers(ProviderConfig {
            stripe_url: stripe,
            paypal_url: paypal,
            shopify_url: shopify,
            ..Default::default()
        })
        .await;
        assert_report_error(
            client
                .send(authed("GET", URI, fixture.root_id(), AdminRole::Admin))
                .await
                .unwrap(),
            503,
            message,
        )
        .await;
    }
}

#[tokio::test]
async fn transactions_maps_each_dependency_failure_and_stops_at_the_failed_call() {
    let fixture = fixture().await;
    let original = flow(true, true, true, false);
    for stage in 0..original.len() {
        for fault in [
            "http",
            "malformed",
            "missing",
            "shape",
            "disconnect",
            "refused",
            "timeout",
        ] {
            let mut steps = original[..stage].to_vec();
            let mut failed = original[stage].clone();
            failed.reply = Reply::Fault(fault);
            steps.push(failed);
            let server = Server::start(steps).await;
            let client = server.client(true, true).await;
            let response = client
                .send(authed("GET", URI, fixture.root_id(), AdminRole::Admin))
                .await
                .unwrap();
            assert_report_error(
                response,
                if fault == "timeout" { 504 } else { 502 },
                "Upstream request failed",
            )
            .await;
            assert_eq!(
                server.finish().await.len(),
                stage + usize::from(fault != "refused"),
                "stage {stage}, {fault}"
            );
        }
    }
}

#[tokio::test]
async fn transactions_exhausts_throttling_without_advancing_and_rejects_graphql_partial_data() {
    let fixture = fixture().await;
    let original = flow(true, true, true, false);
    for stage in 0..original.len() {
        let mut steps = original[..stage].to_vec();
        let mut failed = original[stage].clone();
        failed.reply = Reply::Fault("throttled");
        steps.extend(std::iter::repeat_n(failed, 4));
        let server = Server::start(steps).await;
        let client = server.client(true, true).await;
        assert_report_error(
            client
                .send(authed("GET", URI, fixture.root_id(), AdminRole::Admin))
                .await
                .unwrap(),
            502,
            "Upstream request failed",
        )
        .await;
        assert_eq!(server.finish().await.len(), stage + 4);
    }
    for stage in original
        .iter()
        .enumerate()
        .filter(|(_, step)| step.path == "/graphql.json")
        .map(|(i, _)| i)
    {
        for fault in ["graphql", "partial"] {
            let mut steps = original[..stage].to_vec();
            let mut failed = original[stage].clone();
            failed.reply = Reply::Fault(fault);
            steps.push(failed);
            let server = Server::start(steps).await;
            let client = server.client(true, true).await;
            assert_report_error(
                client
                    .send(authed("GET", URI, fixture.root_id(), AdminRole::Admin))
                    .await
                    .unwrap(),
                502,
                "Upstream request failed",
            )
            .await;
            assert_eq!(server.finish().await.len(), stage + 1);
        }
    }
}

#[tokio::test]
async fn transactions_rejects_bad_currency_continuations_and_incomplete_connections() {
    let fixture = fixture().await;
    let original = flow(true, false, false, false);
    let mut bad = Vec::new();
    let mut currency = original[..2].to_vec();
    if let Reply::Json(ref mut body) = currency[1].reply {
        body["data"][0]["currency"] = json!("usd");
    }
    bad.push(currency);
    let mut fee = original[..2].to_vec();
    if let Reply::Json(ref mut body) = fee[1].reply {
        body["data"][0]["fee_details"][0]["currency"] = json!("usd");
    }
    bad.push(fee);
    let mut cursor = original[..1].to_vec();
    cursor[0].reply = Reply::Json(json!({"data":[],"has_more":true}));
    bad.push(cursor);
    for path in [
        "/data/orders/nodes/0/lineItems/pageInfo/hasNextPage",
        "/data/orders/nodes/0/shippingLines/pageInfo/hasNextPage",
        "/data/orders/nodes/0/refunds/0/transactions/pageInfo/hasNextPage",
        "/data/orders/nodes/0/refunds/0/refundShippingLines/pageInfo/hasNextPage",
        "/data/orders/nodes/0/refunds/0/refundLineItems/pageInfo/hasNextPage",
    ] {
        let mut steps = original.clone();
        if let Reply::Json(ref mut body) = steps.last_mut().unwrap().reply {
            *body.pointer_mut(path).unwrap() = json!(true);
        }
        bad.push(steps);
    }
    let mut missing_rate = original[..3].to_vec();
    if let Reply::Json(ref mut body) = missing_rate[1].reply {
        body["data"][0]["exchange_rate"] = Value::Null;
    }
    bad.push(missing_rate);
    for invalid in [
        "ambiguous",
        "receipt",
        "missing_connection",
        "shipping_currency",
    ] {
        let mut steps = original.clone();
        if let Reply::Json(ref mut body) = steps.last_mut().unwrap().reply {
            match invalid {
                "ambiguous" => {
                    let mut duplicate = body["data"]["orders"]["nodes"][0].clone();
                    duplicate["id"] = json!("gid://shopify/Order/999");
                    body["data"]["orders"]["nodes"]
                        .as_array_mut()
                        .unwrap()
                        .push(duplicate);
                }
                "receipt" => {
                    body["data"]["orders"]["nodes"][0]["transactions"][0]["receiptJson"] =
                        json!("{")
                }
                "missing_connection" => {
                    body["data"]["orders"]["nodes"][0]
                        .as_object_mut()
                        .unwrap()
                        .remove("lineItems");
                }
                "shipping_currency" => {
                    body["data"]["orders"]["nodes"][0]["shippingLines"]["nodes"][0]["priceSet"]["shopMoney"]
                        ["currencyCode"] = json!("EUR")
                }
                _ => unreachable!(),
            }
        }
        bad.push(steps);
    }
    let mut page = flow(false, true, false, false);
    page.truncate(2);
    if let Reply::Json(ref mut body) = page[1].reply {
        body["page"] = json!(2);
    }
    bad.push(page);
    for steps in bad {
        let count = steps.len();
        let server = Server::start(steps).await;
        let client = server
            .client(server.stripe_first(), !server.stripe_first())
            .await;
        assert_report_error(
            client
                .send(authed("GET", URI, fixture.root_id(), AdminRole::Admin))
                .await
                .unwrap(),
            502,
            "Upstream request failed",
        )
        .await;
        assert_eq!(server.finish().await.len(), count);
    }
}

#[tokio::test]
async fn transactions_keeps_unmatched_provider_rows_without_unrelated_order_items() {
    let fixture = fixture().await;
    let mut steps = flow(true, false, false, false);
    if let Reply::Json(ref mut body) = steps.last_mut().unwrap().reply {
        let order = &mut body["data"]["orders"]["nodes"][0];
        order["transactions"][0]["receiptJson"] = Value::Null;
        order["refunds"][0]["transactions"]["nodes"][0]["receiptJson"] = Value::Null;
    }
    let count = steps.len();
    let server = Server::start(steps).await;
    let client = server.client(true, false).await;
    let response = client
        .send(authed("GET", URI, fixture.root_id(), AdminRole::Admin))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let report = body_json(response).await;
    assert_eq!(report["transactions"].as_array().unwrap().len(), 2);
    assert_eq!(report["items"], json!([]));
    assert!(
        report["transactions"]
            .as_array()
            .unwrap()
            .iter()
            .all(|row| row["shopify_order_id"] == 0 && row["shipping_fee"] == 0.0)
    );
    assert_eq!(server.finish().await.len(), count);
}

#[derive(Clone)]
enum Reply {
    Json(Value),
    Fault(&'static str),
}
#[derive(Clone)]
struct Step {
    path: &'static str,
    query: BTreeMap<String, String>,
    reply: Reply,
    cursor: Option<&'static str>,
    paypal_search: bool,
}
impl Step {
    fn json(path: &'static str, value: Value) -> Self {
        Self {
            path,
            query: BTreeMap::new(),
            reply: Reply::Json(value),
            cursor: None,
            paypal_search: false,
        }
    }
    fn query(mut self, pairs: &[(&str, &str)]) -> Self {
        self.query = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        self
    }
}
struct Server {
    url: String,
    task: Option<JoinHandle<()>>,
    requests: Arc<Mutex<Vec<String>>>,
    stripe_first: bool,
}
impl Server {
    async fn start(steps: Vec<Step>) -> Self {
        let stripe_first = steps.first().is_some_and(|step| step.path == "/payouts");
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let recorded = requests.clone();
        let task = tokio::spawn(async move {
            for step in steps {
                if matches!(step.reply, Reply::Fault("refused")) {
                    break;
                }
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut bytes = Vec::new();
                let mut buffer = [0u8; 4096];
                let (header_end, length) = loop {
                    let n = stream.read(&mut buffer).await.unwrap();
                    assert_ne!(n, 0);
                    bytes.extend_from_slice(&buffer[..n]);
                    if let Some(end) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
                        let header = String::from_utf8(bytes[..end].to_vec()).unwrap();
                        let length = header
                            .lines()
                            .find_map(|line| {
                                line.to_ascii_lowercase()
                                    .strip_prefix("content-length:")
                                    .map(|value| value.trim().parse::<usize>().unwrap())
                            })
                            .unwrap_or(0);
                        break (end + 4, length);
                    }
                };
                while bytes.len() < header_end + length {
                    let n = stream.read(&mut buffer).await.unwrap();
                    assert_ne!(n, 0);
                    bytes.extend_from_slice(&buffer[..n]);
                }
                let header = String::from_utf8(bytes[..header_end].to_vec()).unwrap();
                let first = header.lines().next().unwrap();
                recorded.lock().unwrap().push(first.into());
                let uri = first.split_whitespace().nth(1).unwrap();
                let url = reqwest::Url::parse(&format!("http://fixture{uri}")).unwrap();
                assert_eq!(url.path(), step.path);
                assert_eq!(
                    url.query_pairs()
                        .map(|(k, v)| (k.into_owned(), v.into_owned()))
                        .collect::<BTreeMap<_, _>>(),
                    step.query
                );
                let lower = header.to_ascii_lowercase();
                let body = &bytes[header_end..header_end + length];
                match step.path {
                    "/graphql.json" => {
                        assert!(first.starts_with("POST "));
                        assert!(lower.contains("x-shopify-access-token: e2e-fixture-token"));
                        let input: Value = serde_json::from_slice(body).unwrap();
                        let query =
                            include_str!("../../../src/client/shopify/orders_by_payment.graphql");
                        assert_eq!(input["query"], query);
                        assert_eq!(input["operationName"], "OrdersByPayment");
                        let compact = query.split_whitespace().collect::<String>();
                        assert!(compact.contains("transactions(first:250){nodes{"));
                        assert!(compact.contains("refundShippingLines(first:250)"));
                        assert!(!compact.contains("orderAdjustments"));
                        assert!(compact.contains("priceSet:originalPriceSet"));
                        assert_eq!(input["variables"]["first"], 10);
                        assert_eq!(input["variables"]["after"], json!(step.cursor));
                        let start = if step.paypal_search {
                            "2026-09-15T00:00:00+00:00"
                        } else {
                            "2026-10-01T00:00:00+07:00"
                        };
                        let expected = format!(
                            "(created_at:>='{start}' AND created_at:<='2026-10-31T23:59:59+07:00') OR (updated_at:>='{start}' AND updated_at:<='2026-10-31T23:59:59+07:00')"
                        );
                        assert_eq!(input["variables"]["search"], expected);
                    }
                    "/v1/oauth2/token" => {
                        assert!(first.starts_with("POST "));
                        assert!(lower.contains("authorization: basic "));
                        assert_eq!(body, b"grant_type=client_credentials");
                    }
                    "/v1/reporting/transactions" => {
                        assert!(first.starts_with("GET "));
                        assert!(lower.contains("authorization: bearer e2e-token"));
                    }
                    _ => {
                        assert!(first.starts_with("GET "));
                        assert!(lower.contains("authorization: bearer e2e-fixture-key"));
                    }
                }
                let (status,body)=match step.reply {
                    Reply::Json(value)=>(200,value.to_string()),
                    Reply::Fault("disconnect")=>continue,
                    Reply::Fault("timeout")=>{tokio::time::sleep(Duration::from_secs(4)).await;continue;}
                    Reply::Fault("http")=>(503,"private upstream failure".into()),
                    Reply::Fault("throttled")=>(429,"private throttle".into()),
                    Reply::Fault("malformed")=>(200,"{".into()),
                    Reply::Fault("missing")=>(200,"null".into()),
                    Reply::Fault("shape")=>(200,json!({"data":false,"id":0,"transaction_details":false}).to_string()),
                    Reply::Fault("graphql")=>(200,json!({"errors":[{"message":"private GraphQL failure"}]}).to_string()),
                    Reply::Fault("partial")=>(200,json!({"data":{"orders":page(vec![],None)},"errors":[{"message":"private partial failure"}]}).to_string()),
                    Reply::Fault(other)=>panic!("unexpected fault {other}"),
                };
                let response = format!(
                    "HTTP/1.1 {status} Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes()).await;
            }
        });
        Self {
            url,
            task: Some(task),
            requests,
            stripe_first,
        }
    }
    fn stripe_first(&self) -> bool {
        self.stripe_first
    }
    async fn client(&self, stripe: bool, paypal: bool) -> crate::common::TestClient {
        let graphql = format!("{}/graphql.json", self.url);
        client_with_providers(ProviderConfig {
            shopify_url: Some(&graphql),
            stripe_url: stripe.then_some(self.url.as_str()),
            paypal_url: paypal.then_some(self.url.as_str()),
            ..Default::default()
        })
        .await
    }
    async fn finish(mut self) -> Vec<String> {
        tokio::time::timeout(Duration::from_secs(3), self.task.take().unwrap())
            .await
            .unwrap()
            .unwrap();
        self.requests.lock().unwrap().clone()
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}

fn flow(stripe: bool, paypal: bool, paginated: bool, empty: bool) -> Vec<Step> {
    let mut steps = Vec::new();
    if stripe {
        let payouts = if empty {
            vec![]
        } else {
            vec![json!({"id":"po_123","arrival_date":1_790_985_600})]
        };
        steps.push(
            Step::json(
                "/payouts",
                json!({"data":payouts,"has_more":paginated && !empty}),
            )
            .query(&[
                ("arrival_date[gte]", "1790787600"),
                ("arrival_date[lte]", "1793465999"),
                ("status", "paid"),
                ("limit", "100"),
            ]),
        );
        if !empty {
            steps.push(Step::json("/balance_transactions",json!({"data":if paginated {vec![balance(false)]} else {vec![balance(false),balance(true)]},"has_more":paginated})).query(&[("payout","po_123"),("limit","100")]));
            steps.push(Step::json("/charges/ch_123",json!({"id":"ch_123","amount_captured":10_000,"amount_refunded":1000,"captured":true,"paid":true,"refunded":false,"created":1_790_899_200,"currency":"usd","disputed":false,"metadata":{"customer_email":"buyer@example.test"},"payment_intent":null,"balance_transaction":"txn_sale"})));
            if paginated {
                steps.push(
                    Step::json(
                        "/balance_transactions",
                        json!({"data":[balance(true)],"has_more":false}),
                    )
                    .query(&[
                        ("payout", "po_123"),
                        ("limit", "100"),
                        ("starting_after", "txn_sale"),
                    ]),
                );
            }
            steps.push(Step::json("/refunds/re_123",json!({"id":"re_123","amount":1000,"created":1_790_985_600,"currency":"usd","payment_intent":null,"reason":null})));
            if paginated {
                steps.push(
                    Step::json("/payouts", json!({"data":[],"has_more":false})).query(&[
                        ("arrival_date[gte]", "1790787600"),
                        ("arrival_date[lte]", "1793465999"),
                        ("status", "paid"),
                        ("limit", "100"),
                        ("starting_after", "po_123"),
                    ]),
                );
            }
        }
    }
    if paypal {
        steps.push(Step::json(
            "/v1/oauth2/token",
            json!({"access_token":"e2e-token","token_type":"Bearer","expires_in":3600}),
        ));
        let sale = detail(
            "pp_sale",
            "T0006",
            "2026-09-30T20:00:00Z",
            "THB",
            "100",
            Some("-2"),
        );
        let old = detail(
            "usd_sale",
            "T0006",
            "2026-09-15T00:00:00Z",
            "USD",
            "10",
            None,
        );
        let boundary = detail(
            "boundary",
            "T0006",
            "2026-10-01T17:00:00Z",
            "THB",
            "40",
            None,
        );
        let mut first = if empty {
            vec![]
        } else {
            vec![
                sale,
                old,
                detail(
                    "same",
                    "T0000",
                    "2026-10-01T00:00:00.123Z",
                    "THB",
                    "10",
                    None,
                ),
                detail(
                    "same",
                    "T0011",
                    "2026-10-01T00:00:00.123Z",
                    "THB",
                    "10",
                    None,
                ),
            ]
        };
        if !empty && !paginated {
            first.push(boundary.clone());
        }
        steps.push(report_step(
            "2026-08-31T17:00:00Z",
            "2026-10-01T17:00:00Z",
            1,
            if paginated && !empty { 2 } else { 1 },
            first,
        ));
        if paginated && !empty {
            steps.push(report_step(
                "2026-08-31T17:00:00Z",
                "2026-10-01T17:00:00Z",
                2,
                2,
                vec![boundary.clone()],
            ));
        }
        let second = if empty {
            vec![]
        } else {
            vec![
                boundary,
                detail(
                    "pp_refund",
                    "T1107",
                    "2026-10-03T00:00:00Z",
                    "THB",
                    "-20",
                    None,
                ),
                detail(
                    "withdraw",
                    "T0400",
                    "2026-10-20T00:00:00Z",
                    "THB",
                    "0",
                    None,
                ),
                detail("thb", "T0201", "2026-10-19T00:00:00Z", "THB", "3600", None),
                detail("usd", "T0201", "2026-10-18T00:00:00Z", "USD", "-100", None),
            ]
        };
        steps.push(report_step(
            "2026-10-01T17:00:00Z",
            "2026-10-31T16:59:59Z",
            1,
            1,
            second,
        ));
    }
    if !empty {
        let mut first = Step::json(
            "/graphql.json",
            json!({"data":{"orders":page(if paginated {vec![order(true)]} else {[stripe.then(||order(true)),paypal.then(||order(false))].into_iter().flatten().collect()},if paginated {Some("order-1")} else {None})}}),
        );
        first.paypal_search = paypal;
        steps.push(first);
        if paginated {
            let mut second = Step::json(
                "/graphql.json",
                json!({"data":{"orders":page(vec![order(true),order(false)],None)}}),
            );
            second.cursor = Some("order-1");
            second.paypal_search = paypal;
            steps.push(second);
        }
    }
    steps
}
fn report_step(
    start: &str,
    end: &str,
    page_number: u32,
    total_pages: u32,
    rows: Vec<Value>,
) -> Step {
    Step::json("/v1/reporting/transactions",json!({"transaction_details":rows,"page":page_number,"total_items":10,"total_pages":total_pages}))
        .query(&[("start_date",start),("end_date",end),("fields","all"),("page_size","500"),("page",&page_number.to_string())])
}
fn balance(refund: bool) -> Value {
    json!({"amount":if refund {-36_000} else {360_000},"net":if refund {-36_000} else {348_000},"fee":if refund {0} else {12_000},"fee_details":if refund {vec![]} else {vec![json!({"amount":10_000,"currency":"thb","type":"stripe_fee"}),json!({"amount":2000,"currency":"thb","type":"tax"})]},"available_on":1_790_985_600,"created":1_790_899_200,"currency":"thb","id":if refund {"txn_refund"} else {"txn_sale"},"object":"balance_transaction","reporting_category":if refund {"refund"} else {"charge"},"source":if refund {"re_123"} else {"ch_123"},"status":"available","type":if refund {"refund"} else {"charge"},"exchange_rate":36.0,"description":"Charge"})
}
fn detail(
    id: &str,
    event: &str,
    date: &str,
    currency: &str,
    amount: &str,
    fee: Option<&str>,
) -> Value {
    json!({"transaction_info":{"transaction_id":id,"transaction_event_code":event,"transaction_amount":{"currency_code":currency,"value":amount},"transaction_initiation_date":date,"transaction_updated_date":date,"transaction_status":"S","fee_amount":fee.map(|value|json!({"currency_code":currency,"value":value}))}})
}
fn page(nodes: Vec<Value>, cursor: Option<&str>) -> Value {
    json!({"nodes":nodes,"pageInfo":{"hasNextPage":cursor.is_some(),"endCursor":cursor}})
}
fn order(stripe: bool) -> Value {
    let currency = if stripe { "USD" } else { "THB" };
    let line = json!({"title":"Keychain","variantTitle":"Blue","sku":"ABCKEY001","quantity":if stripe {2} else {1},"originalUnitPriceSet":{"shopMoney":{"amount":if stripe {"47.5"} else {"90"},"currencyCode":currency}}});
    let transaction = |refund: bool| json!({"id":if refund {"gid://shopify/OrderTransaction/2"} else {"gid://shopify/OrderTransaction/1"},"paymentId":"different-payment-id","gateway":if stripe {"stripe"} else {"paypal"},"kind":if refund {"REFUND"} else {"CAPTURE"},"status":"SUCCESS","processedAt":"2026-10-02T00:00:00Z","amountSet":{"shopMoney":{"amount":"100","currencyCode":currency}},"receiptJson":if stripe {if refund {json!({"id":"re_123"})} else {json!({"charges":{"data":[{"id":"ch_123"}]}})}} else if refund {json!({"refund_transaction_id":"pp_refund"})} else {json!({"transaction_id":"pp_sale"})}.to_string()});
    json!({"id":if stripe {"gid://shopify/Order/123"} else {"gid://shopify/Order/124"},"name":"#9876","number":9876,"createdAt":"2026-10-01T00:00:00Z","displayFulfillmentStatus":"UNFULFILLED","tags":[],
        "shippingAddress":{"name":"Buyer","address1":"1 Road","city":"Bangkok","zip":"10000","country":"Thailand","phone":"+66 81-234-5678"},
        "currentTotalPriceSet":{"shopMoney":{"amount":"100","currencyCode":currency}},
        "transactions":[transaction(false)],"lineItems":page(vec![line.clone()],None),
        "shippingLines":page(vec![json!({"priceSet":{"shopMoney":{"amount":if stripe {"5"} else {"10"},"currencyCode":currency}}})],None),
        "refunds":[{"transactions":page(vec![transaction(true)],None),"refundShippingLines":page(vec![json!({"subtotalAmountSet":{"shopMoney":{"amount":if stripe {"1"} else {"2"},"currencyCode":currency}}})],None),"refundLineItems":page(vec![json!({"quantity":1,"subtotalSet":{"shopMoney":{"amount":if stripe {"10"} else {"20"},"currencyCode":currency}},"lineItem":line})],None)}]})
}
