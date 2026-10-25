#![allow(clippy::too_many_lines)]

use crate::common::checkout::{
    CHECKOUT, OTHER_SECRET, SECRET, accepted, configured, providers::Providers, purchase,
    purchase_in_currency, purchase_with_slug, request, verify,
};
use crate::common::{TestFixture, body_json, fixture};
use axum::{body::Body, http::Request};
use serde_json::{Value, json};
use uuid::Uuid;

async fn assert_private_data_absent(fixture: &TestFixture, id: &str) {
    let stored: Value = sqlx::query_scalar(
        "SELECT to_jsonb(checkout)
         FROM checkouts checkout
         WHERE id = $1",
    )
    .bind(Uuid::parse_str(id).unwrap())
    .fetch_one(&fixture.pool())
    .await
    .unwrap();
    for field in [
        "email",
        "items",
        "snapshot",
        "purchase",
        "payload",
        "context",
        "money",
        "redirect_url",
        "order_name",
        "note",
    ] {
        assert!(
            stored.get(field).is_none(),
            "unexpected stored purchase field: {field}"
        );
    }
    let encoded = stored.to_string();
    assert!(!encoded.contains("buyer@example.test"));
    assert!(!encoded.contains("gid://shopify/ProductVariant/"));
    assert!(!encoded.contains(SECRET));
    let removed: (Option<String>, Option<String>, Option<String>) = sqlx::query_as(
        "SELECT to_regclass('checkout_items')::TEXT,
                to_regclass('checkout_events')::TEXT,
                to_regclass('checkout_rate_limits')::TEXT",
    )
    .fetch_one(&fixture.pool())
    .await
    .unwrap();
    assert_eq!(removed, (None, None, None));
}

#[tokio::test]
async fn checkout_reserves_authoritative_draft_and_retries_without_new_payment_for_both_gateways() {
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
        let stored: (String, String, String, Vec<u8>) = sqlx::query_as(
            "SELECT draft_state, payment_state, draft_id, guest_secret_hash
             FROM checkouts
             WHERE id = $1",
        )
        .bind(Uuid::parse_str(id).unwrap())
        .fetch_one(&fixture.pool())
        .await
        .unwrap();
        assert_eq!(stored.0, "reserved");
        assert_eq!(stored.1, "requires_action");
        assert_eq!(stored.3.len(), 32);
        assert_ne!(stored.3, hex::decode(SECRET).unwrap());
        assert_private_data_absent(&fixture, id).await;
        assert_eq!(upstream.count("CreateDraftOrder"), 1);
        assert!(upstream.count("CheckoutDraft") >= 1);
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
        let location = before
            .iter()
            .find(|call| call["operation"] == "Location")
            .unwrap();
        assert_eq!(
            location["body"]["variables"]["id"],
            "gid://shopify/Location/1"
        );
        let draft_input = &before[create_index]["body"]["variables"]["input"];
        assert_eq!(draft_input["email"], "buyer@example.test");
        assert_eq!(draft_input["lineItems"][0]["quantity"], 2);
        assert!(draft_input["lineItems"][0].get("priceOverride").is_none());
        assert!(draft_input.get("presentmentCurrencyCode").is_none());
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
        ("note", json!(123), 400),
        ("note", json!("🐶".repeat(2001)), 400),
        ("note", json!("private\u{0}note"), 400),
        ("note", json!("private\u{7f}note"), 400),
        ("note", json!("private\u{85}note"), 400),
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
        let persisted: (String, String, String) = sqlx::query_as(
            "SELECT payment_state, completion_state, order_id
             FROM checkouts
             WHERE id = $1",
        )
        .bind(Uuid::parse_str(id).unwrap())
        .fetch_one(&fixture.pool())
        .await
        .unwrap();
        assert_eq!(persisted.0, "paid");
        assert_eq!(persisted.1, "completed");
        assert!(persisted.2.starts_with("gid://shopify/Order/"));
        assert_private_data_absent(&fixture, id).await;
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
        upstream.fault("UpdateOrderCustomAttributes", "http");
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
    for case in ["last_unit", "untracked", "continue_selling", "no_hold"] {
        let upstream = Providers::start().await;
        let client = configured(&upstream).await;
        let input = purchase(&fixture, "stripe").await;
        match case {
            "last_unit" => upstream.inventory(1, true, "DENY"),
            "untracked" => upstream.inventory(10, false, "DENY"),
            "continue_selling" => upstream.inventory(10, true, "CONTINUE"),
            "no_hold" => upstream.reservation(false, true),
            _ => unreachable!(),
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
            usize::from(case == "no_hold")
        );
        if case == "no_hold" {
            let response = client
                .send(request(CHECKOUT, Some(SECRET), &input))
                .await
                .unwrap();
            assert_eq!(response.status(), 409, "{case}");
            assert!(body_json(response).await.get("money").is_none());
            assert_eq!(upstream.count("CreateDraftOrder"), 1);
            assert_eq!(upstream.payment_creates(), 0);
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
                if matches!(operation, "Location" | "CatalogueProducts") {
                    200
                } else if failure == "timeout" {
                    504
                } else {
                    502
                },
                "{operation}/{failure}: {body}"
            );
            assert!(!body.to_string().contains("private"));
            if matches!(operation, "Location" | "CatalogueProducts") {
                assert_eq!(body["status"], "unpaid");
                upstream.clear_fault();
                let calls = upstream.calls().len();
                assert_eq!(accepted(&client, &input).await["status"], "unpaid");
                assert_eq!(upstream.calls().len(), calls);
            } else {
                assert!(body.get("checkout_id").is_none());
            }
            assert_eq!(upstream.count(operation), 1);
            if operation == "/stripe/account" {
                assert_eq!(upstream.count("CreateDraftOrder"), 0);
                assert_eq!(upstream.payment_creates(), 0);
            }
            assert!(
                upstream.payment_creates()
                    <= usize::from(matches!(
                        operation,
                        "/stripe/checkout/sessions" | "/v2/checkout/orders"
                    ))
            );
            if matches!(operation, "CreateDraftOrder" | "CheckoutDraft") {
                upstream.clear_fault();
                let known_draft = operation == "CheckoutDraft";
                if known_draft {
                    assert_eq!(accepted(&client, &input).await["status"], "requires_action");
                } else {
                    let response = client
                        .send(request(CHECKOUT, Some(SECRET), &input))
                        .await
                        .unwrap();
                    assert_eq!(response.status(), 409);
                    assert!(body_json(response).await.get("money").is_none());
                }
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
    let mut missing_id = event.clone();
    missing_id.as_object_mut().unwrap().remove("id");
    let mut malformed_id = event.clone();
    malformed_id["id"] = json!("");
    let mut malformed_version = event.clone();
    malformed_version["api_version"] = json!(42);
    for event in [
        wrong_account,
        wrong_mode,
        malformed,
        missing_id,
        malformed_id,
        malformed_version,
    ] {
        assert_eq!(
            client
                .send(stripe_webhook(&event, chrono::Utc::now().timestamp()))
                .await
                .unwrap()
                .status(),
            400
        );
    }
    let paypal = upstream.paypal_event(&id, "event_fixture");
    let mut missing_paypal_id = paypal.clone();
    missing_paypal_id.as_object_mut().unwrap().remove("id");
    let mut malformed_paypal_id = paypal;
    malformed_paypal_id["id"] = json!(42);
    for event in [missing_paypal_id, malformed_paypal_id] {
        assert_eq!(
            client.send(paypal_webhook(&event)).await.unwrap().status(),
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
        assert!(body_json(response).await["error"].is_string());
    }
    assert_eq!(upstream.count("/stripe/account"), 1);
    assert!(
        upstream
            .calls()
            .iter()
            .all(|call| call["operation"] == "/stripe/account")
    );
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
    assert_eq!(upstream.count("/stripe/account"), 2);
    assert!(
        upstream
            .calls()
            .iter()
            .all(|call| call["operation"] == "/stripe/account")
    );
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
        assert_eq!(client.send(callback()).await.unwrap().status(), 502);
        let completed: bool = sqlx::query_scalar(
            "SELECT completion_state = 'completed'
             FROM checkouts
             WHERE id = $1",
        )
        .bind(Uuid::parse_str(id).unwrap())
        .fetch_one(&fixture.pool())
        .await
        .unwrap();
        assert!(!completed);
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
    assert!(
        upstream
            .calls()
            .iter()
            .all(|call| call["operation"] == "/stripe/account")
    );
    assert_eq!(upstream.payment_creates(), 0);
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
    assert!(
        upstream
            .calls()
            .iter()
            .all(|call| call["operation"] == "/stripe/account")
    );
    assert_eq!(upstream.payment_creates(), 0);
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "DROP TRIGGER {prefix}_checkout_fault ON checkouts;
         DROP FUNCTION {prefix}_checkout_fault();"
    )))
    .execute(&fixture.pool())
    .await
    .unwrap();
    accepted(&client, &input).await;
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "CREATE FUNCTION {prefix}_checkout_fault() RETURNS TRIGGER
         LANGUAGE plpgsql AS $$
         BEGIN
             IF NEW.id = '{id}'::uuid THEN
                 RAISE EXCEPTION 'Synthetic owned checkout claim failure';
             END IF;
             RETURN NEW;
         END;
         $$;
         CREATE TRIGGER {prefix}_checkout_fault
             BEFORE UPDATE ON checkouts
             FOR EACH ROW
             EXECUTE FUNCTION {prefix}_checkout_fault();"
    )))
    .execute(&fixture.pool())
    .await
    .unwrap();
    let calls = upstream.calls().len();
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
    let after = upstream.calls();
    assert_eq!(after.len(), calls + 1);
    assert_eq!(after[calls]["operation"], "/stripe/account");
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
async fn checkout_provider_connection_failure_does_not_persist_unverified_account() {
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
    let stored: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)
         FROM checkouts
         WHERE id = $1",
    )
    .bind(Uuid::parse_str(input["operation_id"].as_str().unwrap()).unwrap())
    .fetch_one(&fixture.pool())
    .await
    .unwrap();
    assert_eq!(stored, 0);
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
        upstream.shopify_money("12.00", "AUD");
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
            assert_eq!(retried["status"], "requires_action");
            assert_eq!(upstream.count("DraftOrders"), 1);
            assert_eq!(upstream.payment_creates(), 1);
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
        upstream.shopify_money("12.00", "AUD");
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
async fn checkout_last_unit_race_only_opens_payment_for_the_successfully_reserved_draft() {
    let fixture = fixture().await;
    let upstream = Providers::start().await;
    upstream.contend_last_unit();
    let client = configured(&upstream).await;
    let mut first = purchase(&fixture, "stripe").await;
    let variant = Uuid::parse_str(first["items"][0]["variant_id"].as_str().unwrap()).unwrap();
    sqlx::query(
        "UPDATE convention_variants
SET final_price = 40
WHERE id = $1",
    )
    .bind(variant)
    .execute(&fixture.pool())
    .await
    .unwrap();
    first["items"][0]["quantity"] = json!(1);
    first["items"][0]["expected_unit_price"] = json!(4000);
    let mut second = first.clone();
    second["operation_id"] = json!(Uuid::new_v4().to_string());
    let (first_response, second_response) = tokio::join!(
        client.send(request(CHECKOUT, Some(SECRET), &first)),
        client.send(request(CHECKOUT, Some(SECRET), &second)),
    );
    let mut statuses = [
        first_response.unwrap().status().as_u16(),
        second_response.unwrap().status().as_u16(),
    ];
    statuses.sort_unstable();
    assert_eq!(statuses, [200, 200]);
    assert_eq!(upstream.count("CatalogueProducts"), 2);
    assert_eq!(upstream.count("CreateDraftOrder"), 2);
    assert_eq!(upstream.payment_creates(), 1);
}

#[tokio::test]
async fn checkout_oversized_customer_payloads_use_shared_safe_error_responses() {
    let _fixture = fixture().await;
    let upstream = Providers::start().await;
    let client = configured(&upstream).await;
    let id = Uuid::new_v4();
    for path in [CHECKOUT.to_owned(), format!("/api/v1/order/{id}/verify")] {
        let response = client
            .send(
                Request::builder()
                    .method("POST")
                    .uri(path)
                    .header("authorization", format!("Bearer {SECRET}"))
                    .header("content-type", "application/json")
                    .body(Body::from(vec![b'x'; 1_048_577]))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), 413);
        assert_eq!(response.headers()["cache-control"], "no-store");
        assert_eq!(
            body_json(response).await,
            json!({"error":"Checkout request body is too large"})
        );
    }
    assert_eq!(upstream.calls(), Vec::<Value>::new());
}

#[tokio::test]
async fn checkout_missing_draft_id_requires_unique_complete_exact_evidence() {
    let fixture = fixture().await;
    for (mode, expected) in [
        ("unique", 200),
        ("noisy", 200),
        ("paginated", 200),
        ("empty", 409),
        ("multiple", 409),
        ("mismatch", 409),
        ("missing_schema", 409),
        ("duplicate_identity", 409),
        ("old_created", 409),
        ("missing_created", 409),
        ("scan_cap", 409),
        ("incomplete", 502),
        ("repeated_cursor", 502),
        ("later_http", 502),
    ] {
        let upstream = Providers::start().await;
        let client = configured(&upstream).await;
        let input = purchase(&fixture, "stripe").await;
        let id = input["operation_id"].as_str().unwrap();
        upstream.fault("CreateDraftOrder", "lost");
        assert_eq!(
            client
                .send(request(CHECKOUT, Some(SECRET), &input))
                .await
                .unwrap()
                .status(),
            502
        );
        assert_private_data_absent(&fixture, id).await;
        upstream.clear_fault();
        upstream.search_mode(mode);
        let response = client.send(verify(id, Some(SECRET))).await.unwrap();
        let status = response.status();
        let body = body_json(response).await;
        assert_eq!(status, expected, "{mode}: {body}");
        assert_eq!(upstream.count("CreateDraftOrder"), 1, "{mode}");
        assert_eq!(
            upstream.payment_creates(),
            usize::from(expected == 200),
            "{mode}"
        );
        assert_eq!(
            upstream.count("DraftOrders"),
            if mode == "scan_cap" {
                20
            } else if matches!(mode, "paginated" | "later_http" | "repeated_cursor") {
                2
            } else {
                1
            }
        );
        if expected == 200 {
            assert_eq!(body["money"], json!({"minor":4000,"currency":"SGD"}));
        } else {
            assert!(body.get("money").is_none());
        }
        assert_private_data_absent(&fixture, id).await;
    }
}

#[tokio::test]
async fn checkout_draft_search_dependency_failures_never_recreate_or_open_payment() {
    let fixture = fixture().await;
    for failure in ["http", "malformed", "missing", "graphql", "timeout"] {
        let upstream = Providers::start().await;
        let client = configured(&upstream).await;
        let input = purchase(&fixture, "stripe").await;
        let id = input["operation_id"].as_str().unwrap();
        upstream.fault("CreateDraftOrder", "lost");
        assert_eq!(
            client
                .send(request(CHECKOUT, Some(SECRET), &input))
                .await
                .unwrap()
                .status(),
            502
        );
        upstream.fault("DraftOrders", failure);
        let response = client.send(verify(id, Some(SECRET))).await.unwrap();
        assert_eq!(
            response.status(),
            if failure == "timeout" { 504 } else { 502 },
            "{failure}"
        );
        assert_eq!(
            body_json(response).await,
            json!({"error":"Upstream request failed"})
        );
        assert_eq!(upstream.count("CreateDraftOrder"), 1);
        assert_eq!(upstream.payment_creates(), 0);
        upstream.clear_fault();
        assert_eq!(
            client
                .send(verify(id, Some(SECRET)))
                .await
                .unwrap()
                .status(),
            200
        );
        assert_eq!(upstream.count("CreateDraftOrder"), 1);
        assert_eq!(upstream.payment_creates(), 1);
    }
}

#[tokio::test]
async fn checkout_never_attempted_requires_original_body_and_busy_without_draft_is_safe() {
    let fixture = fixture().await;
    let upstream = Providers::start().await;
    let client = configured(&upstream).await;
    let input = purchase(&fixture, "stripe").await;
    let id = input["operation_id"].as_str().unwrap();
    upstream.fault("CatalogueProducts", "http");
    assert_eq!(
        client
            .send(request(CHECKOUT, Some(SECRET), &input))
            .await
            .unwrap()
            .status(),
        200
    );
    sqlx::query(
        "UPDATE checkouts
         SET payment_state = 'not_started'
         WHERE id = $1",
    )
    .bind(Uuid::parse_str(id).unwrap())
    .execute(&fixture.pool())
    .await
    .unwrap();
    assert_eq!(upstream.count("CreateDraftOrder"), 0);
    assert_private_data_absent(&fixture, id).await;
    upstream.clear_fault();
    let calls = upstream.calls().len();
    let response = client.send(verify(id, Some(SECRET))).await.unwrap();
    assert_eq!(response.status(), 409);
    assert!(body_json(response).await.get("money").is_none());
    assert_eq!(upstream.calls().len(), calls);
    sqlx::query(
        "UPDATE checkouts
         SET claim_token = $2, claim_expires_at = CURRENT_TIMESTAMP + INTERVAL '1 minute'
         WHERE id = $1",
    )
    .bind(Uuid::parse_str(id).unwrap())
    .bind(Uuid::new_v4())
    .execute(&fixture.pool())
    .await
    .unwrap();
    let response = client.send(verify(id, Some(SECRET))).await.unwrap();
    assert_eq!(response.status(), 503);
    assert!(body_json(response).await.get("money").is_none());
    sqlx::query(
        "UPDATE checkouts
         SET claim_token = NULL, claim_expires_at = NULL
         WHERE id = $1",
    )
    .bind(Uuid::parse_str(id).unwrap())
    .execute(&fixture.pool())
    .await
    .unwrap();
    assert_eq!(accepted(&client, &input).await["status"], "requires_action");
    assert_eq!(upstream.count("CreateDraftOrder"), 1);
    assert_eq!(upstream.payment_creates(), 1);
}

#[tokio::test]
async fn checkout_draft_edits_block_gateway_retry_and_capture_but_unrelated_tags_are_allowed() {
    let fixture = fixture().await;
    for provider in ["stripe", "paypal"] {
        for field in [
            "email",
            "quantity",
            "money",
            "required_tag",
            "unrelated_tag",
        ] {
            let upstream = Providers::start().await;
            let client = configured(&upstream).await;
            let input = purchase(&fixture, provider).await;
            let id = input["operation_id"].as_str().unwrap();
            let operation = if provider == "stripe" {
                "/stripe/checkout/sessions"
            } else {
                "/v2/checkout/orders"
            };
            upstream.fault(operation, "lost");
            assert_eq!(
                client
                    .send(request(CHECKOUT, Some(SECRET), &input))
                    .await
                    .unwrap()
                    .status(),
                502
            );
            upstream.clear_fault();
            upstream.edit_draft(field);
            let response = client.send(verify(id, Some(SECRET))).await.unwrap();
            assert_eq!(
                response.status(),
                if field == "unrelated_tag" { 200 } else { 409 },
                "{provider}/{field}"
            );
            assert_eq!(
                upstream.payment_creates(),
                if field == "unrelated_tag" { 2 } else { 1 }
            );
            assert_eq!(upstream.count("CompleteCheckoutDraft"), 0);
            if field == "unrelated_tag" {
                upstream.pay();
                assert_eq!(
                    client
                        .send(verify(id, Some(SECRET)))
                        .await
                        .unwrap()
                        .status(),
                    200
                );
                assert_eq!(upstream.count("CompleteCheckoutDraft"), 1);
            }
        }
        let upstream = Providers::start().await;
        let client = configured(&upstream).await;
        let input = purchase(&fixture, provider).await;
        accepted(&client, &input).await;
        upstream.pay();
        upstream.edit_draft("quantity");
        let response = client
            .send(verify(
                input["operation_id"].as_str().unwrap(),
                Some(SECRET),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), 409);
        assert_eq!(upstream.count("CompleteCheckoutDraft"), 0);
        assert!(!upstream.calls().iter().any(|call| {
            call["path"]
                .as_str()
                .is_some_and(|path| path.ends_with("/capture"))
        }));
    }
}

#[tokio::test]
async fn checkout_gateway_fingerprint_prevents_changed_recovery_request() {
    let fixture = fixture().await;
    for provider in ["stripe", "paypal"] {
        let upstream = Providers::start().await;
        upstream.shopify_money("12.00", "AUD");
        let client = configured(&upstream).await;
        let input = purchase(&fixture, provider).await;
        let id = input["operation_id"].as_str().unwrap();
        upstream.fault(
            if provider == "stripe" {
                "/stripe/checkout/sessions"
            } else {
                "/v2/checkout/orders"
            },
            "lost",
        );
        assert_eq!(
            client
                .send(request(CHECKOUT, Some(SECRET), &input))
                .await
                .unwrap()
                .status(),
            502
        );
        assert_private_data_absent(&fixture, id).await;
        upstream.clear_fault();
        sqlx::query(
            "UPDATE checkouts
             SET success_url = success_url || '&changed=true'
             WHERE id = $1",
        )
        .bind(Uuid::parse_str(id).unwrap())
        .execute(&fixture.pool())
        .await
        .unwrap();
        let response = client.send(verify(id, Some(SECRET))).await.unwrap();
        assert_eq!(response.status(), 409);
        assert_eq!(upstream.payment_creates(), 1);
        assert_eq!(upstream.count("CompleteCheckoutDraft"), 0);
    }
}

#[tokio::test]
async fn checkout_post_mutation_link_save_faults_recover_original_draft_and_gateway() {
    let fixture = fixture().await;
    for provider in ["stripe", "paypal"] {
        for association in ["draft_id", "provider_resource_id"] {
            let upstream = Providers::start().await;
            let client = configured(&upstream).await;
            let input = purchase(&fixture, provider).await;
            let id = input["operation_id"].as_str().unwrap();
            let prefix = fixture.prefix();
            sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
                "CREATE FUNCTION {prefix}_checkout_fault() RETURNS TRIGGER
                 LANGUAGE plpgsql AS $$
                 BEGIN
                     IF NEW.id = '{id}'::uuid
                         AND OLD.{association} IS NULL
                         AND NEW.{association} IS NOT NULL THEN
                         RAISE EXCEPTION 'Synthetic private post-mutation linkage failure';
                     END IF;
                     RETURN NEW;
                 END;
                 $$;
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
            assert_eq!(response.status(), 500, "{provider}/{association}");
            assert_eq!(
                body_json(response).await,
                json!({"error":"Internal server error"})
            );
            assert_eq!(upstream.count("CreateDraftOrder"), 1);
            assert_eq!(
                upstream.payment_creates(),
                usize::from(association == "provider_resource_id")
            );
            let interrupted: Value = sqlx::query_scalar(
                "SELECT to_jsonb(checkout)
                 FROM checkouts checkout
                 WHERE id = $1",
            )
            .bind(Uuid::parse_str(id).unwrap())
            .fetch_one(&fixture.pool())
            .await
            .unwrap();
            assert!(interrupted[association].is_null());
            assert!(interrupted["provider_resource_id"].is_null());
            assert!(interrupted["success_url"].is_string());
            assert!(interrupted["cancel_url"].is_string());
            assert!(interrupted["claim_token"].is_null());
            assert!(interrupted["claim_expires_at"].is_null());
            if association == "provider_resource_id" {
                assert_eq!(interrupted["draft_state"], "reserved");
                assert_eq!(interrupted["payment_state"], "creating");
                assert!(interrupted["gateway_request_fingerprint"].is_string());
            } else {
                assert_eq!(interrupted["draft_state"], "creating");
                assert_eq!(interrupted["payment_state"], "not_started");
                assert!(interrupted["gateway_request_fingerprint"].is_null());
            }
            assert_private_data_absent(&fixture, id).await;
            sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
                "DROP TRIGGER {prefix}_checkout_fault ON checkouts;
                 DROP FUNCTION {prefix}_checkout_fault();"
            )))
            .execute(&fixture.pool())
            .await
            .unwrap();
            let response = client.send(verify(id, Some(SECRET))).await.unwrap();
            let status = response.status();
            let body = body_json(response).await;
            assert_eq!(status, 200, "{provider}/{association}: {body}");
            assert_eq!(body["status"], "requires_action");
            assert_eq!(body["money"], json!({"minor":4000,"currency":"SGD"}));
            assert_eq!(upstream.count("CreateDraftOrder"), 1);
            assert_eq!(
                upstream.count("DraftOrders"),
                usize::from(association == "draft_id")
            );
            assert_eq!(upstream.count("CompleteCheckoutDraft"), 0);
            let gateway_calls: Vec<_> = upstream
                .calls()
                .into_iter()
                .filter(|call| call["payment_create"] == true)
                .collect();
            assert_eq!(
                gateway_calls.len(),
                if association == "provider_resource_id" {
                    2
                } else {
                    1
                }
            );
            let key_header = if provider == "stripe" {
                "idempotency_key"
            } else {
                "paypal_request_id"
            };
            for call in &gateway_calls {
                assert_eq!(call[key_header], format!("checkout-{id}"));
                assert_eq!(call["body"], gateway_calls[0]["body"]);
            }
            let recovered: Value = sqlx::query_scalar(
                "SELECT to_jsonb(checkout)
                 FROM checkouts checkout
                 WHERE id = $1",
            )
            .bind(Uuid::parse_str(id).unwrap())
            .fetch_one(&fixture.pool())
            .await
            .unwrap();
            let expected_resource = if provider == "stripe" {
                upstream.stripe_event(id, "evt_binding_identity")["data"]["object"]["id"].clone()
            } else {
                upstream.paypal_event(id, "evt_binding_identity")["resource"]["id"].clone()
            };
            assert_eq!(recovered["provider_resource_id"], expected_resource);
            assert_eq!(recovered["draft_state"], "reserved");
            assert_eq!(recovered["payment_state"], "requires_action");
            assert!(recovered["success_url"].is_null());
            assert!(recovered["cancel_url"].is_null());
            assert_eq!(
                recovered["reservation_expires_at"],
                interrupted["reservation_expires_at"]
            );
            if association == "provider_resource_id" {
                assert_eq!(
                    recovered["gateway_request_fingerprint"],
                    interrupted["gateway_request_fingerprint"]
                );
                assert_eq!(recovered["draft_id"], interrupted["draft_id"]);
            }
            assert_private_data_absent(&fixture, id).await;
        }
    }
}

#[tokio::test]
async fn checkout_completed_unpaid_stripe_session_has_no_redirect_then_completes() {
    let fixture = fixture().await;
    let upstream = Providers::start().await;
    let client = configured(&upstream).await;
    let input = purchase(&fixture, "stripe").await;
    accepted(&client, &input).await;
    upstream.stripe_processing();
    let id = input["operation_id"].as_str().unwrap();
    let response = client.send(verify(id, Some(SECRET))).await.unwrap();
    let status = response.status();
    let body = body_json(response).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["status"], "processing");
    assert!(body["next_action"].is_null());
    upstream.pay();
    let response = client.send(verify(id, Some(SECRET))).await.unwrap();
    assert_eq!(body_json(response).await["status"], "completed");
    assert_eq!(upstream.payment_creates(), 1);
}

#[tokio::test]
async fn checkout_lost_stripe_create_recovers_completed_sessions_without_url() {
    let fixture = fixture().await;
    for paid in [false, true] {
        let upstream = Providers::start().await;
        let client = configured(&upstream).await;
        let input = purchase(&fixture, "stripe").await;
        upstream.fault("/stripe/checkout/sessions", "lost");
        assert_eq!(
            client
                .send(request(CHECKOUT, Some(SECRET), &input))
                .await
                .unwrap()
                .status(),
            502
        );
        upstream.clear_fault();
        if paid {
            upstream.pay();
        } else {
            upstream.stripe_processing();
        }
        let body = accepted(&client, &input).await;
        assert_eq!(
            body["status"],
            if paid { "completed" } else { "processing" }
        );
        assert!(body["next_action"].is_null());
        let calls = upstream.calls();
        let creates: Vec<_> = calls
            .iter()
            .filter(|call| call["payment_create"] == true)
            .collect();
        assert_eq!(creates.len(), 2);
        assert_eq!(creates[0]["body"], creates[1]["body"]);
        assert_eq!(creates[0]["idempotency_key"], creates[1]["idempotency_key"]);
        for call in creates {
            assert_eq!(call["stripe_version"], "2026-08-26.dahlia");
        }
        assert_eq!(upstream.count("CreateDraftOrder"), 1);
    }
}

#[tokio::test]
async fn checkout_late_paid_truth_survives_missing_shortened_or_expired_reservation() {
    let fixture = fixture().await;
    for provider in ["stripe", "paypal"] {
        for hold in ["missing", "shortened", "expired"] {
            let upstream = Providers::start().await;
            let client = configured(&upstream).await;
            let input = purchase(&fixture, provider).await;
            accepted(&client, &input).await;
            if provider == "stripe" {
                upstream.pay();
            } else {
                upstream.captured();
            }
            let id = input["operation_id"].as_str().unwrap();
            match hold {
                "missing" => upstream.reservation(false, true),
                "shortened" => upstream.shorten_hold(),
                _ => {
                    sqlx::query(
                        "UPDATE checkouts
                     SET reservation_expires_at = NOW() - INTERVAL '1 second'
                     WHERE id = $1",
                    )
                    .bind(Uuid::parse_str(id).unwrap())
                    .execute(&fixture.pool())
                    .await
                    .unwrap();
                }
            }
            let response = client.send(verify(id, Some(SECRET))).await.unwrap();
            let status = response.status();
            let body = body_json(response).await;
            assert_eq!(status, 200, "{provider}/{hold}: {body}");
            assert_eq!(
                body["status"], "reconciliation_required",
                "{provider}/{hold}: {body}"
            );
            assert!(body["next_action"].is_null());
            let payment: String = sqlx::query_scalar(
                "SELECT payment_state
                 FROM checkouts
                 WHERE id = $1",
            )
            .bind(Uuid::parse_str(id).unwrap())
            .fetch_one(&fixture.pool())
            .await
            .unwrap();
            assert_eq!(payment, "paid");
            assert_eq!(upstream.count("CompleteCheckoutDraft"), 0);
            upstream.complete();
            let response = client.send(verify(id, Some(SECRET))).await.unwrap();
            let body = body_json(response).await;
            assert_eq!(body["status"], "completed", "{provider}/{hold}: {body}");
            assert_eq!(body["order"]["name"], "#E2E-ORDER");
            assert_eq!(upstream.count("CompleteCheckoutDraft"), 0);
        }
    }
}

#[tokio::test]
async fn checkout_pending_after_unsafe_hold_never_becomes_unpaid_or_captures() {
    let fixture = fixture().await;
    for provider in ["stripe", "paypal"] {
        for hold in ["missing", "expired"] {
            let upstream = Providers::start().await;
            let client = configured(&upstream).await;
            let input = purchase(&fixture, provider).await;
            accepted(&client, &input).await;
            if provider == "paypal" {
                upstream.pay();
            }
            let id = input["operation_id"].as_str().unwrap();
            if hold == "missing" {
                upstream.reservation(false, true);
            } else {
                sqlx::query(
                    "UPDATE checkouts
                     SET reservation_expires_at = NOW() - INTERVAL '1 second'
                     WHERE id = $1",
                )
                .bind(Uuid::parse_str(id).unwrap())
                .execute(&fixture.pool())
                .await
                .unwrap();
            }
            let response = client.send(verify(id, Some(SECRET))).await.unwrap();
            let status = response.status();
            let body = body_json(response).await;
            assert_eq!(status, 200, "{provider}/{hold}: {body}");
            assert!(
                matches!(
                    body["status"].as_str(),
                    Some("processing" | "reconciliation_required")
                ),
                "{body}"
            );
            assert!(body["next_action"].is_null());
            assert_eq!(upstream.count("CompleteCheckoutDraft"), 0);
            assert!(!upstream.calls().iter().any(|call| {
                call["path"]
                    .as_str()
                    .is_some_and(|path| path.ends_with("/capture"))
            }));
            assert_eq!(upstream.payment_creates(), 1);
        }
    }
}

#[tokio::test]
async fn checkout_accepts_public_variant_gid_and_rejects_nonselected_and_alias_duplicates() {
    let fixture = fixture().await;
    for provider in ["stripe", "paypal"] {
        let upstream = Providers::start().await;
        let client = configured(&upstream).await;
        let mut input = purchase(&fixture, provider).await;
        let original = input["items"][0].clone();
        input["items"][0]["variant_id"] = json!("gid://shopify/ProductVariant/2");
        assert_eq!(accepted(&client, &input).await["status"], "requires_action");
        let mut aliases = input.clone();
        aliases["operation_id"] = json!(Uuid::new_v4());
        aliases["items"].as_array_mut().unwrap().push(original);
        assert_eq!(
            client
                .send(request(CHECKOUT, Some(SECRET), &aliases))
                .await
                .unwrap()
                .status(),
            409
        );
        for (variant, expected) in [
            ("gid://shopify/ProductVariant/999", 409),
            ("gid://shopify/ProductVariant/02", 400),
            ("gid://shopify/Product/2", 400),
        ] {
            let mut invalid = input.clone();
            invalid["operation_id"] = json!(Uuid::new_v4());
            invalid["items"][0]["variant_id"] = json!(variant);
            assert_eq!(
                client
                    .send(request(CHECKOUT, Some(SECRET), &invalid))
                    .await
                    .unwrap()
                    .status(),
                expected,
                "{variant}"
            );
        }
        assert_eq!(upstream.count("CreateDraftOrder"), 1);
        assert_eq!(upstream.payment_creates(), 1);
    }
}

#[tokio::test]
async fn checkout_previously_verified_paid_is_not_downgraded_by_pending_provider_evidence() {
    let fixture = fixture().await;
    for provider in ["stripe", "paypal"] {
        let upstream = Providers::start().await;
        let client = configured(&upstream).await;
        let input = purchase(&fixture, provider).await;
        accepted(&client, &input).await;
        upstream.pay();
        upstream.fault("CompleteCheckoutDraft", "http");
        let id = input["operation_id"].as_str().unwrap();
        let response = client.send(verify(id, Some(SECRET))).await.unwrap();
        assert_eq!(
            body_json(response).await["status"],
            "reconciliation_required"
        );
        let before = upstream.calls().len();
        upstream.clear_fault();
        upstream.pending();
        let response = client.send(verify(id, Some(SECRET))).await.unwrap();
        let body = body_json(response).await;
        assert_eq!(body["status"], "reconciliation_required");
        assert!(body["next_action"].is_null());
        let payment: String = sqlx::query_scalar(
            "SELECT payment_state
             FROM checkouts
             WHERE id = $1",
        )
        .bind(Uuid::parse_str(id).unwrap())
        .fetch_one(&fixture.pool())
        .await
        .unwrap();
        assert_eq!(payment, "paid");
        assert!(!upstream.calls()[before..].iter().any(|call| {
            call["path"]
                .as_str()
                .is_some_and(|path| path.ends_with("/capture"))
        }));
    }
}

#[tokio::test]
async fn checkout_authenticated_stripe_binding_handles_completed_null_url_sessions() {
    let fixture = fixture().await;
    for paid in [false, true] {
        let upstream = Providers::start().await;
        let client = configured(&upstream).await;
        let input = purchase(&fixture, "stripe").await;
        upstream.fault("/stripe/checkout/sessions", "lost");
        assert_eq!(
            client
                .send(request(CHECKOUT, Some(SECRET), &input))
                .await
                .unwrap()
                .status(),
            502
        );
        upstream.clear_fault();
        if paid {
            upstream.pay();
        } else {
            upstream.stripe_processing();
        }
        let id = input["operation_id"].as_str().unwrap();
        let event = upstream.stripe_event(id, "evt_null_url_binding");
        let response = client
            .send(stripe_webhook(&event, chrono::Utc::now().timestamp()))
            .await
            .unwrap();
        assert_eq!(response.status(), if paid { 200 } else { 503 });
        let response = client.send(verify(id, Some(SECRET))).await.unwrap();
        let body = body_json(response).await;
        assert_eq!(
            body["status"],
            if paid { "completed" } else { "processing" }
        );
        assert!(body["next_action"].is_null());
        assert_eq!(upstream.payment_creates(), 1);
    }
}

#[tokio::test]
async fn checkout_late_authenticated_binding_preserves_paid_after_unknown_creation_expires() {
    let fixture = fixture().await;
    let upstream = Providers::start().await;
    let client = configured(&upstream).await;
    let input = purchase(&fixture, "stripe").await;
    upstream.fault("/stripe/checkout/sessions", "lost");
    assert_eq!(
        client
            .send(request(CHECKOUT, Some(SECRET), &input))
            .await
            .unwrap()
            .status(),
        502
    );
    upstream.clear_fault();
    let id = input["operation_id"].as_str().unwrap();
    sqlx::query(
        "UPDATE checkouts
         SET reservation_expires_at = NOW() - INTERVAL '1 second'
         WHERE id = $1",
    )
    .bind(Uuid::parse_str(id).unwrap())
    .execute(&fixture.pool())
    .await
    .unwrap();
    let response = client.send(verify(id, Some(SECRET))).await.unwrap();
    assert_eq!(
        body_json(response).await["status"],
        "reconciliation_required"
    );
    upstream.pay();
    let event = upstream.stripe_event(id, "evt_late_binding");
    let response = client
        .send(stripe_webhook(&event, chrono::Utc::now().timestamp()))
        .await
        .unwrap();
    assert_eq!(response.status(), 503);
    let stored: (String, Option<String>) = sqlx::query_as(
        "SELECT payment_state, provider_resource_id
         FROM checkouts
         WHERE id = $1",
    )
    .bind(Uuid::parse_str(id).unwrap())
    .fetch_one(&fixture.pool())
    .await
    .unwrap();
    assert_eq!(stored.0, "paid");
    assert_eq!(stored.1.as_deref(), event["data"]["object"]["id"].as_str());
    assert_eq!(upstream.payment_creates(), 1);
    assert_eq!(upstream.count("CompleteCheckoutDraft"), 0);
    upstream.complete();
    let response = client
        .send(stripe_webhook(&event, chrono::Utc::now().timestamp()))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let response = client.send(verify(id, Some(SECRET))).await.unwrap();
    assert_eq!(body_json(response).await["status"], "completed");
    assert_eq!(upstream.payment_creates(), 1);
}

#[tokio::test]
async fn checkout_hold_expiring_during_final_read_prevents_completion_mutation() {
    let fixture = fixture().await;
    let upstream = Providers::start().await;
    let client = configured(&upstream).await;
    let input = purchase(&fixture, "stripe").await;
    accepted(&client, &input).await;
    upstream.pay();
    let expiry = upstream.expire_during_completion_read();
    let id = input["operation_id"].as_str().unwrap();
    sqlx::query(
        "UPDATE checkouts
         SET reservation_expires_at = $2
         WHERE id = $1",
    )
    .bind(Uuid::parse_str(id).unwrap())
    .bind(expiry)
    .execute(&fixture.pool())
    .await
    .unwrap();
    let response = client.send(verify(id, Some(SECRET))).await.unwrap();
    let status = response.status();
    let body = body_json(response).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["status"], "reconciliation_required");
    assert!(body["next_action"].is_null());
    assert_eq!(upstream.count("CompleteCheckoutDraft"), 0);
    let payment: String = sqlx::query_scalar(
        "SELECT payment_state
         FROM checkouts
         WHERE id = $1",
    )
    .bind(Uuid::parse_str(id).unwrap())
    .fetch_one(&fixture.pool())
    .await
    .unwrap();
    assert_eq!(payment, "paid");
}

#[tokio::test]
async fn checkout_paypal_signed_event_preserves_wire_json_and_rejects_failed_postback() {
    let fixture = fixture().await;
    let upstream = Providers::start().await;
    let client = configured(&upstream).await;
    let input = purchase(&fixture, "paypal").await;
    accepted(&client, &input).await;
    upstream.pay();
    let id = input["operation_id"].as_str().unwrap();
    let event = upstream.paypal_event(id, "event_signed_raw");
    let raw = format!(
        "{{\"resource\" : {}, \"event_type\" : \"CHECKOUT.ORDER.APPROVED\", \"id\":\"event_signed_\\u0072aw\"}}",
        event["resource"]
    );
    let callback = || {
        let mut request = paypal_webhook(&event);
        *request.body_mut() = Body::from(raw.clone());
        request
    };
    upstream.fault(
        "/v1/notifications/verify-webhook-signature",
        "rejected_signature",
    );
    assert_eq!(client.send(callback()).await.unwrap().status(), 400);
    assert_eq!(upstream.count("CompleteCheckoutDraft"), 0);
    assert!(
        upstream
            .calls()
            .iter()
            .all(|call| { !call["path"].as_str().unwrap().ends_with("/capture") })
    );
    upstream.clear_fault();
    assert_eq!(client.send(callback()).await.unwrap().status(), 200);
    let postbacks: Vec<_> = upstream
        .calls()
        .into_iter()
        .filter(|call| call["operation"] == "/v1/notifications/verify-webhook-signature")
        .collect();
    assert_eq!(postbacks.len(), 2);
    for call in postbacks {
        assert_eq!(call["raw_webhook_event"], raw);
        assert_eq!(call["body"]["transmission_sig"], "synthetic_signature");
        assert_eq!(call["body"]["transmission_id"], "transmission_e2e");
        assert_eq!(call["body"]["webhook_id"], "webhook_e2e");
    }
    let response = client.send(verify(id, Some(SECRET))).await.unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(body_json(response).await["status"], "completed");
    assert_eq!(upstream.count("CompleteCheckoutDraft"), 1);
}

#[tokio::test]
async fn checkout_records_named_external_payment_and_preserves_full_convention_slug() {
    let fixture = fixture().await;
    for provider in ["stripe", "paypal"] {
        for length in [35, 36, 40, 41, 63] {
            let upstream = Providers::start().await;
            let client = configured(&upstream).await;
            let slug = format!(
                "{}-{}",
                fixture.prefix(),
                &Uuid::new_v4().simple().to_string()[..20]
            );
            let slug = format!("{}{}", slug, "a".repeat(length - slug.len()));
            let input = purchase_with_slug(&fixture, provider, &slug).await;
            sqlx::query(
                "UPDATE conventions
                 SET name = $2
                 WHERE id = $1",
            )
            .bind(Uuid::parse_str(input["convention_id"].as_str().unwrap()).unwrap())
            .bind(slug.replace('-', " ").to_uppercase())
            .execute(&fixture.pool())
            .await
            .unwrap();
            accepted(&client, &input).await;
            upstream.pay();
            let checkout = input["operation_id"].as_str().unwrap();
            let response = client.send(verify(checkout, Some(SECRET))).await.unwrap();
            assert_eq!(response.status(), 200);
            assert_eq!(body_json(response).await["status"], "completed");
            let calls = upstream.calls();
            let created = calls
                .iter()
                .find(|v| v["operation"] == "CreateDraftOrder")
                .unwrap();
            let draft = &created["body"]["variables"]["input"];
            let attributes = draft["customAttributes"].as_array().unwrap();
            for (key, value) in [
                ("checkout_correlation_version", "3"),
                ("checkout_id", checkout),
                ("convention_id", input["convention_id"].as_str().unwrap()),
                ("convention_slug", slug.as_str()),
                ("payment_provider", provider),
            ] {
                assert_eq!(
                    attributes
                        .iter()
                        .filter(|attribute| attribute["key"] == key && attribute["value"] == value)
                        .count(),
                    1,
                    "{key}"
                );
            }
            assert_eq!(
                draft["tags"],
                json!([slug.chars().take(40).collect::<String>()])
            );
            let converted = calls
                .iter()
                .find(|v| v["operation"] == "CompleteCheckoutDraft")
                .unwrap();
            assert_eq!(converted["body"]["variables"]["paymentPending"], true);
            let recorded = calls
                .iter()
                .find(|v| v["operation"] == "RecordCheckoutPayment")
                .unwrap();
            assert!(
                recorded["body"]["variables"]
                    .get("paymentMethodName")
                    .is_none()
            );
            assert_eq!(upstream.count("EditOrderTags"), 0);
            assert_eq!(upstream.count("UpdateOrderCustomAttributes"), 1);
            assert!(recorded["body"]["variables"].get("amount").is_none());
            let state: (i16, String, String) = sqlx::query_as(
                "SELECT completion_version, convention_slug, payment_recording_state
                 FROM checkouts
                 WHERE id = $1",
            )
            .bind(Uuid::parse_str(checkout).unwrap())
            .fetch_one(&fixture.pool())
            .await
            .unwrap();
            assert_eq!(state.0, 2);
            assert_eq!(state.1, slug);
            assert_eq!(state.2, "recorded");
            let response = client.send(verify(checkout, Some(SECRET))).await.unwrap();
            assert_eq!(body_json(response).await["status"], "completed");
            assert_eq!(upstream.count("CompleteCheckoutDraft"), 1);
            assert_eq!(upstream.count("RecordCheckoutPayment"), 1);
            assert_eq!(upstream.payment_creates(), 1);
        }
    }
}

#[tokio::test]
async fn checkout_lost_external_payment_response_reconciles_without_another_recording() {
    let fixture = fixture().await;
    for provider in ["stripe", "paypal"] {
        let upstream = Providers::start().await;
        let client = configured(&upstream).await;
        let input = purchase(&fixture, provider).await;
        accepted(&client, &input).await;
        upstream.pay();
        upstream.fault("RecordCheckoutPayment", "lost");
        let checkout = input["operation_id"].as_str().unwrap();
        let response = client.send(verify(checkout, Some(SECRET))).await.unwrap();
        assert_eq!(response.status(), 200);
        let first = body_json(response).await;
        assert!(matches!(
            first["status"].as_str(),
            Some("completed" | "reconciliation_required")
        ));
        upstream.clear_fault();
        let response = client.send(verify(checkout, Some(SECRET))).await.unwrap();
        assert_eq!(response.status(), 200);
        assert_eq!(body_json(response).await["status"], "completed");
        assert_eq!(upstream.count("RecordCheckoutPayment"), 1);
        assert_eq!(upstream.count("CompleteCheckoutDraft"), 1);
        assert_eq!(upstream.payment_creates(), 1);
    }
}

#[tokio::test]
async fn untouched_legacy_checkout_replays_original_preparation_and_completes_once() {
    use sha2::Digest;
    let fixture = fixture().await;
    for (provider, version, reject_draft) in [
        ("stripe", 1_i16, false),
        ("paypal", 1_i16, false),
        ("stripe", 1_i16, true),
        ("paypal", 1_i16, true),
        ("stripe", 2_i16, true),
        ("paypal", 2_i16, true),
    ] {
        let upstream = Providers::start().await;
        let client = configured(&upstream).await;
        let input = purchase(&fixture, provider).await;
        let checkout = input["operation_id"].as_str().unwrap();
        upstream.fault("CatalogueProducts", "http");
        let response = client
            .send(request(CHECKOUT, Some(SECRET), &input))
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        assert_eq!(upstream.count("CreateDraftOrder"), 0);
        let lines = std::collections::BTreeMap::from([(
            "gid://shopify/ProductVariant/2".to_owned(),
            2_i64,
        )]);
        let money = domain::dto::checkout::CheckoutMoney {
            minor: 4000,
            currency: "SGD".to_owned(),
        };
        let slug: String = sqlx::query_scalar(
            "SELECT slug
             FROM conventions
             WHERE id = $1",
        )
        .bind(Uuid::parse_str(input["convention_id"].as_str().unwrap()).unwrap())
        .fetch_one(&fixture.pool())
        .await
        .unwrap();
        let legacy_hash = if version == 1 {
            sha2::Sha256::digest(serde_json::to_vec(&("buyer@example.test", money, lines)).unwrap())
                .to_vec()
        } else {
            sqlx::query_scalar(
                "SELECT evidence_hash
             FROM checkouts
             WHERE id = $1",
            )
            .bind(Uuid::parse_str(checkout).unwrap())
            .fetch_one(&fixture.pool())
            .await
            .unwrap()
        };
        let legacy_tags = if version == 1 {
            vec![
                format!("cv:{}", input["convention_id"].as_str().unwrap()),
                "E2E convention".to_owned(),
                format!("co:{checkout}"),
            ]
        } else {
            vec![slug.chars().take(40).collect()]
        };
        let legacy_tag_hashes: Vec<Vec<u8>> = legacy_tags
            .iter()
            .map(|tag| sha2::Sha256::digest(tag.as_bytes()).to_vec())
            .collect();
        sqlx::query(
            "UPDATE checkouts
             SET payment_state = 'not_started',
                 completion_version = $4,
                 correlation_version = $4,
                 convention_slug = $5,
                 payment_amount_minor = NULL,
                 payment_currency = NULL,
                 shopify_amount_minor = NULL,
                 shopify_currency = NULL,
                 evidence_hash = $2,
                 required_tag_hashes = $3
             WHERE id = $1",
        )
        .bind(Uuid::parse_str(checkout).unwrap())
        .bind(&legacy_hash)
        .bind(&legacy_tag_hashes)
        .bind(version)
        .bind((version == 2).then_some(&slug))
        .execute(&fixture.pool())
        .await
        .unwrap();
        let original: (Vec<u8>, Vec<Vec<u8>>) = sqlx::query_as(
            "SELECT request_fingerprint, required_tag_hashes
             FROM checkouts
             WHERE id = $1",
        )
        .bind(Uuid::parse_str(checkout).unwrap())
        .fetch_one(&fixture.pool())
        .await
        .unwrap();
        upstream.clear_fault();
        if reject_draft {
            upstream.fault("CreateDraftOrder", "user_error");
            let response = client
                .send(request(CHECKOUT, Some(SECRET), &input))
                .await
                .unwrap();
            assert_eq!(response.status(), 502);
            assert_eq!(
                body_json(response).await,
                json!({"error":"Upstream request failed"})
            );
            let states: (String, String) = sqlx::query_as(
                "SELECT draft_state, payment_state
             FROM checkouts
             WHERE id = $1",
            )
            .bind(Uuid::parse_str(checkout).unwrap())
            .fetch_one(&fixture.pool())
            .await
            .unwrap();
            assert_eq!(states, ("uncertain".into(), "not_started".into()));
            upstream.clear_fault();
            assert_eq!(
                client
                    .send(verify(checkout, Some(SECRET)))
                    .await
                    .unwrap()
                    .status(),
                409
            );
            assert_eq!(upstream.count("CreateDraftOrder"), 1);
            assert_eq!(upstream.payment_creates(), 0);
            continue;
        }
        let created = accepted(&client, &input).await;
        assert_eq!(created["status"], "requires_action");
        upstream.pay();
        let response = client.send(verify(checkout, Some(SECRET))).await.unwrap();
        assert_eq!(response.status(), 200);
        assert_eq!(body_json(response).await["status"], "completed");
        let calls = upstream.calls();
        let draft = &calls
            .iter()
            .find(|call| call["operation"] == "CreateDraftOrder")
            .unwrap()["body"]["variables"]["input"];
        assert_eq!(draft["tags"], json!(legacy_tags));
        assert!(draft.get("customAttributes").is_none());
        let converted = calls
            .iter()
            .find(|call| call["operation"] == "CompleteCheckoutDraft")
            .unwrap();
        assert_eq!(converted["body"]["variables"]["paymentPending"], false);
        assert_eq!(upstream.count("CreateDraftOrder"), 1);
        assert_eq!(upstream.count("CompleteCheckoutDraft"), 1);
        assert_eq!(upstream.count("RecordCheckoutPayment"), 0);
        assert_eq!(upstream.payment_creates(), 1);
        let preserved: (i16, Vec<u8>, Vec<u8>, Vec<Vec<u8>>) = sqlx::query_as(
            "SELECT completion_version, evidence_hash, request_fingerprint, required_tag_hashes
             FROM checkouts
             WHERE id = $1",
        )
        .bind(Uuid::parse_str(checkout).unwrap())
        .fetch_one(&fixture.pool())
        .await
        .unwrap();
        assert_eq!(preserved.0, 1);
        assert_eq!(preserved.1, legacy_hash);
        assert_eq!(preserved.2, original.0);
        assert_eq!(preserved.3, original.1);
    }
}

#[tokio::test]
async fn external_payment_recording_failures_preserve_paid_truth_without_retrying_mutation() {
    let fixture = fixture().await;
    for provider in ["stripe", "paypal"] {
        for failure in [
            "http",
            "malformed",
            "missing",
            "graphql",
            "timeout",
            "gateway_rejected",
            "wrong_record_identity",
            "recorded_other",
        ] {
            let upstream = Providers::start().await;
            let client = configured(&upstream).await;
            let input = purchase(&fixture, provider).await;
            accepted(&client, &input).await;
            upstream.pay();
            upstream.fault("RecordCheckoutPayment", failure);
            let checkout = input["operation_id"].as_str().unwrap();
            let response = client.send(verify(checkout, Some(SECRET))).await.unwrap();
            assert_eq!(response.status(), 200, "{provider}/{failure}");
            let body = body_json(response).await;
            assert_eq!(
                body["status"], "reconciliation_required",
                "{provider}/{failure}: {body}"
            );
            assert!(body["next_action"].is_null());
            assert!(body["order"].is_null());
            assert!(
                !body
                    .to_string()
                    .contains("Synthetic configured gateway unavailable")
            );
            let payment: String = sqlx::query_scalar(
                "SELECT payment_state
                 FROM checkouts
                 WHERE id = $1",
            )
            .bind(Uuid::parse_str(checkout).unwrap())
            .fetch_one(&fixture.pool())
            .await
            .unwrap();
            assert_eq!(payment, "paid");
            upstream.clear_fault();
            let response = client.send(verify(checkout, Some(SECRET))).await.unwrap();
            assert_eq!(response.status(), 200);
            assert_eq!(
                body_json(response).await["status"],
                "reconciliation_required"
            );
            assert_eq!(upstream.count("CreateDraftOrder"), 1);
            assert_eq!(upstream.count("CompleteCheckoutDraft"), 1);
            assert_eq!(upstream.count("RecordCheckoutPayment"), 1);
            assert_eq!(upstream.payment_creates(), 1);
        }
    }
}

#[tokio::test]
async fn checkout_provider_reference_write_failures_recover_same_manual_order_without_recording_again()
 {
    let fixture = fixture().await;
    for provider in ["stripe", "paypal"] {
        for failure in ["http", "malformed", "missing", "graphql", "timeout", "lost"] {
            let upstream = Providers::start().await;
            let client = configured(&upstream).await;
            let input = purchase(&fixture, provider).await;
            accepted(&client, &input).await;
            upstream.pay();
            upstream.fault("UpdateOrderCustomAttributes", failure);
            let checkout = input["operation_id"].as_str().unwrap();
            let response = client.send(verify(checkout, Some(SECRET))).await.unwrap();
            assert_eq!(response.status(), 200, "{provider}/{failure}");
            let first = body_json(response).await;
            assert!(
                matches!(
                    first["status"].as_str(),
                    Some("completed" | "reconciliation_required")
                ),
                "{provider}/{failure}: {first}"
            );
            upstream.clear_fault();
            let response = client.send(verify(checkout, Some(SECRET))).await.unwrap();
            assert_eq!(response.status(), 200);
            assert_eq!(
                body_json(response).await["status"],
                "completed",
                "{provider}/{failure}"
            );
            assert_eq!(upstream.count("CreateDraftOrder"), 1);
            assert_eq!(upstream.count("CompleteCheckoutDraft"), 1);
            assert_eq!(upstream.count("RecordCheckoutPayment"), 1);
            assert_eq!(upstream.count("EditOrderTags"), 0);
            assert_eq!(upstream.payment_creates(), 1);
        }
    }
}

#[tokio::test]
async fn checkout_customer_note_replays_exactly_and_completes_same_draft_for_both_providers() {
    let fixture = fixture().await;
    let note = "  Please sign\nありがとう 🐶\t<gift>\r\n  ";
    for provider in ["stripe", "paypal"] {
        let upstream = Providers::start().await;
        let client = configured(&upstream).await;
        let mut input = purchase(&fixture, provider).await;
        input["note"] = json!(note);
        let first = accepted(&client, &input).await;
        assert_eq!(accepted(&client, &input).await, first);
        let creates: Vec<_> = upstream
            .calls()
            .into_iter()
            .filter(|call| call["operation"] == "CreateDraftOrder")
            .collect();
        assert_eq!(creates.len(), 1);
        assert_eq!(creates[0]["body"]["variables"]["input"]["note"], note);
        let before = upstream.calls().len();
        let mut changed = input.clone();
        changed["note"] = json!("Different instructions");
        let response = client
            .send(request(CHECKOUT, Some(SECRET), &changed))
            .await
            .unwrap();
        assert_eq!(response.status(), 409);
        assert_eq!(upstream.calls().len(), before);
        upstream.pay();
        let id = input["operation_id"].as_str().unwrap();
        let response = client.send(verify(id, Some(SECRET))).await.unwrap();
        assert_eq!(response.status(), 200);
        assert_eq!(body_json(response).await["status"], "completed");
        assert_eq!(upstream.count("CreateDraftOrder"), 1);
        assert_eq!(upstream.count("CompleteCheckoutDraft"), 1);
        assert_eq!(upstream.payment_creates(), 1);
        let stored: Value = sqlx::query_scalar(
            "SELECT to_jsonb(checkout)
             FROM checkouts checkout
             WHERE id = $1",
        )
        .bind(Uuid::parse_str(id).unwrap())
        .fetch_one(&fixture.pool())
        .await
        .unwrap();
        assert!(stored.get("note").is_none());
        assert!(!stored.to_string().contains("ありがとう"));
        assert!(!client.logs().contains("ありがとう"));
        for call in upstream
            .calls()
            .iter()
            .filter(|call| call["payment_create"] == true)
        {
            assert!(!call["body"].to_string().contains("ありがとう"));
        }
    }
}

#[tokio::test]
async fn checkout_accepts_absent_blank_and_unicode_note_limit_without_normalizing_customer_text() {
    let fixture = fixture().await;
    for provider in ["stripe", "paypal"] {
        for note in [
            None,
            Some(String::new()),
            Some(" \t\r\n".into()),
            Some("🐶".repeat(2000)),
        ] {
            let upstream = Providers::start().await;
            let client = configured(&upstream).await;
            let mut input = purchase(&fixture, provider).await;
            if let Some(note) = &note {
                input["note"] = json!(note);
            }
            accepted(&client, &input).await;
            let call = upstream
                .calls()
                .into_iter()
                .find(|call| call["operation"] == "CreateDraftOrder")
                .unwrap();
            let sent = &call["body"]["variables"]["input"];
            match note {
                Some(note) => assert_eq!(sent["note"], note),
                None => assert!(sent.get("note").is_none()),
            }
        }
    }
}

#[tokio::test]
async fn checkout_pins_discovered_stripe_account_and_rejects_switched_credentials() {
    let fixture = fixture().await;
    let upstream = Providers::start().await;
    upstream.stripe_account(json!({"id":"acct_discovered","object":"account"}));
    let client = configured(&upstream).await;
    let input = purchase(&fixture, "stripe").await;
    accepted(&client, &input).await;
    assert_eq!(upstream.calls()[0]["operation"], "/stripe/account");
    let id = Uuid::parse_str(input["operation_id"].as_str().unwrap()).unwrap();
    let stored: (String, String) = sqlx::query_as(
        "SELECT provider_account_id, provider_environment
         FROM checkouts
         WHERE id = $1",
    )
    .bind(id)
    .fetch_one(&fixture.pool())
    .await
    .unwrap();
    assert_eq!(stored, ("acct_discovered".into(), "false".into()));
    let creates = upstream.payment_creates();
    upstream.stripe_account(json!({"id":"acct_changed","object":"account"}));
    let response = client
        .send(verify(&id.to_string(), Some(SECRET)))
        .await
        .unwrap();
    assert_eq!(response.status(), 502);
    assert_eq!(
        body_json(response).await,
        json!({"error":"Upstream request failed"})
    );
    let retained: String = sqlx::query_scalar(
        "SELECT provider_account_id
         FROM checkouts
         WHERE id = $1",
    )
    .bind(id)
    .fetch_one(&fixture.pool())
    .await
    .unwrap();
    assert_eq!(retained, "acct_discovered");
    assert_eq!(upstream.payment_creates(), creates);
    assert_eq!(upstream.count("CompleteCheckoutDraft"), 0);
    upstream.stripe_account(json!({"id":"acct_discovered","object":"account"}));
    let response = client
        .send(verify(&id.to_string(), Some(SECRET)))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(body_json(response).await["status"], "requires_action");
    assert_eq!(upstream.payment_creates(), creates);
}

#[tokio::test]
async fn checkout_unstarted_retry_preserves_original_provider_account() {
    let fixture = fixture().await;
    let upstream = Providers::start().await;
    let client = configured(&upstream).await;
    let input = purchase(&fixture, "stripe").await;
    upstream.fault("Location", "http");
    let response = client
        .send(request(CHECKOUT, Some(SECRET), &input))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    sqlx::query(
        "UPDATE checkouts
         SET payment_state = 'not_started'
         WHERE id = $1",
    )
    .bind(Uuid::parse_str(input["operation_id"].as_str().unwrap()).unwrap())
    .execute(&fixture.pool())
    .await
    .unwrap();
    upstream.clear_fault();
    upstream.stripe_account(json!({"id":"acct_changed","object":"account"}));
    let response = client
        .send(request(CHECKOUT, Some(SECRET), &input))
        .await
        .unwrap();
    assert_eq!(response.status(), 409);
    let stored: (String, String) = sqlx::query_as(
        "SELECT provider_account_id, draft_state
         FROM checkouts
         WHERE id = $1",
    )
    .bind(Uuid::parse_str(input["operation_id"].as_str().unwrap()).unwrap())
    .fetch_one(&fixture.pool())
    .await
    .unwrap();
    assert_eq!(stored, ("acct_e2e".into(), "not_started".into()));
    assert_eq!(upstream.count("CreateDraftOrder"), 0);
    assert_eq!(upstream.payment_creates(), 0);
    upstream.stripe_account(json!({"id":"acct_e2e","object":"account"}));
    assert_eq!(accepted(&client, &input).await["status"], "requires_action");
    assert_eq!(upstream.count("CreateDraftOrder"), 1);
    assert_eq!(upstream.payment_creates(), 1);
}

#[tokio::test]
async fn checkout_invalid_stripe_account_identity_prevents_all_mutations() {
    let fixture = fixture().await;
    for account in [
        json!({"id":"acct_e2e","object":"customer"}),
        json!({"id":"","object":"account"}),
        json!({"id":"acct_","object":"account"}),
        json!({"id":"cus_other","object":"account"}),
        json!({"id":"acct_invalid/path","object":"account"}),
    ] {
        let upstream = Providers::start().await;
        upstream.stripe_account(account);
        let client = configured(&upstream).await;
        let input = purchase(&fixture, "stripe").await;
        let response = client
            .send(request(CHECKOUT, Some(SECRET), &input))
            .await
            .unwrap();
        assert_eq!(response.status(), 502);
        assert_eq!(
            body_json(response).await,
            json!({"error":"Upstream request failed"})
        );
        assert_eq!(upstream.calls().len(), 1);
        assert_eq!(upstream.count("/stripe/account"), 1);
        let stored: i64 = sqlx::query_scalar(
            "SELECT COUNT(*)
             FROM checkouts
             WHERE id = $1",
        )
        .bind(Uuid::parse_str(input["operation_id"].as_str().unwrap()).unwrap())
        .fetch_one(&fixture.pool())
        .await
        .unwrap();
        assert_eq!(stored, 0);
    }
}

#[tokio::test]
async fn checkout_stripe_webhook_account_dependency_failure_does_not_advance_payment() {
    let _fixture = fixture().await;
    for failure in ["http", "malformed", "missing", "timeout"] {
        let upstream = Providers::start().await;
        let client = configured(&upstream).await;
        let event = upstream.stripe_event(&Uuid::new_v4().to_string(), "evt_account_fault");
        upstream.fault("/stripe/account", failure);
        let response = client
            .send(stripe_webhook(&event, chrono::Utc::now().timestamp()))
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            if failure == "timeout" { 504 } else { 502 }
        );
        assert_eq!(
            body_json(response).await,
            json!({"error":"Upstream request failed"})
        );
        assert_eq!(upstream.count("/stripe/account"), 1);
        assert_eq!(upstream.calls().len(), 1);
    }
}

#[tokio::test]
async fn independent_convention_payment_and_shopify_money_stay_pinned_for_both_gateways() {
    let fixture = fixture().await;
    for provider in ["stripe", "paypal"] {
        for (payment_currency, unit_price, unit_minor, shopify_currency) in [
            ("THB", "20.00", 2000, "AUD"),
            ("SGD", "20.00", 2000, "USD"),
            ("SGD", "20.00", 2000, "SGD"),
            ("JPY", "20", 20, "USD"),
        ] {
            let upstream = Providers::start().await;
            upstream.shopify_money("12.00", shopify_currency);
            let client = configured(&upstream).await;
            let slug = format!("{}-{}", fixture.prefix(), Uuid::new_v4().simple());
            let mut input = purchase_in_currency(
                &fixture,
                provider,
                &slug,
                payment_currency,
                unit_price,
                unit_minor,
            )
            .await;
            input["note"] = json!("Independent currency pickup");
            let convention = Uuid::parse_str(input["convention_id"].as_str().unwrap()).unwrap();
            let first = client
                .send(request(CHECKOUT, Some(SECRET), &input))
                .await
                .unwrap();
            let status = first.status();
            let first = body_json(first).await;
            assert_eq!(status, 200, "{first}");
            let expected = json!({"minor":unit_minor*2,"currency":payment_currency});
            assert_eq!(first["money"], expected);
            assert_eq!(first["status"], "requires_action");
            let checkout = input["operation_id"].as_str().unwrap();
            let snapshots: (i16, i64, String, i64, String) = sqlx::query_as(
                "SELECT correlation_version, payment_amount_minor, payment_currency,
                        shopify_amount_minor, shopify_currency
                 FROM checkouts
                 WHERE id = $1",
            )
            .bind(Uuid::parse_str(checkout).unwrap())
            .fetch_one(&fixture.pool())
            .await
            .unwrap();
            assert_eq!(
                snapshots,
                (
                    3,
                    unit_minor * 2,
                    payment_currency.into(),
                    1200,
                    shopify_currency.into()
                )
            );
            sqlx::query(
                "UPDATE conventions
                 SET preorder_enabled = FALSE
                 WHERE id = $1",
            )
            .bind(convention)
            .execute(&fixture.pool())
            .await
            .unwrap();
            sqlx::query(
                "UPDATE convention_variants
                 SET final_price = 99
                 WHERE convention_id = $1",
            )
            .bind(convention)
            .execute(&fixture.pool())
            .await
            .unwrap();
            upstream.pay();
            for _ in 0..2 {
                let result = client.send(verify(checkout, Some(SECRET))).await.unwrap();
                let status = result.status();
                let result = body_json(result).await;
                assert_eq!(status, 200, "{result}");
                assert_eq!(result["status"], "completed", "{result}");
                assert_eq!(result["money"], expected);
            }
            assert_eq!(upstream.payment_creates(), 1);
            assert_eq!(upstream.count("CreateDraftOrder"), 1);
            assert_eq!(upstream.count("RecordCheckoutPayment"), 1);
            let calls = upstream.calls();
            let draft = &calls
                .iter()
                .find(|call| call["operation"] == "CreateDraftOrder")
                .unwrap()["body"]["variables"]["input"];
            assert!(draft.get("presentmentCurrencyCode").is_none());
            assert!(draft["lineItems"][0].get("priceOverride").is_none());
            assert_eq!(draft["note"], "Independent currency pickup");
            let updated = &calls
                .iter()
                .find(|call| call["operation"] == "UpdateOrderCustomAttributes")
                .unwrap()["body"]["variables"]["input"]["customAttributes"];
            for (key, value) in [
                ("payment_amount_minor", (unit_minor * 2).to_string()),
                ("payment_currency", payment_currency.into()),
                ("shopify_amount_minor", "1200".into()),
                ("shopify_currency", shopify_currency.into()),
            ] {
                assert_eq!(
                    updated
                        .as_array()
                        .unwrap()
                        .iter()
                        .filter(|attribute| attribute["key"] == key && attribute["value"] == value)
                        .count(),
                    1,
                    "{updated}"
                );
            }
        }
    }
}

#[tokio::test]
async fn invalid_or_changed_shopify_snapshot_never_starts_another_payment() {
    let fixture = fixture().await;
    for provider in ["stripe", "paypal"] {
        for amount in ["0.00", "-1.00"] {
            let upstream = Providers::start().await;
            upstream.shopify_money(amount, "AUD");
            let client = configured(&upstream).await;
            let input = purchase(&fixture, provider).await;
            let result = client
                .send(request(CHECKOUT, Some(SECRET), &input))
                .await
                .unwrap();
            assert_eq!(result.status(), 409);
            assert_eq!(upstream.payment_creates(), 0);
            assert_eq!(upstream.count("CreateDraftOrder"), 1);
        }
        for (amount, currency) in [("12.01", "AUD"), ("12.00", "USD")] {
            let upstream = Providers::start().await;
            upstream.shopify_money("12.00", "AUD");
            let client = configured(&upstream).await;
            let input = purchase(&fixture, provider).await;
            accepted(&client, &input).await;
            upstream.shopify_money(amount, currency);
            upstream.pay();
            let result = client
                .send(verify(
                    input["operation_id"].as_str().unwrap(),
                    Some(SECRET),
                ))
                .await
                .unwrap();
            assert_eq!(result.status(), 409);
            assert_eq!(upstream.payment_creates(), 1);
            assert_eq!(upstream.count("RecordCheckoutPayment"), 0);
            assert_eq!(upstream.count("CompleteCheckoutDraft"), 0);
        }
    }
}

#[tokio::test]
async fn shopify_draft_rejection_retains_private_diagnostics_without_starting_gateway_payment() {
    let fixture = fixture().await;
    let upstream = Providers::start().await;
    let client = configured(&upstream).await;
    let input = purchase(&fixture, "stripe").await;
    upstream.fault("CreateDraftOrder", "user_error");
    let response = client
        .send(request(CHECKOUT, Some(SECRET), &input))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(body_json(response).await["status"], "unpaid");
    upstream.clear_fault();
    let calls = upstream.calls().len();
    assert_eq!(accepted(&client, &input).await["status"], "unpaid");
    let response = client
        .send(verify(
            input["operation_id"].as_str().unwrap(),
            Some(SECRET),
        ))
        .await
        .unwrap();
    assert_eq!(body_json(response).await["status"], "unpaid");
    assert_eq!(upstream.calls().len(), calls);
    assert_eq!(upstream.payment_creates(), 0);
    assert_eq!(upstream.count("CreateDraftOrder"), 1);
    assert!(!client.logs().contains("buyer@example.test"));
}

#[tokio::test]
async fn checkout_stripe_thb_minimum_precedes_checkout_and_draft_mutations() {
    let fixture = fixture().await;
    for (minor, accepted_total) in [(999, false), (1000, true)] {
        let upstream = Providers::start().await;
        upstream
            .stripe_account(json!({"id":"acct_e2e","object":"account","default_currency":"thb"}));
        let client = configured(&upstream).await;
        let mut input = purchase_in_currency(
            &fixture,
            "stripe",
            &format!("minimum-{}", Uuid::new_v4()),
            "THB",
            &format!("{}.{:02}", minor / 100, minor % 100),
            minor,
        )
        .await;
        input["items"][0]["quantity"] = json!(1);
        let response = client
            .send(request(CHECKOUT, Some(SECRET), &input))
            .await
            .unwrap();
        assert_eq!(response.status(), if accepted_total { 200 } else { 422 });
        if !accepted_total {
            assert_eq!(
                body_json(response).await,
                json!({"error":"Stripe requires a minimum payment of THB 10.00"})
            );
            let rows: i64 = sqlx::query_scalar(
                "SELECT COUNT(*)
         FROM checkouts
         WHERE id = $1",
            )
            .bind(Uuid::parse_str(input["operation_id"].as_str().unwrap()).unwrap())
            .fetch_one(&fixture.pool())
            .await
            .unwrap();
            assert_eq!(rows, 0);
            assert_eq!(upstream.count("CreateDraftOrder"), 0);
            assert_eq!(upstream.payment_creates(), 0);
        }
    }
}

#[tokio::test]
async fn checkout_definitive_create_rejections_are_terminal_without_recreation() {
    let fixture = fixture().await;
    for provider in ["stripe", "paypal"] {
        for failure in ["status_400", "status_401", "status_403", "status_422"] {
            let upstream = Providers::start().await;
            let client = configured(&upstream).await;
            let input = purchase(&fixture, provider).await;
            upstream.fault(
                if provider == "stripe" {
                    "/stripe/checkout/sessions"
                } else {
                    "/v2/checkout/orders"
                },
                failure,
            );
            let first = accepted(&client, &input).await;
            assert_eq!(first["status"], "unpaid");
            upstream.clear_fault();
            let calls = upstream.calls().len();
            assert_eq!(accepted(&client, &input).await["status"], "unpaid");
            assert_eq!(
                body_json(
                    client
                        .send(verify(
                            input["operation_id"].as_str().unwrap(),
                            Some(SECRET)
                        ))
                        .await
                        .unwrap()
                )
                .await["status"],
                "unpaid"
            );
            assert_eq!(upstream.calls().len(), calls);
            assert_eq!(
                upstream.payment_creates(),
                if provider == "paypal" && failure == "status_401" {
                    2
                } else {
                    1
                }
            );
        }
    }
}

#[tokio::test]
async fn checkout_uncertain_gateway_rejection_never_becomes_safe_to_replace() {
    let fixture = fixture().await;
    for provider in ["stripe", "paypal"] {
        let upstream = Providers::start().await;
        let client = configured(&upstream).await;
        let input = purchase(&fixture, provider).await;
        let operation = if provider == "stripe" {
            "/stripe/checkout/sessions"
        } else {
            "/v2/checkout/orders"
        };
        upstream.fault(operation, "lost");
        assert_eq!(
            client
                .send(request(CHECKOUT, Some(SECRET), &input))
                .await
                .unwrap()
                .status(),
            502
        );
        upstream.fault(operation, "status_422");
        assert_eq!(
            client
                .send(request(CHECKOUT, Some(SECRET), &input))
                .await
                .unwrap()
                .status(),
            502
        );
        let stored: (String, Option<String>) = sqlx::query_as(
            "SELECT payment_state, provider_resource_id
         FROM checkouts
         WHERE id = $1",
        )
        .bind(Uuid::parse_str(input["operation_id"].as_str().unwrap()).unwrap())
        .fetch_one(&fixture.pool())
        .await
        .unwrap();
        assert_eq!(stored, ("creating".into(), None));
        assert_eq!(upstream.count("CreateDraftOrder"), 1);
    }
}

#[tokio::test]
async fn checkout_old_creating_minimum_rejection_preserves_original_operation() {
    let fixture = fixture().await;
    let upstream = Providers::start().await;
    let client = configured(&upstream).await;
    let mut input = purchase_in_currency(
        &fixture,
        "stripe",
        &format!("old-min-{}", Uuid::new_v4()),
        "THB",
        "1.00",
        100,
    )
    .await;
    input["items"][0]["quantity"] = json!(1);
    upstream.fault("/stripe/checkout/sessions", "lost");
    assert_eq!(
        client
            .send(request(CHECKOUT, Some(SECRET), &input))
            .await
            .unwrap()
            .status(),
        502
    );
    let checkout = Uuid::parse_str(input["operation_id"].as_str().unwrap()).unwrap();
    let before: Value = sqlx::query_scalar(
        "SELECT to_jsonb(c) - 'claim_token' - 'claim_expires_at' - 'updated_at'
         FROM checkouts c
         WHERE id = $1",
    )
    .bind(checkout)
    .fetch_one(&fixture.pool())
    .await
    .unwrap();
    upstream.clear_fault();
    upstream.stripe_account(json!({"id":"acct_e2e","object":"account","default_currency":"thb"}));
    let response = client
        .send(request(CHECKOUT, Some(SECRET), &input))
        .await
        .unwrap();
    assert_eq!(response.status(), 422);
    assert_eq!(
        body_json(response).await,
        json!({"error":"Stripe requires a minimum payment of THB 10.00"})
    );
    let after: Value = sqlx::query_scalar(
        "SELECT to_jsonb(c) - 'claim_token' - 'claim_expires_at' - 'updated_at'
         FROM checkouts c
         WHERE id = $1",
    )
    .bind(checkout)
    .fetch_one(&fixture.pool())
    .await
    .unwrap();
    assert_eq!(before, after);
    assert_eq!(upstream.payment_creates(), 1);
    assert_eq!(upstream.count("CreateDraftOrder"), 1);
}

#[tokio::test]
async fn checkout_terminal_rejection_save_failure_never_reports_unpaid() {
    let fixture = fixture().await;
    for operation in ["CatalogueProducts", "/stripe/checkout/sessions"] {
        let upstream = Providers::start().await;
        let client = configured(&upstream).await;
        let input = purchase(&fixture, "stripe").await;
        let id = input["operation_id"].as_str().unwrap();
        let prefix = fixture.prefix();
        sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
            "CREATE FUNCTION {prefix}_checkout_fault() RETURNS TRIGGER
         LANGUAGE plpgsql AS $$
         BEGIN
             IF NEW.id = '{id}'::UUID AND NEW.payment_state = 'unpaid' THEN
                 RAISE EXCEPTION 'Synthetic owned terminal checkpoint failure';
             END IF;
             RETURN NEW;
         END;
         $$;
         CREATE TRIGGER {prefix}_checkout_fault
             BEFORE UPDATE ON checkouts
             FOR EACH ROW
             EXECUTE FUNCTION {prefix}_checkout_fault();"
        )))
        .execute(&fixture.pool())
        .await
        .unwrap();
        upstream.fault(
            operation,
            if operation == "CatalogueProducts" {
                "http"
            } else {
                "status_422"
            },
        );
        let response = client
            .send(request(CHECKOUT, Some(SECRET), &input))
            .await
            .unwrap();
        assert_eq!(response.status(), 500);
        assert_eq!(
            body_json(response).await,
            json!({"error":"Internal server error"})
        );
        let state: String = sqlx::query_scalar(
            "SELECT payment_state
         FROM checkouts
         WHERE id = $1",
        )
        .bind(Uuid::parse_str(id).unwrap())
        .fetch_one(&fixture.pool())
        .await
        .unwrap();
        assert_eq!(
            state,
            if operation == "CatalogueProducts" {
                "not_started"
            } else {
                "creating"
            }
        );
        sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
            "DROP TRIGGER {prefix}_checkout_fault ON checkouts;
         DROP FUNCTION {prefix}_checkout_fault();"
        )))
        .execute(&fixture.pool())
        .await
        .unwrap();
        let response = client
            .send(request(CHECKOUT, Some(SECRET), &input))
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            if operation == "CatalogueProducts" {
                200
            } else {
                502
            }
        );
        let state: String = sqlx::query_scalar(
            "SELECT payment_state
         FROM checkouts
         WHERE id = $1",
        )
        .bind(Uuid::parse_str(id).unwrap())
        .fetch_one(&fixture.pool())
        .await
        .unwrap();
        assert_eq!(
            state,
            if operation == "CatalogueProducts" {
                "unpaid"
            } else {
                "creating"
            }
        );
    }
}

#[tokio::test]
async fn checkout_draft_with_errors_remains_uncertain_and_recovers_same_draft() {
    let fixture = fixture().await;
    let upstream = Providers::start().await;
    let client = configured(&upstream).await;
    let input = purchase(&fixture, "stripe").await;
    upstream.fault("CreateDraftOrder", "draft_and_errors");
    let response = client
        .send(request(CHECKOUT, Some(SECRET), &input))
        .await
        .unwrap();
    assert_eq!(response.status(), 502);
    let states: (String, String) = sqlx::query_as(
        "SELECT draft_state, payment_state
         FROM checkouts
         WHERE id = $1",
    )
    .bind(Uuid::parse_str(input["operation_id"].as_str().unwrap()).unwrap())
    .fetch_one(&fixture.pool())
    .await
    .unwrap();
    assert_eq!(states, ("uncertain".into(), "not_started".into()));
    assert_eq!(upstream.payment_creates(), 0);
    upstream.clear_fault();
    assert_eq!(accepted(&client, &input).await["status"], "requires_action");
    assert_eq!(upstream.count("CreateDraftOrder"), 1);
    assert_eq!(upstream.payment_creates(), 1);
}
