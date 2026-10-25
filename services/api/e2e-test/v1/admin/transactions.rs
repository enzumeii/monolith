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
    sync::oneshot,
    task::JoinHandle,
};

const URI: &str = "/api/v1/admin/transactions?date_from=2026-10-01T00%3A00%3A00%2B07%3A00&date_to=2026-10-31T23%3A59%3A59%2B07%3A00";

#[tokio::test]
async fn transactions_maps_settled_sales_refunds_and_receipt_references_over_tcp() {
    let fixture = fixture().await;
    for (stripe, paypal, paginated, card_gateway) in [
        (true, false, false, false),
        (false, true, false, false),
        (true, true, true, false),
        (true, false, false, true),
    ] {
        let mut steps = flow(stripe, paypal, paginated, Rows::Populated);
        if card_gateway && let Reply::Json(ref mut body) = steps.last_mut().unwrap().reply {
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
        if card_gateway && let Reply::Json(ref mut body) = steps[1].reply {
            body["data"][0]["description"] =
                json!("Paid (gid://shopify/PaymentSession/session_12345)");
            body["data"][1]["description"] =
                json!("REFUND FOR CHARGE (gid://shopify/PaymentSession/session_12345)");
        }
        let expected_paths = steps.iter().map(|step| step.path).collect::<Vec<_>>();
        let server = Server::start(steps).await;
        let client = server.client().await;
        let response = client
            .send(authed(
                "GET",
                &URI.replace("%2B", "+"),
                fixture.root_id(),
                AdminRole::Admin,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let report = body_json(response).await;
        let stripe_rows = report["transactions"]["stripe"].as_array().unwrap();
        let paypal_rows = report["transactions"]["paypal"].as_array().unwrap();
        assert_eq!(stripe_rows.len(), usize::from(stripe) * 2);
        assert_eq!(paypal_rows.len(), usize::from(paypal) * 6);
        assert!(
            stripe_rows
                .iter()
                .all(|row| row["gateway"] == "STRIPE" && row["currency"] == "THB")
        );
        assert!(paypal_rows.iter().all(|row| row["gateway"] == "PAYPAL"
            && row["exchange_rate"].is_null()
            && row["po_date"].is_null()));
        let rows = stripe_rows.iter().chain(paypal_rows).collect::<Vec<_>>();
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
            assert_eq!(
                sale["shopify_order_tags"],
                json!(["Native", "comma,quote\"", "ไทย"])
            );
            assert_eq!(sale["customer_name"], "Buyer");
            let refund = rows
                .iter()
                .find(|row| row["payment_reference"] == refund_reference && row["type"] == "REFUND")
                .unwrap();
            assert_eq!(refund["type"], "REFUND");
            assert_eq!(refund["gross_paid"], -360.0);
            assert_eq!(refund["shipping_fee"], -36.0);
            assert_eq!(refund["shopify_order_tags"], sale["shopify_order_tags"]);
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
            assert_eq!(
                sale["shopify_order_tags"],
                json!(["Native", "comma,quote\"", "ไทย"])
            );
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
            let aud = rows
                .iter()
                .find(|row| row["payment_reference"] == "aud_sale")
                .unwrap();
            assert_eq!(aud["currency"], "AUD");
            assert_eq!(aud["gross_paid"], 53.08);
            assert_eq!(aud["net_paid"], 50.26);
            assert_eq!(aud["fee"], 2.82);
            assert_eq!(aud["shipping_fee"], 10.08);
            assert_eq!(aud["shopify_order_id"], 0);
            assert_eq!(aud["shopify_order_tags"], json!([]));
            assert!(
                rows.iter()
                    .all(|row| row["payment_reference"] != "usd_sale")
            );
            assert_eq!(
                rows.iter()
                    .filter(|row| row["payment_reference"] == "boundary")
                    .count(),
                1
            );
        }
        for provider in [stripe_rows, paypal_rows] {
            assert!(provider.windows(2).all(|pair| {
                let key = |row: &Value| {
                    (
                        row["po_date"].is_null(),
                        row["po_date"]
                            .as_str()
                            .map(|s| chrono::DateTime::parse_from_rfc3339(s).unwrap()),
                        chrono::DateTime::parse_from_rfc3339(
                            row["transaction_date"].as_str().unwrap(),
                        )
                        .unwrap(),
                    )
                };
                key(&pair[0]) <= key(&pair[1])
            }));
            for row in provider {
                let value = |key: &str| row[key].as_f64().unwrap();
                assert!(
                    (value("gross_paid") - value("net_paid") - value("fee") - value("vat")).abs()
                        < 0.000_001
                );
            }
        }
        let items = report["items"].as_array().unwrap();
        assert!(items.windows(2).all(|pair| {
            let instant = |row: &Value| {
                chrono::DateTime::parse_from_rfc3339(row["transaction_date"].as_str().unwrap())
                    .unwrap()
            };
            instant(&pair[0]) <= instant(&pair[1])
        }));
        assert!(items.iter().all(|item| item["category"] == "KEYCHAIN"));
        assert!(
            items.iter().any(|item| item["kind"] == "REFUND"
                && item["quantity"] == -1
                && item["price"] == -10.0)
                || !stripe
        );
        let requests = server.finish().await;
        let actual_paths = requests
            .iter()
            .map(|request| {
                let uri = request.split_whitespace().nth(1).unwrap();
                reqwest::Url::parse(&format!("http://fixture{uri}"))
                    .unwrap()
                    .path()
                    .to_owned()
            })
            .collect::<Vec<_>>();
        assert_eq!(actual_paths, expected_paths);
        assert_eq!(
            actual_paths
                .iter()
                .filter(|path| *path == "/charges/ch_123")
                .count(),
            usize::from(stripe)
        );
        assert!(actual_paths.iter().all(|path| path != "/account"));
    }
}

#[tokio::test]
async fn transactions_empty_results_and_both_admin_roles_over_tcp() {
    let fixture = fixture().await;
    for (stripe, paypal, role) in [
        (true, false, AdminRole::Admin),
        (false, true, AdminRole::SuperAdmin),
    ] {
        let steps = flow(stripe, paypal, false, Rows::Empty);
        let count = steps.len();
        let server = Server::start(steps).await;
        let client = server.client().await;
        let response = client
            .send(authed("GET", URI, fixture.root_id(), role))
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        assert_eq!(
            body_json(response).await,
            json!({"transactions":{"stripe":[],"paypal":[]},"items":[]})
        );
        assert_eq!(server.finish().await.len(), count);
    }
}

#[tokio::test]
async fn transactions_paypal_explicit_empty_page_without_details_preserves_stripe_over_tcp() {
    let fixture = fixture().await;
    for stripe in [false, true] {
        let mut steps = flow(
            stripe,
            false,
            false,
            if stripe { Rows::Populated } else { Rows::Empty },
        );
        let report = steps
            .iter_mut()
            .find(|step| step.path == "/v1/reporting/transactions")
            .unwrap();
        report.reply = Reply::Json(json!({"page":1,"total_items":0,"total_pages":0}));
        let count = steps.len();
        let server = Server::start(steps).await;
        let client = server.client().await;
        let response = client
            .send(authed("GET", URI, fixture.root_id(), AdminRole::Admin))
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let report = body_json(response).await;
        assert_eq!(report["transactions"]["paypal"], json!([]));
        let stripe_rows = report["transactions"]["stripe"].as_array().unwrap();
        assert_eq!(stripe_rows.len(), usize::from(stripe) * 2);
        assert_eq!(
            report["items"].as_array().unwrap().len(),
            usize::from(stripe) * 2
        );
        if stripe {
            assert!(stripe_rows.iter().all(|row| row["shopify_order_id"] == 123));
            assert!(
                stripe_rows
                    .iter()
                    .any(|row| row["payment_reference"] == "ch_123")
            );
            assert!(
                stripe_rows
                    .iter()
                    .any(|row| row["payment_reference"] == "re_123")
            );
        } else {
            assert_eq!(
                report,
                json!({"transactions":{"stripe":[],"paypal":[]},"items":[]})
            );
        }
        assert_eq!(server.finish().await.len(), count);
    }
}

#[tokio::test]
async fn transactions_paypal_rejects_unproven_empty_or_malformed_details_over_tcp() {
    let fixture = fixture().await;
    for body in [
        json!({"page":1,"total_items":1,"total_pages":1}),
        json!({"page":1,"total_items":0,"total_pages":1}),
        json!({"page":1,"total_items":1,"total_pages":0}),
        json!({"page":2,"total_items":0,"total_pages":0}),
        json!({"page":1,"total_pages":0}),
        json!({"page":1,"total_items":0}),
        json!({"total_items":0,"total_pages":0}),
        json!({"page":1,"total_items":0,"total_pages":0,"transaction_details":null}),
        json!({"page":1,"total_items":0,"total_pages":0,"transaction_details":{}}),
    ] {
        let mut steps = flow(false, true, false, Rows::Populated);
        let stage = steps
            .iter()
            .position(|step| step.path == "/v1/reporting/transactions")
            .unwrap();
        steps.truncate(stage + 1);
        steps[stage].reply = Reply::Json(body);
        let server = Server::start(steps).await;
        let client = server.client().await;
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

#[tokio::test]
async fn transactions_rejects_each_query_rule_and_auth_before_provider_calls() {
    let fixture = fixture().await;
    let server = Server::start(vec![]).await;
    let client = server.client().await;
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
                &format!("/api/v1/admin/transactions?{query}"),
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
    assert_eq!(server.finish().await, [] as [std::string::String; 0]);
}

#[tokio::test]
async fn transactions_unavailable_configuration_is_safe() {
    let fixture = fixture().await;
    for (stripe, paypal, shopify, message) in [
        (
            None,
            Some("valid"),
            Some("valid"),
            "Stripe client unavailable",
        ),
        (
            Some("invalid-url"),
            Some("valid"),
            Some("valid"),
            "Stripe client unavailable",
        ),
        (
            Some("valid"),
            None,
            Some("valid"),
            "PayPal client unavailable",
        ),
        (
            Some("valid"),
            Some("invalid-url"),
            Some("valid"),
            "PayPal client unavailable",
        ),
        (None, None, Some("valid"), "Stripe client unavailable"),
        (
            None,
            Some("invalid-url"),
            Some("valid"),
            "Stripe client unavailable",
        ),
        (
            Some("valid"),
            Some("valid"),
            None,
            "Shopify client unavailable",
        ),
        (
            Some("valid"),
            Some("valid"),
            Some("invalid-url"),
            "Shopify client unavailable",
        ),
    ] {
        let server = Server::start(vec![]).await;
        let graphql = format!("{}/graphql.json", server.url);
        let configured = |value, url| match value {
            Some("valid") => Some(url),
            other => other,
        };
        let client = client_with_providers(ProviderConfig {
            stripe_url: configured(stripe, server.url.as_str()),
            paypal_url: configured(paypal, server.url.as_str()),
            shopify_url: configured(shopify, graphql.as_str()),
            ..Default::default()
        })
        .await;
        assert_eq!(
            client
                .send(
                    Request::builder()
                        .uri("/healthz")
                        .body(Body::empty())
                        .unwrap()
                )
                .await
                .unwrap()
                .status(),
            200
        );
        assert_eq!(
            client
                .send(authed(
                    "GET",
                    "/api/v1/admins",
                    fixture.root_id(),
                    AdminRole::SuperAdmin
                ))
                .await
                .unwrap()
                .status(),
            200
        );
        assert_report_error(
            client
                .send(Request::builder().uri(URI).body(Body::empty()).unwrap())
                .await
                .unwrap(),
            401,
            "Invalid or expired token",
        )
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
        assert!(server.finish().await.is_empty(), "{message}");
    }
}

#[tokio::test]
async fn transactions_maps_each_dependency_failure_and_stops_at_the_failed_call() {
    let fixture = fixture().await;
    let original = flow(true, true, true, Rows::Populated);
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
            let client = server.client().await;
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
    let original = flow(true, true, true, Rows::Populated);
    for stage in 0..original.len() {
        let mut steps = original[..stage].to_vec();
        let mut failed = original[stage].clone();
        failed.reply = Reply::Fault("throttled");
        steps.extend(std::iter::repeat_n(failed, 4));
        let server = Server::start(steps).await;
        let client = server.client().await;
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
            let client = server.client().await;
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
    let original = flow(true, false, false, Rows::Populated);
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
                        json!("{");
                }
                "missing_connection" => {
                    body["data"]["orders"]["nodes"][0]
                        .as_object_mut()
                        .unwrap()
                        .remove("lineItems");
                }
                "shipping_currency" => {
                    body["data"]["orders"]["nodes"][0]["shippingLines"]["nodes"][0]["priceSet"]["shopMoney"]
                        ["currencyCode"] = json!("EUR");
                }
                _ => unreachable!(),
            }
        }
        bad.push(steps);
    }
    let mut page = flow(false, true, false, Rows::Populated);
    page.truncate(3);
    if let Reply::Json(ref mut body) = page.last_mut().unwrap().reply {
        body["page"] = json!(2);
    }
    bad.push(page);
    for steps in bad {
        let count = steps.len();
        let server = Server::start(steps).await;
        let client = server.client().await;
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
    let mut steps = flow(true, false, false, Rows::Populated);
    if let Reply::Json(ref mut body) = steps.last_mut().unwrap().reply {
        let order = &mut body["data"]["orders"]["nodes"][0];
        order["transactions"][0]["receiptJson"] = Value::Null;
        order["refunds"][0]["transactions"]["nodes"][0]["receiptJson"] = Value::Null;
    }
    let count = steps.len();
    let server = Server::start(steps).await;
    let client = server.client().await;
    let response = client
        .send(authed("GET", URI, fixture.root_id(), AdminRole::Admin))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let report = body_json(response).await;
    assert_eq!(
        report["transactions"]["stripe"].as_array().unwrap().len(),
        2
    );
    assert_eq!(report["items"], json!([]));
    assert!(
        report["transactions"]["stripe"]
            .as_array()
            .unwrap()
            .iter()
            .all(|row| row["shopify_order_id"] == 0 && row["shipping_fee"] == 0.0)
    );
    assert_eq!(server.finish().await.len(), count);
}

#[tokio::test]
async fn transactions_raw_and_encoded_plus_return_identical_reports() {
    let fixture = fixture().await;
    let mut reports = Vec::new();
    for uri in [URI.to_owned(), URI.replace("%2B", "+")] {
        let steps = flow(true, true, true, Rows::Populated);
        let count = steps.len();
        let server = Server::start(steps).await;
        let client = server.client().await;
        let response = client
            .send(authed("GET", &uri, fixture.root_id(), AdminRole::Admin))
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        reports.push(body_json(response).await);
        assert_eq!(server.finish().await.len(), count);
    }
    assert_eq!(reports[0], reports[1]);
}

#[tokio::test]
async fn transactions_paypal_long_ranges_exhaust_each_window_and_page_without_fx_lookback() {
    let fixture = fixture().await;
    let mut steps = vec![
        Step::json("/payouts", json!({"data":[],"has_more":false})).query(&[
            ("arrival_date[gte]", "1788220800"),
            ("arrival_date[lte]", "1794700800"),
            ("status", "paid"),
            ("limit", "100"),
        ]),
        Step::json(
            "/v1/oauth2/token",
            json!({"access_token":"e2e-token","token_type":"Bearer","expires_in":3600}),
        ),
    ];
    for (start, end) in [
        ("2026-09-01T00:00:00Z", "2026-10-02T00:00:00Z"),
        ("2026-10-02T00:00:00Z", "2026-11-02T00:00:00Z"),
        ("2026-11-02T00:00:00Z", "2026-11-15T00:00:00Z"),
    ] {
        let conversion = detail("conversion", "T0201", start, "USD", "1", None);
        let mut rows = vec![conversion];
        if start == "2026-09-01T00:00:00Z" || start == "2026-10-02T00:00:00Z" {
            let mut boundary = detail(
                "shared-boundary",
                "T0006",
                "2026-10-02T00:00:00Z",
                "AUD",
                "53.08",
                Some("-2.82"),
            );
            boundary["transaction_info"]["shipping_amount"] =
                json!({"currency_code":"AUD","value":"10.08"});
            rows.push(boundary);
        }
        steps.push(report_step(start, end, 1, 2, rows));
        steps.push(report_step(start, end, 2, 2, vec![]));
    }
    let mut orders = Step::json(
        "/graphql.json",
        json!({"data":{"orders":page(vec![], None)}}),
    );
    orders.search_bounds = Some(("2026-09-01T00:00:00+00:00", "2026-11-15T00:00:00+00:00"));
    steps.push(orders);
    let count = steps.len();
    let server = Server::start(steps).await;
    let client = server.client().await;
    let response = client
        .send(authed(
            "GET",
            "/api/v1/admin/transactions?date_from=2026-09-01T00:00:00Z&date_to=2026-11-15T00:00:00Z",
            fixture.root_id(),
            AdminRole::Admin,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let report = body_json(response).await;
    let rows = report["transactions"]["paypal"].as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["payment_reference"], "shared-boundary");
    assert_eq!(rows[0]["gross_paid"], 53.08);
    assert_eq!(rows[0]["net_paid"], 50.26);
    assert_eq!(rows[0]["shipping_fee"], 10.08);
    assert_eq!(report["transactions"]["stripe"], json!([]));
    assert_eq!(report["items"], json!([]));
    assert_eq!(server.finish().await.len(), count);
}

#[tokio::test]
async fn transactions_paypal_rejects_incompatible_amount_currencies_before_shopify() {
    let fixture = fixture().await;
    for field in [
        "fee_amount",
        "shipping_amount",
        "transaction_amount",
        "overflow",
    ] {
        let mut steps = flow(false, true, false, Rows::Populated);
        steps.truncate(3);
        if let Reply::Json(ref mut body) = steps.last_mut().unwrap().reply {
            let info = &mut body["transaction_details"][2]["transaction_info"];
            if field == "overflow" {
                info["transaction_amount"]["value"] = json!("1e308");
            } else {
                info[field]["currency_code"] = json!(if field == "transaction_amount" {
                    "ZZZ"
                } else {
                    "USD"
                });
            }
        }
        let server = Server::start(steps).await;
        let client = server.client().await;
        assert_report_error(
            client
                .send(authed("GET", URI, fixture.root_id(), AdminRole::Admin))
                .await
                .unwrap(),
            502,
            "Upstream request failed",
        )
        .await;
        assert_eq!(server.finish().await.len(), 3);
    }
}

#[tokio::test]
async fn transactions_paypal_preserves_zero_and_signed_shipping_over_shopify() {
    let fixture = fixture().await;
    let mut steps = flow(false, true, false, Rows::Populated);
    if let Reply::Json(ref mut body) = steps
        .iter_mut()
        .find(|step| step.path == "/v1/reporting/transactions")
        .unwrap()
        .reply
    {
        for row in body["transaction_details"].as_array_mut().unwrap() {
            let info = &mut row["transaction_info"];
            let shipping = match info["transaction_id"].as_str().unwrap() {
                "pp_sale" => Some("0"),
                "pp_refund" => Some("-1.25"),
                _ => None,
            };
            if let Some(value) = shipping {
                info["shipping_amount"] = json!({"currency_code":"THB", "value":value});
            }
        }
    }
    let count = steps.len();
    let server = Server::start(steps).await;
    let client = server.client().await;
    let response = client
        .send(authed("GET", URI, fixture.root_id(), AdminRole::Admin))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let report = body_json(response).await;
    let rows = report["transactions"]["paypal"].as_array().unwrap();
    for (reference, shipping) in [("pp_sale", 0.0), ("pp_refund", -1.25)] {
        let row = rows
            .iter()
            .find(|row| row["payment_reference"] == reference)
            .unwrap();
        assert_eq!(row["shopify_order_id"], 124);
        assert_eq!(row["shipping_fee"], shipping);
    }
    assert_eq!(server.finish().await.len(), count);
}

#[tokio::test]
async fn external_checkout_payments_link_the_created_order_and_items_in_accounting_over_tcp() {
    use crate::common::checkout::{
        SECRET, accepted, configured, providers::Providers, purchase, verify,
    };
    let fixture = fixture().await;
    for provider in ["stripe", "paypal"] {
        let upstream = Providers::start().await;
        upstream.shopify_money("52.00", "AUD");
        let client = configured(&upstream).await;
        let input = purchase(&fixture, provider).await;
        accepted(&client, &input).await;
        upstream.pay();
        let response = client
            .send(verify(
                input["operation_id"].as_str().unwrap(),
                Some(SECRET),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let completed = body_json(response).await;
        assert_eq!(completed["status"], "completed");
        let order_id: u64 = completed["order"]["id"]
            .as_str()
            .unwrap()
            .rsplit('/')
            .next()
            .unwrap()
            .parse()
            .unwrap();
        let response = client
            .send(authed("GET", URI, fixture.root_id(), AdminRole::Admin))
            .await
            .unwrap();
        let status = response.status();
        let report = body_json(response).await;
        assert_eq!(status, 200, "{provider}: {report}");
        let rows = report["transactions"][provider].as_array().unwrap();
        assert_eq!(rows.len(), 1, "{provider}: {report}");
        let row = &rows[0];
        assert_eq!(row["shopify_order_id"], order_id);
        assert_eq!(row["shopify_order_number"], 1234);
        let tags = row["shopify_order_tags"].as_array().unwrap();
        assert_eq!(tags.len(), 1);
        assert!(!tags[0].as_str().unwrap().contains(':'));
        assert_eq!(upstream.count("EditOrderTags"), 0);
        assert_eq!(upstream.count("UpdateOrderCustomAttributes"), 1);
        assert_eq!(report["items"].as_array().unwrap().len(), 1);
        let item = &report["items"][0];
        assert_eq!(item["shopify_order_id"], order_id);
        assert_eq!(item["payment_reference"], row["payment_reference"]);
        assert_eq!(item["quantity"], 2);
        assert_eq!(item["currency"], "AUD");
        assert_eq!(item["price"], 26.0);
        assert_eq!(
            row["currency"],
            if provider == "stripe" { "THB" } else { "SGD" }
        );
        assert_eq!(upstream.count("CompleteCheckoutDraft"), 1);
        assert_eq!(upstream.count("RecordCheckoutPayment"), 1);
        assert_eq!(upstream.payment_creates(), 1);
    }
}

#[tokio::test]
async fn independent_checkout_money_links_settled_gateway_rows_to_shopify_items_over_tcp() {
    let fixture = fixture().await;
    for stripe in [true, false] {
        for mode in [
            "valid",
            "payment_mismatch",
            "shopify_mismatch",
            "reference_mismatch",
            "missing_money",
            "duplicate_money",
        ] {
            let mut steps = independent_accounting_flow(stripe);
            let Reply::Json(body) = &mut steps.last_mut().unwrap().reply else {
                unreachable!()
            };
            let order = &mut body["data"]["orders"]["nodes"][0];
            match mode {
                "payment_mismatch" => order["customAttributes"][5]["value"] = json!("1251"),
                "shopify_mismatch" => {
                    order["transactions"][0]["amountSet"]["shopMoney"]["amount"] = json!("4.01");
                }
                "reference_mismatch" => {
                    order["customAttributes"]
                        .as_array_mut()
                        .unwrap()
                        .last_mut()
                        .unwrap()["value"] = json!("different_reference");
                }
                "missing_money" => {
                    order["customAttributes"].as_array_mut().unwrap().remove(5);
                }
                "duplicate_money" => {
                    let attribute = order["customAttributes"][5].clone();
                    order["customAttributes"]
                        .as_array_mut()
                        .unwrap()
                        .push(attribute);
                }
                _ => {}
            }
            let count = steps.len();
            let server = Server::start(steps).await;
            let response = server
                .client()
                .await
                .send(authed("GET", URI, fixture.root_id(), AdminRole::Admin))
                .await
                .unwrap();
            let status = response.status();
            let report = body_json(response).await;
            if matches!(mode, "missing_money" | "duplicate_money") {
                assert_eq!(status, 502, "{mode}: {report}");
            } else {
                assert_eq!(status, 200, "{mode}: {report}");
                let provider = if stripe { "stripe" } else { "paypal" };
                let row = &report["transactions"][provider][0];
                assert_eq!(row["gross_paid"], 12.5);
                assert_eq!(row["currency"], if stripe { "THB" } else { "SGD" });
                if mode == "valid" {
                    assert_eq!(row["shopify_order_id"], if stripe { 123 } else { 124 });
                    assert_eq!(row["shopify_order_tags"], json!(["independent-convention"]));
                    assert_eq!(row["shipping_fee"], 0.0);
                    assert_eq!(report["items"].as_array().unwrap().len(), 1);
                    let item = &report["items"][0];
                    assert_eq!(item["price"], 4.0);
                    assert_eq!(item["currency"], if stripe { "AUD" } else { "USD" });
                    assert_eq!(item["payment_reference"], row["payment_reference"]);
                } else {
                    assert_eq!(row["shopify_order_id"], 0, "{mode}: {report}");
                    assert_eq!(report["items"], json!([]));
                }
            }
            assert_eq!(server.finish().await.len(), count);
        }
    }
}

fn independent_accounting_flow(stripe: bool) -> Vec<Step> {
    let mut steps = flow(stripe, !stripe, false, Rows::Populated);
    steps.retain(|step| !matches!(step.path, "/refunds/re_123"));
    let payment_currency = if stripe { "THB" } else { "SGD" };
    let shop_currency = if stripe { "AUD" } else { "USD" };
    for step in &mut steps {
        let Reply::Json(body) = &mut step.reply else {
            unreachable!()
        };
        match step.path {
            "/balance_transactions" => {
                let mut value = balance(false);
                value["amount"] = json!(1250);
                value["net"] = json!(1200);
                value["fee"] = json!(50);
                value["fee_details"] = json!([{"amount":50,"currency":"thb","type":"stripe_fee"}]);
                value["exchange_rate"] = json!(1.0);
                body["data"] = json!([value]);
            }
            "/charges/ch_123" => {
                body["amount_captured"] = json!(1250);
                body["amount"] = json!(1250);
                body["status"] = json!("succeeded");
                body["amount_refunded"] = json!(0);
                body["currency"] = json!("thb");
            }
            "/v1/reporting/transactions" => {
                body["transaction_details"] = if stripe {
                    json!([])
                } else {
                    json!([detail(
                        "pp_sale",
                        "T0006",
                        "2026-10-01T18:36:07Z",
                        "SGD",
                        "12.50",
                        Some("-0.50")
                    )])
                };
            }
            "/graphql.json" => {
                let mut value = order(stripe);
                value["tags"] = json!(["independent-convention"]);
                value["displayFinancialStatus"] = json!("PAID");
                value["cancelledAt"] = Value::Null;
                let total = json!({"shopMoney":{"amount":"4.00","currencyCode":shop_currency},"presentmentMoney":{"amount":"4.00","currencyCode":shop_currency}});
                value["currentTotalPriceSet"] = total.clone();
                value["totalOutstandingSet"] = json!({"shopMoney":{"amount":"0.00","currencyCode":shop_currency},"presentmentMoney":{"amount":"0.00","currencyCode":shop_currency}});
                value["transactions"][0]["amountSet"] = total.clone();
                value["transactions"][0]["gateway"] = json!("manual");
                value["transactions"][0]["kind"] = json!("SALE");
                value["transactions"][0]["receiptJson"] = Value::Null;
                value["lineItems"]["nodes"][0]["quantity"] = json!(1);
                value["lineItems"]["nodes"][0]["originalUnitPriceSet"] = total;
                value["shippingLines"] = page(vec![], None);
                value["refunds"] = json!([]);
                let mut attributes = vec![
                    json!({"key":"checkout_correlation_version","value":"3"}),
                    json!({"key":"checkout_id","value":"11111111-1111-4111-8111-111111111111"}),
                    json!({"key":"convention_id","value":"22222222-2222-4222-8222-222222222222"}),
                    json!({"key":"convention_slug","value":"independent-convention"}),
                    json!({"key":"payment_provider","value":if stripe {"stripe"} else {"paypal"}}),
                    json!({"key":"payment_amount_minor","value":"1250"}),
                    json!({"key":"payment_currency","value":payment_currency}),
                    json!({"key":"shopify_amount_minor","value":"400"}),
                    json!({"key":"shopify_currency","value":shop_currency}),
                ];
                if stripe {
                    attributes.extend([
                        json!({"key":"stripe_checkout_session_id","value":"cs_accounting"}),
                        json!({"key":"stripe_payment_intent_id","value":"pi_accounting"}),
                        json!({"key":"stripe_charge_id","value":"ch_123"}),
                    ]);
                } else {
                    attributes.extend([
                        json!({"key":"paypal_order_id","value":"order_accounting"}),
                        json!({"key":"paypal_capture_id","value":"pp_sale"}),
                    ]);
                }
                value["customAttributes"] = json!(attributes);
                body["data"]["orders"] = page(vec![value], None);
            }
            _ => {}
        }
    }
    steps
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
    search_bounds: Option<(&'static str, &'static str)>,
}
impl Step {
    fn json(path: &'static str, value: Value) -> Self {
        Self {
            path,
            query: BTreeMap::new(),
            reply: Reply::Json(value),
            cursor: None,
            search_bounds: None,
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
pub(crate) struct Server {
    pub(crate) url: String,
    task: Option<JoinHandle<()>>,
    requests: Arc<Mutex<Vec<String>>>,
    stop: Option<oneshot::Sender<()>>,
}
impl Server {
    pub(crate) async fn accounting(failure: Option<(&str, &'static str)>) -> (Self, usize) {
        let mut steps = flow(true, true, false, Rows::Populated);
        for step in &mut steps {
            if step.path == "/v1/reporting/transactions" {
                step.query
                    .insert("end_date".into(), "2026-10-31T16:59:59.999999999Z".into());
            }
            if step.path == "/graphql.json" {
                step.search_bounds = Some((
                    "2026-10-01T00:00:00+07:00",
                    "2026-10-31T23:59:59.999999999+07:00",
                ));
            }
        }

        if let Some((operation, fault)) = failure {
            let stage = steps
                .iter()
                .position(|step| step.path == operation)
                .unwrap();
            steps.truncate(stage + 1);
            steps[stage].reply = Reply::Fault(fault);
        }
        let calls = steps.len() - usize::from(failure.is_some_and(|(_, fault)| fault == "refused"));
        (Self::start(steps).await, calls)
    }

    async fn start(steps: Vec<Step>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let recorded = requests.clone();
        let (stop, mut stopped) = oneshot::channel();
        let task = tokio::spawn(async move {
            if steps.is_empty() {
                loop {
                    tokio::select! {
                        _ = &mut stopped => break,
                        accepted = listener.accept() => {
                            let (mut stream, _) = accepted.unwrap();
                            recorded.lock().unwrap().push("unexpected upstream connection".into());
                            let _ = stream.write_all(b"HTTP/1.1 500 Fixture\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await;
                        }
                    }
                }
                return;
            }
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
                        let (start, end) = step
                            .search_bounds
                            .unwrap_or(("2026-10-01T00:00:00+07:00", "2026-10-31T23:59:59+07:00"));
                        let expected = format!(
                            "(created_at:>='{start}' AND created_at:<='{end}') OR (updated_at:>='{start}' AND updated_at:<='{end}')"
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
            stop: Some(stop),
        }
    }
    async fn client(&self) -> crate::common::TestClient {
        let graphql = format!("{}/graphql.json", self.url);
        client_with_providers(ProviderConfig {
            shopify_url: Some(&graphql),
            stripe_url: Some(&self.url),
            paypal_url: Some(&self.url),
            ..Default::default()
        })
        .await
    }
    pub(crate) async fn finish(mut self) -> Vec<String> {
        let _ = self.stop.take().unwrap().send(());
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

enum Rows {
    Empty,
    Populated,
}

fn flow(stripe: bool, paypal: bool, paginated: bool, rows: Rows) -> Vec<Step> {
    let empty = matches!(rows, Rows::Empty);
    let mut steps = Vec::new();
    {
        let stripe_empty = empty || !stripe;
        let payouts = if stripe_empty {
            vec![]
        } else {
            vec![json!({"id":"po_123","arrival_date":1_790_985_600})]
        };
        steps.push(
            Step::json(
                "/payouts",
                json!({"data":payouts,"has_more":paginated && !stripe_empty}),
            )
            .query(&[
                ("arrival_date[gte]", "1790787600"),
                ("arrival_date[lte]", "1793465999"),
                ("status", "paid"),
                ("limit", "100"),
            ]),
        );
        if !stripe_empty {
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
    {
        let paypal_empty = empty || !paypal;
        steps.push(Step::json(
            "/v1/oauth2/token",
            json!({"access_token":"e2e-token","token_type":"Bearer","expires_in":3600}),
        ));
        let mut aud = detail(
            "aud_sale",
            "T0006",
            "2026-10-01T18:36:07Z",
            "AUD",
            "53.08",
            Some("-2.82"),
        );
        aud["transaction_info"]["shipping_amount"] = json!({"currency_code":"AUD","value":"10.08"});
        let mut first = if paypal_empty {
            vec![]
        } else {
            vec![
                detail(
                    "pp_sale",
                    "T0006",
                    "2026-09-30T20:00:00Z",
                    "THB",
                    "100",
                    Some("-2"),
                ),
                detail(
                    "usd_sale",
                    "T0006",
                    "2026-09-15T00:00:00Z",
                    "USD",
                    "10",
                    None,
                ),
                aud,
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
        let second = if paypal_empty {
            vec![]
        } else {
            vec![
                detail(
                    "boundary",
                    "T0006",
                    "2026-10-01T17:00:00Z",
                    "THB",
                    "40",
                    None,
                ),
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
        if !paginated {
            first.extend(second.clone());
        }
        steps.push(report_step(
            "2026-09-30T17:00:00Z",
            "2026-10-31T16:59:59Z",
            1,
            if paginated && !paypal_empty { 2 } else { 1 },
            first,
        ));
        if paginated && !paypal_empty {
            steps.push(report_step(
                "2026-09-30T17:00:00Z",
                "2026-10-31T16:59:59Z",
                2,
                2,
                second,
            ));
        }
    }
    if !empty {
        let first = Step::json(
            "/graphql.json",
            json!({"data":{"orders":page(if paginated {vec![order(true)]} else {[stripe.then(||order(true)),paypal.then(||order(false))].into_iter().flatten().collect()},if paginated {Some("order-1")} else {None})}}),
        );
        steps.push(first);
        if paginated {
            let mut second = Step::json(
                "/graphql.json",
                json!({"data":{"orders":page(vec![order(true),order(false)],None)}}),
            );
            second.cursor = Some("order-1");
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
    json!({"id":if stripe {"gid://shopify/Order/123"} else {"gid://shopify/Order/124"},"name":"#9876","number":9876,"createdAt":"2026-10-01T00:00:00Z","displayFulfillmentStatus":"UNFULFILLED","tags":["Native", "comma,quote\"", "ไทย"],
        "shippingAddress":{"name":"Buyer","address1":"1 Road","city":"Bangkok","zip":"10000","country":"Thailand","phone":"+66 81-234-5678"},
        "currentTotalPriceSet":{"shopMoney":{"amount":"100","currencyCode":currency}},
        "transactions":[transaction(false)],"lineItems":page(vec![line.clone()],None),
        "shippingLines":page(vec![json!({"priceSet":{"shopMoney":{"amount":if stripe {"5"} else {"10"},"currencyCode":currency}}})],None),
        "refunds":[{"transactions":page(vec![transaction(true)],None),"refundShippingLines":page(vec![json!({"subtotalAmountSet":{"shopMoney":{"amount":if stripe {"1"} else {"2"},"currencyCode":currency}}})],None),"refundLineItems":page(vec![json!({"quantity":1,"subtotalSet":{"shopMoney":{"amount":if stripe {"10"} else {"20"},"currencyCode":currency}},"lineItem":line})],None)}]})
}
