#![allow(clippy::too_many_lines)]

mod providers;

use crate::common::{
    ProviderConfig, TestClient, TestFixture, body_json, client_with_providers, fixture,
};
use axum::{body::Body, http::Request};
use providers::Providers;
use serde_json::{Value, json};
use uuid::Uuid;

const SECRET: &str = "abababababababababababababababababababababababababababababababab";
const OTHER_SECRET: &str = "cdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcd";
const CHECKOUT: &str = "/api/v1/order/checkout";

async fn configured(providers: &Providers) -> TestClient {
    client_with_providers(ProviderConfig {
        shopify_url: Some(&format!("{}/graphql.json", providers.url)),
        stripe_url: Some(&format!("{}/stripe/", providers.url)),
        paypal_url: Some(&providers.url),
        checkout_enabled: true,
        scheduler_enabled: Some(false),
        ..Default::default()
    })
    .await
}

async fn purchase(fixture: &TestFixture, provider: &str) -> Value {
    let convention = Uuid::new_v4();
    let variant = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO conventions (
             id, slug, name, country_code, event_starts_on, event_ends_on,
             description_markdown, preorder_enabled, currency_code, payment_methods
         )
         VALUES ($1, $2, 'E2E convention', 'SG', '2028-02-29', '2028-02-29',
                 'Synthetic checkout fixture', TRUE, 'SGD', ARRAY['stripe', 'paypal'])",
    )
    .bind(convention)
    .bind(format!("{}-{}", fixture.prefix(), Uuid::new_v4().simple()))
    .execute(&fixture.pool())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO convention_variants (
             id, convention_id, shopify_product_id, shopify_variant_id,
             final_price, product_title, variant_title, sku
         )
         VALUES ($1, $2, 'gid://shopify/Product/1', 'gid://shopify/ProductVariant/2',
                 20.00, 'Synthetic product', 'Blue', 'SKU-E2E')",
    )
    .bind(variant)
    .bind(convention)
    .execute(&fixture.pool())
    .await
    .unwrap();
    json!({
        "operation_id": Uuid::new_v4(), "convention_id": convention,
        "email": "buyer@example.test", "provider": provider, "expected_currency": "SGD",
        "items": [{"variant_id": variant, "quantity": 2, "expected_unit_price": 2000}]
    })
}

fn request(path: &str, token: Option<&str>, body: &Value) -> Request<Body> {
    let mut builder = Request::builder()
        .method("POST")
        .uri(path)
        .header("content-type", "application/json");
    if let Some(token) = token {
        builder = builder.header("authorization", format!("Bearer {token}"));
    }
    builder.body(Body::from(body.to_string())).unwrap()
}

fn verify(id: &str, token: Option<&str>) -> Request<Body> {
    let mut builder = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/order/{id}/verify"));
    if let Some(token) = token {
        builder = builder.header("authorization", format!("Bearer {token}"));
    }
    builder.body(Body::empty()).unwrap()
}

async fn accepted(client: &TestClient, input: &Value) -> Value {
    let response = client
        .send(request(CHECKOUT, Some(SECRET), input))
        .await
        .unwrap();
    let status = response.status();
    assert_eq!(response.headers()["cache-control"], "no-store");
    let body = body_json(response).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["checkout_id"], input["operation_id"]);
    assert_eq!(body["money"], json!({"minor":4000,"currency":"SGD"}));
    body
}

#[tokio::test]
async fn checkout_reserves_exact_snapshot_and_retries_without_new_payment_for_both_gateways() {
    let fixture = fixture().await;
    for provider in ["stripe", "paypal"] {
        let upstream = Providers::start().await;
        let client = configured(&upstream).await;
        let input = purchase(&fixture, provider).await;
        let first = accepted(&client, &input).await;
        assert_eq!(first["status"], "requires_action");
        assert_eq!(first["next_action"]["type"], "redirect");
        assert!(first.get("order").is_none());
        assert!(first["reservation_expires_at"].as_str().is_some());
        let before = upstream.calls();
        // Discarding the first response models a caller that retries before saving it.
        let retried = accepted(&client, &input).await;
        assert_eq!(retried, first);
        assert_eq!(upstream.payment_creates(), 1);
        let id = input["operation_id"].as_str().unwrap();
        let stored: (Value, String, String, String, Vec<u8>) = sqlx::query_as(
            "SELECT snapshot, draft_state, payment_state, draft_id, guest_secret_hash
             FROM checkouts
             WHERE id = $1",
        )
        .bind(Uuid::parse_str(id).unwrap())
        .fetch_one(&fixture.pool())
        .await
        .unwrap();
        assert_eq!(stored.0["money"], first["money"]);
        assert_eq!(stored.0["items"][0]["quantity"], 2);
        assert_eq!(stored.0["items"][0]["unit_price_minor"], 2000);
        assert_eq!(stored.1, "reserved");
        assert_eq!(stored.2, "requires_action");
        assert_eq!(stored.4.len(), 32);
        assert_ne!(stored.4, hex::decode(SECRET).unwrap());
        assert_eq!(upstream.count("CreateDraftOrder"), 1);
        assert_eq!(upstream.count("CheckoutDraft"), 1);
        assert_eq!(upstream.payment_creates(), 1);
        let create_index = before
            .iter()
            .position(|v| v["operation"] == "CreateDraftOrder")
            .unwrap();
        let read_index = before
            .iter()
            .position(|v| v["operation"] == "CheckoutDraft")
            .unwrap();
        let payment_index = before
            .iter()
            .position(|v| v["payment_create"] == true)
            .unwrap();
        assert!(create_index < read_index && read_index < payment_index);
        let draft_input = &before[create_index]["body"]["variables"]["input"];
        assert_eq!(draft_input["email"], "buyer@example.test");
        assert_eq!(draft_input["lineItems"][0]["quantity"], 2);
        assert_eq!(
            draft_input["lineItems"][0]["priceOverride"]["amount"],
            "20.00"
        );
        assert_eq!(draft_input["presentmentCurrencyCode"], "SGD");
        upstream.assert_navigation_and_private_data(id, SECRET);
        let response = client.send(verify(id, Some(SECRET))).await.unwrap();
        assert_eq!(response.status(), 200);
        assert_eq!(response.headers()["cache-control"], "no-store");
        assert_ne!(body_json(response).await["status"], "completed");
        assert_eq!(upstream.count("CompleteCheckoutDraft"), 0);
        assert_eq!(upstream.payment_creates(), 1);
    }
}

#[tokio::test]
async fn checkout_guest_authentication_and_immutable_operation_ownership() {
    let fixture = fixture().await;
    let upstream = Providers::start().await;
    let client = configured(&upstream).await;
    let input = purchase(&fixture, "stripe").await;
    for token in [
        None,
        Some(""),
        Some("abcd"),
        Some("zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz"),
    ] {
        let response = client.send(request(CHECKOUT, token, &input)).await.unwrap();
        assert_eq!(response.status(), 401);
        assert_eq!(response.headers()["www-authenticate"], "Bearer");
    }
    assert_eq!(upstream.calls(), Vec::<Value>::new());
    accepted(&client, &input).await;
    let id = input["operation_id"].as_str().unwrap();
    let count = upstream.calls().len();
    for response in [
        client
            .send(request(CHECKOUT, Some(OTHER_SECRET), &input))
            .await
            .unwrap(),
        client.send(verify(id, None)).await.unwrap(),
        client.send(verify(id, Some(OTHER_SECRET))).await.unwrap(),
    ] {
        assert_eq!(response.status(), 401);
        assert!(body_json(response).await.get("checkout_id").is_none());
    }
    let mut changed = input.clone();
    changed["email"] = json!("other@example.test");
    assert_eq!(
        client
            .send(request(CHECKOUT, Some(SECRET), &changed))
            .await
            .unwrap()
            .status(),
        409
    );
    sqlx::query(
        "UPDATE checkouts
         SET owner_expires_at = CURRENT_TIMESTAMP - INTERVAL '1 second'
         WHERE id = $1",
    )
    .bind(Uuid::parse_str(id).unwrap())
    .execute(&fixture.pool())
    .await
    .unwrap();
    assert_eq!(
        client
            .send(verify(id, Some(SECRET)))
            .await
            .unwrap()
            .status(),
        401
    );
    assert_eq!(upstream.calls().len(), count);
}

#[tokio::test]
async fn checkout_rejects_each_invalid_input_without_business_dependency_calls() {
    let fixture = fixture().await;
    let upstream = Providers::start().await;
    let client = configured(&upstream).await;
    let input = purchase(&fixture, "stripe").await;
    let mut cases = Vec::new();
    for field in [
        "operation_id",
        "convention_id",
        "email",
        "provider",
        "items",
        "expected_currency",
    ] {
        let mut body = input.clone();
        body.as_object_mut().unwrap().remove(field);
        cases.push((format!("missing {field}"), body, 400));
    }
    for (field, value, status) in [
        ("operation_id", json!("invalid"), 400),
        ("convention_id", json!("invalid"), 400),
        ("email", json!(""), 400),
        ("email", json!("bad-address"), 400),
        ("email", json!("x".repeat(255) + "@example.test"), 400),
        ("provider", json!("other"), 400),
        ("expected_currency", json!("sgd"), 400),
        ("expected_currency", json!("SG"), 400),
        ("items", json!([]), 400),
        ("extra", json!(true), 400),
    ] {
        let mut body = input.clone();
        body[field] = value;
        cases.push((field.to_owned(), body, status));
    }
    for (field, value, status) in [
        ("variant_id", json!("invalid"), 400),
        ("quantity", json!(0), 400),
        ("quantity", json!(1000), 400),
        ("quantity", json!(-1), 400),
        ("quantity", json!(1.5), 400),
        ("expected_unit_price", json!(0), 400),
        ("expected_unit_price", json!(-1), 400),
        ("expected_unit_price", json!("2000"), 400),
    ] {
        let mut body = input.clone();
        body["items"][0][field] = value;
        cases.push((field.to_owned(), body, status));
    }
    for field in ["variant_id", "quantity", "expected_unit_price"] {
        let mut body = input.clone();
        body["items"][0].as_object_mut().unwrap().remove(field);
        cases.push((format!("missing line {field}"), body, 400));
    }
    for email in [
        " buyer@example.test",
        "buyer@example.test ",
        "@example.test",
        ".buyer@example.test",
        "buyer.@example.test",
        "buy..er@example.test",
        "buyer@",
        "buyer@.test",
        "buyer@example.",
        "buyer@localhost",
        "buyer@@example.test",
        "buy er@example.test",
    ] {
        let mut body = input.clone();
        body["email"] = json!(email);
        cases.push((format!("invalid email {email}"), body, 400));
    }
    for field in ["operation_id", "convention_id"] {
        let mut body = input.clone();
        body[field] = json!(Uuid::nil());
        cases.push((format!("nil {field}"), body, 400));
    }
    let mut duplicate = input.clone();
    duplicate["items"] = json!([input["items"][0], input["items"][0]]);
    cases.push(("duplicate variants".into(), duplicate, 400));
    let mut too_many = input.clone();
    too_many["items"] = Value::Array(
        (0..101)
            .map(|_| {
                let mut item = input["items"][0].clone();
                item["variant_id"] = json!(Uuid::new_v4());
                item
            })
            .collect(),
    );
    cases.push(("over 100 lines".into(), too_many, 400));
    for (label, body, status) in cases {
        let response = client
            .send(request(CHECKOUT, Some(SECRET), &body))
            .await
            .unwrap();
        let got = response.status();
        assert_eq!(got, status, "{label}: {}", body_json(response).await);
        assert_eq!(upstream.calls(), Vec::<Value>::new(), "{label}");
    }
    for uri in [
        "/api/v1/order/not-a-uuid/verify",
        "/api/v1/order/00000000-0000-0000-0000-000000000000/verify?paid=true",
    ] {
        let response = client
            .send(
                Request::builder()
                    .method("POST")
                    .uri(uri)
                    .header("authorization", format!("Bearer {SECRET}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), 400);
    }
    assert_eq!(upstream.calls(), Vec::<Value>::new());
    let id = Uuid::new_v4();
    let response = client
        .send(request(
            &format!("/api/v1/order/{id}/verify"),
            Some(SECRET),
            &json!({"paid":true}),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 400);
    let response = client
        .send(request(
            &format!("{CHECKOUT}?provider=stripe"),
            Some(SECRET),
            &input,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 400);
    assert_eq!(
        client
            .send(verify(&id.to_string(), Some(SECRET)))
            .await
            .unwrap()
            .status(),
        401
    );
    assert_eq!(upstream.calls(), Vec::<Value>::new());
}

fn stripe_webhook(event: &Value, timestamp: i64) -> Request<Body> {
    use hmac::{Hmac, KeyInit, Mac};
    let raw = event.to_string();
    let mut signature = Hmac::<sha2::Sha256>::new_from_slice(b"whsec_e2e").unwrap();
    signature.update(format!("{timestamp}.{raw}").as_bytes());
    Request::builder()
        .method("POST")
        .uri("/api/v1/order/webhook/stripe")
        .header(
            "stripe-signature",
            format!(
                "t={timestamp},v1={}",
                hex::encode(signature.finalize().into_bytes())
            ),
        )
        .body(Body::from(raw))
        .unwrap()
}

fn paypal_webhook(event: &Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/api/v1/order/webhook/paypal")
        .header("paypal-auth-algo", "SHA256withRSA")
        .header(
            "paypal-cert-url",
            "https://api.sandbox.paypal.com/cert/synthetic",
        )
        .header("paypal-transmission-id", "transmission_e2e")
        .header("paypal-transmission-sig", "synthetic_signature")
        .header("paypal-transmission-time", chrono::Utc::now().to_rfc3339())
        .body(Body::from(event.to_string()))
        .unwrap()
}

#[tokio::test]
async fn checkout_paid_return_and_authenticated_duplicate_webhook_complete_same_draft_once() {
    let fixture = fixture().await;
    for provider in ["stripe", "paypal"] {
        let upstream = Providers::start().await;
        let client = configured(&upstream).await;
        let input = purchase(&fixture, provider).await;
        accepted(&client, &input).await;
        upstream.pay();
        let id = input["operation_id"].as_str().unwrap();
        let event_id = format!("evt_{}", Uuid::new_v4().simple());
        let event = if provider == "stripe" {
            upstream.stripe_event(id, &event_id)
        } else {
            upstream.paypal_event(id, &event_id)
        };
        let callback = || {
            if provider == "stripe" {
                stripe_webhook(&event, chrono::Utc::now().timestamp())
            } else {
                paypal_webhook(&event)
            }
        };
        let (verified, delivered) = tokio::join!(
            client.send(verify(id, Some(SECRET))),
            client.send(callback())
        );
        assert_eq!(verified.unwrap().status(), 200);
        let delivered = delivered.unwrap();
        assert!(matches!(delivered.status().as_u16(), 200 | 503));
        let response = client.send(verify(id, Some(SECRET))).await.unwrap();
        assert_eq!(response.status(), 200);
        let result = body_json(response).await;
        assert_eq!(result["status"], "completed", "{provider}: {result}");
        assert_eq!(result["order"]["name"], "#E2E-ORDER");
        assert!(
            result["order"]["id"]
                .as_str()
                .unwrap()
                .starts_with("gid://shopify/Order/")
        );
        assert!(result["next_action"].is_null());
        let completes = upstream.count("CompleteCheckoutDraft");
        assert_eq!(completes, 1);
        assert_eq!(upstream.payment_creates(), 1);
        let callback_response = client.send(callback()).await.unwrap();
        assert_eq!(
            callback_response.status(),
            200,
            "{}",
            body_json(callback_response).await
        );
        assert_eq!(client.send(callback()).await.unwrap().status(), 200);
        assert_eq!(upstream.count("CompleteCheckoutDraft"), completes);
        if provider == "paypal" {
            assert_eq!(
                upstream
                    .calls()
                    .iter()
                    .filter(|v| v["path"]
                        .as_str()
                        .is_some_and(|path| path.ends_with("/capture")))
                    .count(),
                1
            );
        }
        let persisted: (String, String, String, bool) = sqlx::query_as(
            "SELECT checkout.payment_state, checkout.completion_state, checkout.order_name,
                    event.processed_at IS NOT NULL
             FROM checkouts checkout
             JOIN checkout_events event ON event.checkout_id = checkout.id
             WHERE checkout.id = $1
               AND event.event_id = $2",
        )
        .bind(Uuid::parse_str(id).unwrap())
        .bind(&event_id)
        .fetch_one(&fixture.pool())
        .await
        .unwrap();
        assert_eq!(
            persisted,
            ("paid".into(), "completed".into(), "#E2E-ORDER".into(), true)
        );
        let logs = client.logs();
        assert!(!logs.contains(SECRET));
        assert!(!logs.contains("buyer@example.test"));
    }
}

#[tokio::test]
async fn checkout_paid_shopify_failure_preserves_payment_and_retries_same_order() {
    let fixture = fixture().await;
    for provider in ["stripe", "paypal"] {
        let upstream = Providers::start().await;
        let client = configured(&upstream).await;
        let input = purchase(&fixture, provider).await;
        accepted(&client, &input).await;
        upstream.pay();
        upstream.fault("EditOrderTags", "http");
        let id = input["operation_id"].as_str().unwrap();
        let response = client.send(verify(id, Some(SECRET))).await.unwrap();
        assert_eq!(response.status(), 200);
        assert_eq!(
            body_json(response).await["status"],
            "reconciliation_required"
        );
        let paid: String =
            sqlx::query_scalar("SELECT payment_state\nFROM checkouts\nWHERE id = $1")
                .bind(Uuid::parse_str(id).unwrap())
                .fetch_one(&fixture.pool())
                .await
                .unwrap();
        assert_eq!(paid, "paid");
        assert_eq!(upstream.count("CompleteCheckoutDraft"), 1);
        upstream.clear_fault();
        let response = client.send(verify(id, Some(SECRET))).await.unwrap();
        assert_eq!(response.status(), 200);
        assert_eq!(body_json(response).await["status"], "completed");
        assert_eq!(upstream.count("CompleteCheckoutDraft"), 1);
        assert_eq!(upstream.payment_creates(), 1);
    }
}

#[tokio::test]
async fn checkout_inventory_and_exact_reservation_failures_stop_before_payment() {
    let fixture = fixture().await;
    for case in [
        "last_unit",
        "untracked",
        "continue_selling",
        "no_hold",
        "total_mismatch",
    ] {
        let upstream = Providers::start().await;
        let client = configured(&upstream).await;
        let input = purchase(&fixture, "stripe").await;
        match case {
            "last_unit" => upstream.inventory(1, true, "DENY"),
            "untracked" => upstream.inventory(10, false, "DENY"),
            "continue_selling" => upstream.inventory(10, true, "CONTINUE"),
            "no_hold" => upstream.reservation(false, true),
            _ => upstream.reservation(true, false),
        }
        let response = client
            .send(request(CHECKOUT, Some(SECRET), &input))
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            409,
            "{case}: {}",
            body_json(response).await
        );
        assert_eq!(upstream.payment_creates(), 0);
        assert_eq!(
            upstream.count("CreateDraftOrder"),
            usize::from(matches!(case, "no_hold" | "total_mismatch"))
        );
        if matches!(case, "no_hold" | "total_mismatch") {
            let retry = accepted(&client, &input).await;
            assert_eq!(retry["status"], "reconciliation_required");
            assert_eq!(upstream.count("CreateDraftOrder"), 1);
        }
    }
}

#[tokio::test]
async fn checkout_each_creation_dependency_failure_maps_safely_and_retains_mutation_identity() {
    let fixture = fixture().await;
    for operation in [
        "Location",
        "CatalogueProducts",
        "CreateDraftOrder",
        "CheckoutDraft",
        "/stripe/account",
        "/stripe/checkout/sessions",
        "/v1/oauth2/token",
        "/v2/checkout/orders",
    ] {
        for failure in ["http", "malformed", "missing", "timeout"] {
            let upstream = Providers::start().await;
            let client = configured(&upstream).await;
            let provider = if operation.starts_with("/v") {
                "paypal"
            } else {
                "stripe"
            };
            let input = purchase(&fixture, provider).await;
            upstream.fault(operation, failure);
            let response = client
                .send(request(CHECKOUT, Some(SECRET), &input))
                .await
                .unwrap();
            let status = response.status();
            let body = body_json(response).await;
            assert_eq!(
                status,
                if failure == "timeout" { 504 } else { 502 },
                "{operation}/{failure}: {body}"
            );
            assert!(!body.to_string().contains("private"));
            assert!(body.get("checkout_id").is_none());
            assert_eq!(upstream.count(operation), 1);
            assert!(
                upstream.payment_creates()
                    <= usize::from(matches!(
                        operation,
                        "/stripe/checkout/sessions" | "/v2/checkout/orders"
                    ))
            );
            if matches!(operation, "CreateDraftOrder" | "CheckoutDraft") {
                upstream.clear_fault();
                let retry = accepted(&client, &input).await;
                let known_draft = operation == "CheckoutDraft";
                assert_eq!(
                    retry["status"],
                    if known_draft {
                        "requires_action"
                    } else {
                        "reconciliation_required"
                    }
                );
                assert_eq!(upstream.count("CreateDraftOrder"), 1);
                assert_eq!(upstream.payment_creates(), usize::from(known_draft));
            }
        }
    }
}

#[tokio::test]
async fn checkout_webhook_rejects_invalid_headers_payload_account_and_provider_without_advancement()
{
    let _fixture = fixture().await;
    let upstream = Providers::start().await;
    let client = configured(&upstream).await;
    let id = Uuid::new_v4().to_string();
    let event = upstream.stripe_event(&id, "evt_unknown");
    for provider in ["stripe", "paypal", "unknown"] {
        let response = client
            .send(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/v1/order/webhook/{provider}"))
                    .body(Body::from(event.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), 400);
    }
    for timestamp in [0, chrono::Utc::now().timestamp() - 301] {
        assert_eq!(
            client
                .send(stripe_webhook(&event, timestamp))
                .await
                .unwrap()
                .status(),
            400
        );
    }
    let mut wrong_account = event.clone();
    wrong_account["account"] = json!("acct_other");
    let mut wrong_mode = event.clone();
    wrong_mode["livemode"] = json!(true);
    let mut malformed = event.clone();
    malformed.as_object_mut().unwrap().remove("data");
    for event in [wrong_account, wrong_mode, malformed] {
        assert_eq!(
            client
                .send(stripe_webhook(&event, chrono::Utc::now().timestamp()))
                .await
                .unwrap()
                .status(),
            400
        );
    }
    let mut wrong_signature = stripe_webhook(&event, chrono::Utc::now().timestamp());
    wrong_signature
        .headers_mut()
        .insert("stripe-signature", "t=1,v1=bad".parse().unwrap());
    assert_eq!(client.send(wrong_signature).await.unwrap().status(), 400);
    let mut mixed = stripe_webhook(&event, chrono::Utc::now().timestamp());
    mixed
        .headers_mut()
        .insert("paypal-transmission-id", "mixed".parse().unwrap());
    assert_eq!(client.send(mixed).await.unwrap().status(), 400);
    for provider in ["stripe", "paypal"] {
        let response = client
            .send(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/v1/order/webhook/{provider}"))
                    .body(Body::from(vec![b'x'; 1_048_577]))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), 413);
    }
    assert_eq!(upstream.calls(), Vec::<Value>::new());
    let mut irrelevant = event;
    irrelevant["type"] = json!("invoice.created");
    assert_eq!(
        client
            .send(stripe_webhook(&irrelevant, chrono::Utc::now().timestamp()))
            .await
            .unwrap()
            .status(),
        200
    );
    assert_eq!(upstream.calls(), Vec::<Value>::new());
}

#[tokio::test]
async fn checkout_webhook_early_delivery_and_failed_advancement_remain_retryable() {
    let fixture = fixture().await;
    for provider in ["stripe", "paypal"] {
        let upstream = Providers::start().await;
        let client = configured(&upstream).await;
        let input = purchase(&fixture, provider).await;
        let id = input["operation_id"].as_str().unwrap();
        let event_id = format!("evt_{}", Uuid::new_v4().simple());
        let event = if provider == "stripe" {
            upstream.stripe_event(id, &event_id)
        } else {
            upstream.paypal_event(id, &event_id)
        };
        let callback = || {
            if provider == "stripe" {
                stripe_webhook(&event, chrono::Utc::now().timestamp())
            } else {
                paypal_webhook(&event)
            }
        };
        assert_eq!(client.send(callback()).await.unwrap().status(), 503);
        accepted(&client, &input).await;
        upstream.pay();
        upstream.fault("CheckoutDraft", "http");
        assert_eq!(client.send(callback()).await.unwrap().status(), 503);
        let processed: bool = sqlx::query_scalar(
            "SELECT processed_at IS NOT NULL
             FROM checkout_events
             WHERE checkout_id = $1
               AND event_id = $2",
        )
        .bind(Uuid::parse_str(id).unwrap())
        .bind(&event_id)
        .fetch_one(&fixture.pool())
        .await
        .unwrap();
        assert!(!processed);
        upstream.clear_fault();
        // Read failures before completion leave the paid draft safe to retry.
        let response = client.send(verify(id, Some(SECRET))).await.unwrap();
        assert_eq!(response.status(), 200);
        assert_eq!(body_json(response).await["status"], "completed");
        assert_eq!(upstream.count("CreateDraftOrder"), 1);
        assert_eq!(upstream.payment_creates(), 1);
        assert_eq!(upstream.count("CompleteCheckoutDraft"), 1);
    }
}

#[tokio::test]
async fn checkout_verify_and_paypal_signature_dependency_failures_are_safe() {
    let fixture = fixture().await;
    for provider in ["stripe", "paypal"] {
        for failure in ["http", "malformed", "timeout"] {
            let upstream = Providers::start().await;
            let client = configured(&upstream).await;
            let input = purchase(&fixture, provider).await;
            accepted(&client, &input).await;
            let id = input["operation_id"].as_str().unwrap();
            let operation = if provider == "stripe" {
                "/stripe/account"
            } else {
                "/v1/notifications/verify-webhook-signature"
            };
            upstream.fault(operation, failure);
            let response = if provider == "stripe" {
                client.send(verify(id, Some(SECRET))).await.unwrap()
            } else {
                client
                    .send(paypal_webhook(&upstream.paypal_event(id, "evt_auth_fault")))
                    .await
                    .unwrap()
            };
            assert_eq!(
                response.status(),
                if failure == "timeout" { 504 } else { 502 }
            );
            assert_eq!(
                body_json(response).await,
                json!({"error":"Upstream request failed"})
            );
            assert_eq!(upstream.count("CompleteCheckoutDraft"), 0);
            assert_eq!(upstream.payment_creates(), 1);
        }
    }
}

#[tokio::test]
async fn checkout_owned_row_database_faults_rollback_and_return_safe_errors() {
    let fixture = fixture().await;
    let upstream = Providers::start().await;
    let client = configured(&upstream).await;
    let input = purchase(&fixture, "stripe").await;
    let id = input["operation_id"].as_str().unwrap();
    let prefix = fixture.prefix();
    // TestFixture::drop removes these prefix-owned controls before deleting rows.
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "CREATE FUNCTION {prefix}_checkout_fault() RETURNS TRIGGER
         LANGUAGE plpgsql AS $$
         BEGIN
             IF NEW.id = '{id}'::uuid THEN
                 RAISE EXCEPTION 'Synthetic owned checkout write failure';
             END IF;
             RETURN NEW;
         END;
         $$;
         CREATE TRIGGER {prefix}_checkout_fault
             BEFORE INSERT ON checkouts
             FOR EACH ROW
             EXECUTE FUNCTION {prefix}_checkout_fault();"
    )))
    .execute(&fixture.pool())
    .await
    .unwrap();
    let response = client
        .send(request(CHECKOUT, Some(SECRET), &input))
        .await
        .unwrap();
    assert_eq!(response.status(), 500);
    assert_eq!(
        body_json(response).await,
        json!({"error":"Internal server error"})
    );
    assert_eq!(upstream.calls(), Vec::<Value>::new());
    let rows: i64 = sqlx::query_scalar("SELECT COUNT(*)\nFROM checkouts\nWHERE id = $1")
        .bind(Uuid::parse_str(id).unwrap())
        .fetch_one(&fixture.pool())
        .await
        .unwrap();
    assert_eq!(rows, 0);
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "DROP TRIGGER {prefix}_checkout_fault ON checkouts;
         CREATE TRIGGER {prefix}_checkout_fault
             BEFORE UPDATE ON checkouts
             FOR EACH ROW
             EXECUTE FUNCTION {prefix}_checkout_fault();"
    )))
    .execute(&fixture.pool())
    .await
    .unwrap();
    let response = client
        .send(request(CHECKOUT, Some(SECRET), &input))
        .await
        .unwrap();
    assert_eq!(response.status(), 500);
    assert_eq!(upstream.calls(), Vec::<Value>::new());
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "DROP TRIGGER {prefix}_checkout_fault ON checkouts;
         DROP FUNCTION {prefix}_checkout_fault();"
    )))
    .execute(&fixture.pool())
    .await
    .unwrap();
    accepted(&client, &input).await;
    let calls = upstream.calls().len();
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "CREATE FUNCTION {prefix}_checkout_event_fault() RETURNS TRIGGER
         LANGUAGE plpgsql AS $$
         BEGIN
             IF NEW.checkout_id = '{id}'::uuid THEN
                 RAISE EXCEPTION 'Synthetic owned webhook receipt failure';
             END IF;
             RETURN NEW;
         END;
         $$;
         CREATE TRIGGER {prefix}_checkout_event_fault
             BEFORE INSERT ON checkout_events
             FOR EACH ROW
             EXECUTE FUNCTION {prefix}_checkout_event_fault();"
    )))
    .execute(&fixture.pool())
    .await
    .unwrap();
    let event = upstream.stripe_event(id, "evt_database_fault");
    let response = client
        .send(stripe_webhook(&event, chrono::Utc::now().timestamp()))
        .await
        .unwrap();
    assert_eq!(response.status(), 500);
    assert_eq!(
        body_json(response).await,
        json!({"error":"Internal server error"})
    );
    assert_eq!(upstream.calls().len(), calls);
    let rows: i64 =
        sqlx::query_scalar("SELECT COUNT(*)\nFROM checkout_events\nWHERE checkout_id = $1")
            .bind(Uuid::parse_str(id).unwrap())
            .fetch_one(&fixture.pool())
            .await
            .unwrap();
    assert_eq!(rows, 0);
}

#[tokio::test]
async fn checkout_each_payment_evidence_and_capture_failure_stops_fulfillment() {
    let fixture = fixture().await;
    for (provider, operation) in [
        ("stripe", "StripeSession"),
        ("stripe", "StripeIntent"),
        ("stripe", "StripeCharge"),
        ("paypal", "PaypalOrder"),
        ("paypal", "PaypalCaptureOrder"),
        ("paypal", "PaypalCapture"),
    ] {
        for failure in ["http", "malformed", "missing", "timeout"] {
            let upstream = Providers::start().await;
            let client = configured(&upstream).await;
            let input = purchase(&fixture, provider).await;
            accepted(&client, &input).await;
            upstream.pay();
            upstream.fault(operation, failure);
            let id = input["operation_id"].as_str().unwrap();
            let response = client.send(verify(id, Some(SECRET))).await.unwrap();
            assert_eq!(
                response.status(),
                if failure == "timeout" { 504 } else { 502 },
                "{operation}/{failure}: {}",
                body_json(response).await
            );
            assert_eq!(upstream.count("CompleteCheckoutDraft"), 0);
            assert_eq!(upstream.count("CreateDraftOrder"), 1);
            assert_eq!(upstream.payment_creates(), 1);
            if operation == "PaypalCaptureOrder" {
                let requests = upstream.calls();
                let capture = requests
                    .iter()
                    .find(|call| {
                        call["path"]
                            .as_str()
                            .is_some_and(|path| path.ends_with("/capture"))
                    })
                    .unwrap();
                assert_eq!(capture["paypal_request_id"], format!("capture-{id}"));
                assert_eq!(capture["body"], json!({}));
            }
        }
    }
}

#[tokio::test]
async fn checkout_provider_connection_failure_preserves_unstarted_intent() {
    let fixture = fixture().await;
    let mut upstream = Providers::start().await;
    let client = configured(&upstream).await;
    let input = purchase(&fixture, "stripe").await;
    upstream.stop().await;
    let response = client
        .send(request(CHECKOUT, Some(SECRET), &input))
        .await
        .unwrap();
    assert_eq!(response.status(), 502);
    assert_eq!(
        body_json(response).await,
        json!({"error":"Upstream request failed"})
    );
    let stored: (String, String) = sqlx::query_as(
        "SELECT draft_state, payment_state
         FROM checkouts
         WHERE id = $1",
    )
    .bind(Uuid::parse_str(input["operation_id"].as_str().unwrap()).unwrap())
    .fetch_one(&fixture.pool())
    .await
    .unwrap();
    assert_eq!(stored, ("not_started".into(), "not_started".into()));
}

#[tokio::test]
async fn checkout_paypal_each_authentication_header_and_rejected_signature_fail_closed() {
    let _fixture = fixture().await;
    let upstream = Providers::start().await;
    let client = configured(&upstream).await;
    let event = upstream.paypal_event(&Uuid::new_v4().to_string(), "event_header_fault");
    for header in [
        "paypal-auth-algo",
        "paypal-cert-url",
        "paypal-transmission-id",
        "paypal-transmission-sig",
        "paypal-transmission-time",
    ] {
        let mut request = paypal_webhook(&event);
        request.headers_mut().remove(header);
        assert_eq!(client.send(request).await.unwrap().status(), 400);
    }
    for (header, value) in [
        ("paypal-cert-url", "http://example.test/cert"),
        ("paypal-transmission-time", "invalid"),
        ("paypal-auth-algo", ""),
    ] {
        let mut request = paypal_webhook(&event);
        request.headers_mut().insert(header, value.parse().unwrap());
        assert_eq!(client.send(request).await.unwrap().status(), 400);
    }
    let mut mixed = paypal_webhook(&event);
    mixed
        .headers_mut()
        .insert("stripe-signature", "mixed".parse().unwrap());
    assert_eq!(client.send(mixed).await.unwrap().status(), 400);
    assert_eq!(upstream.calls(), Vec::<Value>::new());
    upstream.fault(
        "/v1/notifications/verify-webhook-signature",
        "rejected_signature",
    );
    assert_eq!(
        client.send(paypal_webhook(&event)).await.unwrap().status(),
        400
    );
    assert_eq!(
        upstream.count("/v1/notifications/verify-webhook-signature"),
        1
    );
    assert_eq!(upstream.count("CreateDraftOrder"), 0);
}

#[tokio::test]
async fn checkout_lost_draft_and_provider_mutation_responses_preserve_operation_identity() {
    let fixture = fixture().await;
    for (provider, operation) in [
        ("stripe", "CreateDraftOrder"),
        ("stripe", "/stripe/checkout/sessions"),
        ("paypal", "/v2/checkout/orders"),
    ] {
        let upstream = Providers::start().await;
        let client = configured(&upstream).await;
        let input = purchase(&fixture, provider).await;
        upstream.fault(operation, "lost");
        let response = client
            .send(request(CHECKOUT, Some(SECRET), &input))
            .await
            .unwrap();
        assert_eq!(response.status(), 502);
        upstream.clear_fault();
        let retried = accepted(&client, &input).await;
        assert_eq!(upstream.count("CreateDraftOrder"), 1);
        if operation == "CreateDraftOrder" {
            assert_eq!(retried["status"], "reconciliation_required");
            assert_eq!(upstream.payment_creates(), 0);
        } else {
            assert_eq!(retried["status"], "requires_action");
            let calls: Vec<_> = upstream
                .calls()
                .into_iter()
                .filter(|call| call["operation"] == operation)
                .collect();
            assert_eq!(calls.len(), 2);
            let header = if provider == "stripe" {
                "idempotency_key"
            } else {
                "paypal_request_id"
            };
            assert_eq!(calls[0][header], calls[1][header]);
            assert_eq!(
                calls[0][header],
                format!("checkout-{}", input["operation_id"].as_str().unwrap())
            );
            assert_eq!(calls[0]["body"], calls[1]["body"]);
        }
    }
}

#[tokio::test]
async fn checkout_lost_capture_and_completion_responses_reconcile_without_repeating_mutation() {
    let fixture = fixture().await;
    for operation in ["PaypalCaptureOrder", "CompleteCheckoutDraft"] {
        let upstream = Providers::start().await;
        let client = configured(&upstream).await;
        let input = purchase(&fixture, "paypal").await;
        accepted(&client, &input).await;
        upstream.pay();
        upstream.fault(operation, "lost");
        let id = input["operation_id"].as_str().unwrap();
        let response = client.send(verify(id, Some(SECRET))).await.unwrap();
        assert_eq!(
            response.status(),
            if operation == "PaypalCaptureOrder" {
                502
            } else {
                200
            }
        );
        upstream.clear_fault();
        let response = client.send(verify(id, Some(SECRET))).await.unwrap();
        assert_eq!(response.status(), 200);
        assert_eq!(body_json(response).await["status"], "completed");
        assert_eq!(upstream.count("CreateDraftOrder"), 1);
        assert_eq!(upstream.count("CompleteCheckoutDraft"), 1);
        assert_eq!(
            upstream
                .calls()
                .iter()
                .filter(|call| call["path"]
                    .as_str()
                    .is_some_and(|path| path.ends_with("/capture")))
                .count(),
            1
        );
        assert_eq!(upstream.payment_creates(), 1);
    }
}

#[tokio::test]
async fn checkout_restart_and_independent_browser_attempts_keep_their_original_ownership() {
    let fixture = fixture().await;
    let first_upstream = Providers::start().await;
    let second_upstream = Providers::start().await;
    let first_client = configured(&first_upstream).await;
    let second_client = configured(&second_upstream).await;
    let first = purchase(&fixture, "stripe").await;
    let mut second = first.clone();
    second["operation_id"] = json!(Uuid::new_v4());
    let first_response = accepted(&first_client, &first).await;
    let second_response = second_client
        .send(request(CHECKOUT, Some(OTHER_SECRET), &second))
        .await
        .unwrap();
    assert_eq!(second_response.status(), 200);
    assert_ne!(
        body_json(second_response).await["checkout_id"],
        first_response["checkout_id"]
    );
    drop(first_client);
    let restarted = configured(&first_upstream).await;
    let recovered = accepted(&restarted, &first).await;
    assert_eq!(recovered, first_response);
    assert_eq!(first_upstream.count("CreateDraftOrder"), 1);
    assert_eq!(first_upstream.payment_creates(), 1);
    assert_eq!(second_upstream.count("CreateDraftOrder"), 1);
    let second_id = second["operation_id"].as_str().unwrap();
    assert_eq!(
        restarted
            .send(verify(second_id, Some(SECRET)))
            .await
            .unwrap()
            .status(),
        401
    );
    assert_eq!(
        second_client
            .send(verify(second_id, Some(OTHER_SECRET)))
            .await
            .unwrap()
            .status(),
        200
    );
}

#[tokio::test]
async fn checkout_changed_price_currency_and_disabled_selection_never_create_payment() {
    let fixture = fixture().await;
    for case in [
        "price",
        "currency",
        "disabled",
        "provider",
        "missing_variant",
        "missing_convention",
    ] {
        let upstream = Providers::start().await;
        let client = configured(&upstream).await;
        let mut input = purchase(&fixture, "stripe").await;
        let id = Uuid::parse_str(input["convention_id"].as_str().unwrap()).unwrap();
        match case {
            "price" => input["items"][0]["expected_unit_price"] = json!(1999),
            "currency" => input["expected_currency"] = json!("USD"),
            "disabled" => {
                sqlx::query("UPDATE conventions\nSET preorder_enabled = FALSE\nWHERE id = $1")
                    .bind(id)
                    .execute(&fixture.pool())
                    .await
                    .unwrap();
            }
            "provider" => {
                sqlx::query(
                    "UPDATE conventions\nSET payment_methods = ARRAY['paypal']\nWHERE id = $1",
                )
                .bind(id)
                .execute(&fixture.pool())
                .await
                .unwrap();
            }
            "missing_variant" => input["items"][0]["variant_id"] = json!(Uuid::new_v4()),
            _ => input["convention_id"] = json!(Uuid::new_v4()),
        }
        let response = client
            .send(request(CHECKOUT, Some(SECRET), &input))
            .await
            .unwrap();
        assert!(
            matches!(response.status().as_u16(), 404 | 409),
            "{case}: {}",
            body_json(response).await
        );
        assert_eq!(upstream.count("CreateDraftOrder"), 0, "{case}");
        assert_eq!(upstream.payment_creates(), 0, "{case}");
    }
}

#[tokio::test]
async fn checkout_rate_limit_rejects_new_intents_but_allows_owned_retries() {
    let fixture = fixture().await;
    let upstream = Providers::start().await;
    let client = client_with_providers(ProviderConfig {
        shopify_url: Some(&format!("{}/graphql.json", upstream.url)),
        stripe_url: Some(&format!("{}/stripe/", upstream.url)),
        paypal_url: Some(&upstream.url),
        checkout_enabled: true,
        checkout_rate_limit_attempts: Some(1),
        checkout_rate_limit_window_secs: Some(10),
        scheduler_enabled: Some(false),
        ..Default::default()
    })
    .await;
    let first = purchase(&fixture, "stripe").await;
    let mut second = first.clone();
    second["operation_id"] = json!(Uuid::new_v4());
    // Let the preceding fixture's source-address window expire without resetting it.
    tokio::time::sleep(std::time::Duration::from_secs(11)).await;
    accepted(&client, &first).await;
    accepted(&client, &first).await;
    let count = upstream.calls().len();
    let response = client
        .send(request(CHECKOUT, Some(SECRET), &second))
        .await
        .unwrap();
    assert_eq!(response.status(), 429);
    assert_eq!(upstream.calls().len(), count);
    assert_eq!(upstream.count("CreateDraftOrder"), 1);
    assert_eq!(upstream.payment_creates(), 1);
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS (\n    SELECT 1\n    FROM checkouts\n    WHERE id = $1\n)",
    )
    .bind(Uuid::parse_str(second["operation_id"].as_str().unwrap()).unwrap())
    .fetch_one(&fixture.pool())
    .await
    .unwrap();
    assert!(!exists);
}
