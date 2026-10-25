//! Recovery through authenticated HTTP cron and the owned VPS process.
#![allow(clippy::too_many_lines)]

use crate::common::checkout::{
    CHECKOUT, SECRET, accepted, providers::Providers, purchase, request, verify,
};
use crate::common::{self, ProviderConfig, TestClient, TestFixture, body_json};
use axum::{body::Body, http::Request};
use serde_json::Value;
use std::time::{Duration, Instant};
use uuid::Uuid;

#[tokio::test]
async fn both_gateways_complete_without_browser_or_webhook_with_reports_disabled() {
    let fixture = common::fixture().await;
    for provider in ["stripe", "paypal"] {
        let upstream = Providers::start().await;
        let client = configured(&upstream, ProviderConfig::default()).await;
        let input = purchase(&fixture, provider).await;
        accepted(&client, &input).await;
        let checkout = id(&input);
        let original = stored(&fixture, checkout).await;
        upstream.pay();
        due(&fixture, checkout).await;
        expired_token(&fixture).await;
        tick(&client).await;
        let completed = stored(&fixture, checkout).await;
        assert_eq!(completed["completion_state"], "completed", "{completed}");
        assert_eq!(completed["payment_state"], "paid");
        assert_eq!(completed["completion_version"], 2);
        assert_eq!(completed["correlation_version"], 2);
        assert_eq!(upstream.count("EditOrderTags"), 0);
        assert_eq!(upstream.count("UpdateOrderCustomAttributes"), 1);
        assert_eq!(completed["payment_recording_state"], "recorded");
        assert_eq!(upstream.count("RecordCheckoutPayment"), 1);
        assert_eq!(
            completed["provider_resource_id"],
            original["provider_resource_id"]
        );
        assert_eq!(completed["draft_id"], original["draft_id"]);
        assert!(!completed["order_id"].is_null());
        assert!(completed["next_recovery_at"].is_null());
        assert!(completed["claim_token"].is_null());
        assert_eq!(completed["recovery_failures"], 0);
        assert_eq!(upstream.payment_creates(), 1);
        assert_eq!(upstream.count("CreateDraftOrder"), 1);
        assert_eq!(upstream.count("CompleteCheckoutDraft"), 1);
        assert_eq!(captures(&upstream), usize::from(provider == "paypal"));
        assert_eq!(token_count(&fixture).await, 0);
        assert_private_diagnostics(&client.logs(), &completed);
        tick(&client).await;
        assert_eq!(upstream.count("CompleteCheckoutDraft"), 1);
        assert_eq!(captures(&upstream), usize::from(provider == "paypal"));
    }
}

#[tokio::test]
async fn vercel_waits_for_authenticated_get_and_pending_observation_resets_failures() {
    let fixture = common::fixture().await;
    let upstream = Providers::start().await;
    let client = configured(&upstream, ProviderConfig::default()).await;
    let input = purchase(&fixture, "stripe").await;
    accepted(&client, &input).await;
    let checkout = id(&input);
    due(&fixture, checkout).await;
    sqlx::query(
        "UPDATE checkouts
         SET recovery_failures = 5
         WHERE id = $1",
    )
    .bind(checkout)
    .execute(&fixture.pool())
    .await
    .unwrap();
    let before = upstream.calls().len();
    tokio::time::sleep(Duration::from_millis(1100)).await;
    assert_eq!(upstream.calls().len(), before);
    for (method, authorization, status) in [
        ("GET", "Bearer invalid", 401),
        ("HEAD", "Bearer e2e-cron-secret", 405),
    ] {
        let response = client
            .send(
                Request::builder()
                    .method(method)
                    .uri("/_cron")
                    .header("authorization", authorization)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), status);
        assert_eq!(upstream.calls().len(), before);
    }
    tick(&client).await;
    let row = stored(&fixture, checkout).await;
    assert_eq!(row["recovery_failures"], 0);
    assert_eq!(row["recovery_manual"], false);
    assert_eq!(row["recovery_code"], "waiting_for_payment");
    assert!(future_due(&fixture, checkout).await);
    assert_eq!(upstream.count("CompleteCheckoutDraft"), 0);
    assert_eq!(captures(&upstream), 0);
}

#[tokio::test]
async fn vps_recovery_runs_with_reports_disabled_and_shutdown_stops_owned_work() {
    let fixture = common::fixture().await;
    let upstream = Providers::start().await;
    let creator = configured(&upstream, ProviderConfig::default()).await;
    let input = purchase(&fixture, "paypal").await;
    accepted(&creator, &input).await;
    let checkout = id(&input);
    upstream.pay();
    due(&fixture, checkout).await;
    drop(creator);
    let client = configured(
        &upstream,
        ProviderConfig {
            vps_cron: true,
            recovery_wake_secs: Some(1),
            ..Default::default()
        },
    )
    .await;
    wait_state(&fixture, checkout, "completed").await;
    let (success, logs) = client.shutdown();
    assert!(success, "{logs}");
    assert!(logs.contains("Cron runtime tasks stopped"));
    assert_eq!(upstream.count("CompleteCheckoutDraft"), 1);
    assert_eq!(upstream.count("RecordCheckoutPayment"), 1);
    assert_eq!(upstream.count("EditOrderTags"), 0);
    assert_eq!(upstream.count("UpdateOrderCustomAttributes"), 1);
    assert_eq!(
        stored(&fixture, checkout).await["payment_recording_state"],
        "recorded"
    );
    let before = upstream.calls().len();
    let saved = stored(&fixture, checkout).await;
    tokio::time::sleep(Duration::from_millis(1200)).await;
    assert_eq!(upstream.calls().len(), before);
    assert_eq!(stored(&fixture, checkout).await, saved);

    let blocked = Providers::start().await;
    let creator = configured(&blocked, ProviderConfig::default()).await;
    let input = purchase(&fixture, "paypal").await;
    accepted(&creator, &input).await;
    let checkout = id(&input);
    drop(creator);
    blocked.pay();
    due(&fixture, checkout).await;
    let gate = blocked.gate("PaypalOrder");
    let client = configured(
        &blocked,
        ProviderConfig {
            vps_cron: true,
            recovery_wake_secs: Some(1),
            ..Default::default()
        },
    )
    .await;
    gate.entered().await;
    let (success, logs) = client.shutdown();
    assert!(success, "{logs}");
    assert!(logs.contains("Cron runtime tasks stopped"));
    let saved = stored(&fixture, checkout).await;
    let calls = blocked.calls().len();
    gate.release();
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(stored(&fixture, checkout).await, saved);
    assert_eq!(blocked.calls().len(), calls);
    assert_eq!(captures(&blocked), 0);
    assert_eq!(blocked.count("CompleteCheckoutDraft"), 0);
}

#[tokio::test]
async fn expired_guest_capability_does_not_prevent_internal_recovery() {
    let fixture = common::fixture().await;
    let upstream = Providers::start().await;
    let client = configured(&upstream, ProviderConfig::default()).await;
    let input = purchase(&fixture, "stripe").await;
    accepted(&client, &input).await;
    let checkout = id(&input);
    sqlx::query(
        "UPDATE checkouts
         SET owner_expires_at = NOW() - INTERVAL '1 second'
         WHERE id = $1",
    )
    .bind(checkout)
    .execute(&fixture.pool())
    .await
    .unwrap();
    assert_eq!(
        client
            .send(verify(&checkout.to_string(), Some(SECRET)))
            .await
            .unwrap()
            .status(),
        401
    );
    upstream.pay();
    due(&fixture, checkout).await;
    tick(&client).await;
    assert_eq!(
        stored(&fixture, checkout).await["completion_state"],
        "completed"
    );
    assert_eq!(
        client
            .send(verify(&checkout.to_string(), Some(SECRET)))
            .await
            .unwrap()
            .status(),
        401
    );
}

#[tokio::test]
async fn missing_purchase_rows_do_not_starve_paid_checkout_or_exceed_batch() {
    let fixture = common::fixture().await;
    let upstream = Providers::start().await;
    let client = configured(
        &upstream,
        ProviderConfig {
            recovery_batch_size: Some(3),
            recovery_concurrency: Some(2),
            ..Default::default()
        },
    )
    .await;
    let input = purchase(&fixture, "stripe").await;
    accepted(&client, &input).await;
    let checkout = id(&input);
    let bad = missing_purchase(&fixture, checkout, -30).await;
    due(&fixture, checkout).await;
    upstream.pay();
    tick(&client).await;
    let stopped = stored(&fixture, bad).await;
    assert_eq!(stopped["recovery_manual"], true);
    assert_eq!(stopped["recovery_code"], "missing_purchase");
    assert_eq!(
        stored(&fixture, checkout).await["completion_state"],
        "completed"
    );
    assert_eq!(upstream.count("CreateDraftOrder"), 1);
    let mut rows = Vec::new();
    for offset in 0..5 {
        rows.push(missing_purchase(&fixture, checkout, -20 + offset).await);
    }
    tick(&client).await;
    let mut stopped = 0;
    for checkout in &rows {
        stopped += usize::from(stored(&fixture, *checkout).await["recovery_manual"] == true);
    }
    assert_eq!(stopped, 3);
    tick(&client).await;
    for checkout in rows {
        assert_eq!(stored(&fixture, checkout).await["recovery_manual"], true);
    }
    assert_eq!(upstream.count("CreateDraftOrder"), 1);
}

#[tokio::test]
async fn recovery_only_starts_configured_concurrency_and_leaves_batch_overflow_due() {
    let fixture = common::fixture().await;
    let upstream = Providers::start().await;
    let client = configured(
        &upstream,
        ProviderConfig {
            recovery_batch_size: Some(3),
            recovery_concurrency: Some(2),
            ..Default::default()
        },
    )
    .await;
    let input = purchase(&fixture, "stripe").await;
    accepted(&client, &input).await;
    let checkout = id(&input);
    upstream.pay();
    let mut rows = vec![checkout];
    for _ in 0..4 {
        let duplicate = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO checkouts (
                 id, guest_secret_hash, owner_expires_at, request_fingerprint, convention_id,
                 provider, provider_account_id, provider_environment, evidence_hash,
                 required_tag_hashes, draft_state, draft_id, payment_state,
                 provider_resource_id, reservation_expires_at, next_recovery_at
             )
             SELECT $2, guest_secret_hash, owner_expires_at, request_fingerprint, convention_id,
                    provider, provider_account_id, provider_environment, evidence_hash,
                    required_tag_hashes, draft_state, $3, payment_state, $4,
                    reservation_expires_at, NOW() - INTERVAL '1 second'
             FROM checkouts
             WHERE id = $1",
        )
        .bind(checkout)
        .bind(duplicate)
        .bind(format!("gid://shopify/DraftOrder/{}", duplicate.as_u128()))
        .bind(format!("cs_{}", duplicate.simple()))
        .execute(&fixture.pool())
        .await
        .unwrap();
        rows.push(duplicate);
    }
    due(&fixture, checkout).await;
    let gate = upstream.gate("CheckoutDraft");
    let task = tokio::spawn({
        let client = client.clone();
        async move { tick(&client).await }
    });
    gate.entered().await;
    gate.entered().await;
    let claimed: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)
FROM checkouts
WHERE id = ANY($1) AND claim_token IS NOT NULL",
    )
    .bind(&rows)
    .fetch_one(&fixture.pool())
    .await
    .unwrap();
    assert_eq!(claimed, 2);
    gate.release();
    task.await.unwrap();
    let due: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)
FROM checkouts
WHERE id = ANY($1) AND next_recovery_at <= NOW() AND NOT recovery_manual",
    )
    .bind(&rows)
    .fetch_one(&fixture.pool())
    .await
    .unwrap();
    assert_eq!(due, 2);
    assert!(upstream.count("CompleteCheckoutDraft") <= 1);
}

#[tokio::test]
async fn concurrent_http_invocations_do_not_repeat_payment_or_completion() {
    let fixture = common::fixture().await;
    let upstream = Providers::start().await;
    let client = configured(&upstream, ProviderConfig::default()).await;
    let input = purchase(&fixture, "paypal").await;
    accepted(&client, &input).await;
    let checkout = id(&input);
    upstream.pay();
    due(&fixture, checkout).await;
    let gate = upstream.gate("PaypalOrder");
    let first = tokio::spawn({
        let client = client.clone();
        async move { tick(&client).await }
    });
    gate.entered().await;
    tick(&client).await;
    assert_eq!(captures(&upstream), 0);
    gate.release();
    first.await.unwrap();
    assert_eq!(
        stored(&fixture, checkout).await["completion_state"],
        "completed"
    );
    assert_eq!(captures(&upstream), 1);
    assert_eq!(upstream.count("CompleteCheckoutDraft"), 1);
}

#[tokio::test]
async fn short_invocation_budget_joins_work_and_preserves_retention_reserve() {
    let fixture = common::fixture().await;
    let upstream = Providers::start().await;
    let client = configured(
        &upstream,
        ProviderConfig {
            invocation_budget_secs: Some(6),
            ..Default::default()
        },
    )
    .await;
    let input = purchase(&fixture, "paypal").await;
    accepted(&client, &input).await;
    let checkout = id(&input);
    upstream.pay();
    due(&fixture, checkout).await;
    expired_token(&fixture).await;
    let gate = upstream.gate("PaypalOrder");
    let started = Instant::now();
    let task = tokio::spawn({
        let client = client.clone();
        async move { tick_status(&client, 503).await }
    });
    gate.entered().await;
    task.await.unwrap();
    assert!(started.elapsed() < Duration::from_secs(6));
    assert_eq!(token_count(&fixture).await, 0);
    assert_eq!(captures(&upstream), 0);
    let saved = stored(&fixture, checkout).await;
    let calls = upstream.calls().len();
    gate.release();
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(stored(&fixture, checkout).await, saved);
    assert_eq!(upstream.calls().len(), calls);
    abandon(&fixture, checkout).await;
    tick(&client).await;
    due(&fixture, checkout).await;
    tick(&client).await;
    assert_eq!(
        stored(&fixture, checkout).await["completion_state"],
        "completed"
    );
}

#[tokio::test]
async fn heartbeat_renews_short_lease_while_provider_read_is_waiting() {
    let fixture = common::fixture().await;
    let upstream = Providers::start().await;
    let client = configured(
        &upstream,
        ProviderConfig {
            recovery_lease_secs: Some(1),
            ..Default::default()
        },
    )
    .await;
    let input = purchase(&fixture, "paypal").await;
    accepted(&client, &input).await;
    let checkout = id(&input);
    upstream.pay();
    due(&fixture, checkout).await;
    let gate = upstream.gate("PaypalOrder");
    let first = tokio::spawn({
        let client = client.clone();
        async move { tick(&client).await }
    });
    gate.entered().await;
    let initial = stored(&fixture, checkout).await;
    tokio::time::sleep(Duration::from_millis(1500)).await;
    let renewed = stored(&fixture, checkout).await;
    assert_eq!(renewed["claim_token"], initial["claim_token"]);
    assert!(
        renewed["claim_expires_at"].as_str().unwrap()
            > initial["claim_expires_at"].as_str().unwrap()
    );
    tick(&client).await;
    assert_eq!(captures(&upstream), 0);
    gate.release();
    first.await.unwrap();
    assert_eq!(captures(&upstream), 1);
    assert_eq!(upstream.count("CompleteCheckoutDraft"), 1);
}

#[tokio::test]
async fn lost_lease_cancels_work_and_cannot_overwrite_successor_claim() {
    let fixture = common::fixture().await;
    let upstream = Providers::start().await;
    let client = configured(
        &upstream,
        ProviderConfig {
            recovery_lease_secs: Some(1),
            ..Default::default()
        },
    )
    .await;
    let input = purchase(&fixture, "paypal").await;
    accepted(&client, &input).await;
    let checkout = id(&input);
    upstream.pay();
    due(&fixture, checkout).await;
    let gate = upstream.gate("PaypalOrder");
    let first = tokio::spawn({
        let client = client.clone();
        async move { tick_status(&client, 503).await }
    });
    gate.entered().await;
    let successor = Uuid::new_v4();
    sqlx::query(
        "UPDATE checkouts
         SET claim_token = $2, claim_expires_at = NOW() + INTERVAL '1 minute'
         WHERE id = $1",
    )
    .bind(checkout)
    .bind(successor)
    .execute(&fixture.pool())
    .await
    .unwrap();
    first.await.unwrap();
    let saved = stored(&fixture, checkout).await;
    assert_eq!(saved["claim_token"], successor.to_string());
    gate.release();
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(stored(&fixture, checkout).await, saved);
    assert_eq!(captures(&upstream), 0);
    assert_eq!(upstream.count("CompleteCheckoutDraft"), 0);
}

#[tokio::test]
async fn abandoned_claim_backs_off_once_and_failure_cap_stops_automatically() {
    let fixture = common::fixture().await;
    let upstream = Providers::start().await;
    let client = configured(
        &upstream,
        ProviderConfig {
            recovery_max_failures: Some(2),
            ..Default::default()
        },
    )
    .await;
    let input = purchase(&fixture, "stripe").await;
    accepted(&client, &input).await;
    let checkout = id(&input);
    abandon(&fixture, checkout).await;
    let calls = upstream.calls().len();
    tick(&client).await;
    let backed_off = stored(&fixture, checkout).await;
    assert_eq!(backed_off["recovery_failures"], 1);
    assert_eq!(backed_off["recovery_code"], "claim_abandoned");
    assert!(future_due(&fixture, checkout).await);
    tick(&client).await;
    assert_eq!(upstream.calls().len(), calls);
    assert_eq!(stored(&fixture, checkout).await["recovery_failures"], 1);
    abandon(&fixture, checkout).await;
    tick(&client).await;
    let stopped = stored(&fixture, checkout).await;
    assert_eq!(stopped["recovery_failures"], 2);
    assert_eq!(stopped["recovery_manual"], true);
    assert_eq!(stopped["recovery_code"], "attempts_exhausted");
    assert!(stopped["next_recovery_at"].is_null());
    assert_eq!(upstream.calls().len(), calls);
}

#[tokio::test]
async fn lost_gateway_response_replays_original_key_and_payload_without_new_draft() {
    let fixture = common::fixture().await;
    for (provider, operation) in [
        ("stripe", "/stripe/checkout/sessions"),
        ("paypal", "/v2/checkout/orders"),
    ] {
        let upstream = Providers::start().await;
        let client = configured(&upstream, ProviderConfig::default()).await;
        let input = purchase(&fixture, provider).await;
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
        let checkout = id(&input);
        due(&fixture, checkout).await;
        tick(&client).await;
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
        assert_eq!(calls[0]["body"], calls[1]["body"]);
        assert_eq!(upstream.count("CreateDraftOrder"), 1);
        assert_eq!(stored(&fixture, checkout).await["recovery_failures"], 0);
    }
}

#[tokio::test]
async fn ambiguous_draft_search_binds_one_exact_match_and_never_creates_second_draft() {
    let fixture = common::fixture().await;
    for search in ["", "empty", "multiple", "mismatch"] {
        let upstream = Providers::start().await;
        let client = configured(&upstream, ProviderConfig::default()).await;
        let input = purchase(&fixture, "stripe").await;
        upstream.fault("CreateDraftOrder", "lost");
        assert_eq!(
            client
                .send(request(CHECKOUT, Some(SECRET), &input))
                .await
                .unwrap()
                .status(),
            502
        );
        upstream.clear_fault();
        upstream.search_mode(search);
        let checkout = id(&input);
        due(&fixture, checkout).await;
        tick(&client).await;
        let row = stored(&fixture, checkout).await;
        assert_eq!(upstream.count("CreateDraftOrder"), 1);
        assert_eq!(upstream.count("DraftOrders"), 1);
        if search.is_empty() {
            assert_eq!(row["recovery_manual"], false, "{row}");
            assert_eq!(upstream.payment_creates(), 1);
        } else {
            assert_eq!(row["recovery_manual"], true, "{search}: {row}");
            assert_eq!(upstream.payment_creates(), 0);
        }
    }
}

#[tokio::test]
async fn ambiguous_capture_and_completion_reconcile_without_repeated_mutation() {
    let fixture = common::fixture().await;
    for operation in [
        "PaypalCaptureOrder",
        "CompleteCheckoutDraft",
        "RecordCheckoutPayment",
        "UpdateOrderCustomAttributes",
    ] {
        let upstream = Providers::start().await;
        let client = configured(&upstream, ProviderConfig::default()).await;
        let input = purchase(&fixture, "paypal").await;
        accepted(&client, &input).await;
        let checkout = id(&input);
        upstream.pay();
        upstream.fault(operation, "lost");
        due(&fixture, checkout).await;
        tick(&client).await;
        upstream.clear_fault();
        due(&fixture, checkout).await;
        tick(&client).await;
        assert_eq!(
            stored(&fixture, checkout).await["completion_state"],
            "completed"
        );
        assert_eq!(captures(&upstream), 1);
        assert_eq!(upstream.count("CompleteCheckoutDraft"), 1);
        assert_eq!(upstream.payment_creates(), 1);
    }
}

#[tokio::test]
async fn unsafe_paid_hold_requires_manual_but_existing_link_can_reconcile() {
    let fixture = common::fixture().await;
    for provider in ["stripe", "paypal"] {
        let upstream = Providers::start().await;
        let client = configured(&upstream, ProviderConfig::default()).await;
        let input = purchase(&fixture, provider).await;
        accepted(&client, &input).await;
        let checkout = id(&input);
        if provider == "stripe" {
            upstream.pay();
        } else {
            upstream.captured();
        }
        sqlx::query(
            "UPDATE checkouts
         SET reservation_expires_at = NOW() - INTERVAL '1 second'
         WHERE id = $1",
        )
        .bind(checkout)
        .execute(&fixture.pool())
        .await
        .unwrap();
        due(&fixture, checkout).await;
        tick(&client).await;
        let row = stored(&fixture, checkout).await;
        assert_eq!(row["payment_state"], "paid");
        assert_eq!(row["recovery_manual"], true);
        assert_eq!(row["recovery_code"], "reservation_unsafe");
        assert_eq!(upstream.count("CompleteCheckoutDraft"), 0);
        assert_eq!(captures(&upstream), 0);
        upstream.complete();
        sqlx::query(
            "UPDATE checkouts
SET recovery_manual = FALSE, next_recovery_at = NOW() - INTERVAL '1 second'
WHERE id = $1",
        )
        .bind(checkout)
        .execute(&fixture.pool())
        .await
        .unwrap();
        tick(&client).await;
        assert_eq!(
            stored(&fixture, checkout).await["completion_state"],
            "completed"
        );
        let response = client
            .send(verify(&checkout.to_string(), Some(SECRET)))
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        assert_eq!(body_json(response).await["status"], "completed");
        let completed = stored(&fixture, checkout).await;
        assert_eq!(completed["recovery_manual"], false);
        assert!(completed["recovery_code"].is_null());
        assert_eq!(upstream.count("CompleteCheckoutDraft"), 0);
    }
}

#[tokio::test]
async fn malformed_reads_retry_but_explicit_evidence_conflict_is_manual() {
    let fixture = common::fixture().await;
    for fault in ["malformed", "missing", "http", "conflict"] {
        let upstream = Providers::start().await;
        let client = configured(&upstream, ProviderConfig::default()).await;
        let input = purchase(&fixture, "stripe").await;
        accepted(&client, &input).await;
        let checkout = id(&input);
        upstream.pay();
        if fault == "conflict" {
            upstream.evidence_conflict();
        } else {
            upstream.fault("StripeSession", fault);
        }
        due(&fixture, checkout).await;
        tick(&client).await;
        let row = stored(&fixture, checkout).await;
        assert_eq!(
            row["recovery_manual"],
            fault == "conflict",
            "{fault}: {row}"
        );
        assert_eq!(
            row["recovery_code"],
            if fault == "conflict" {
                "evidence_conflict"
            } else {
                "transient_failure"
            }
        );
        if fault != "conflict" {
            assert_eq!(row["recovery_failures"], 1);
            assert!(future_due(&fixture, checkout).await);
        }
        assert_eq!(upstream.count("CompleteCheckoutDraft"), 0);
        assert_private_diagnostics(&client.logs(), &row);
        sqlx::query(
            "UPDATE checkouts
         SET recovery_manual = TRUE, next_recovery_at = NULL
         WHERE id = $1",
        )
        .bind(checkout)
        .execute(&fixture.pool())
        .await
        .unwrap();
    }
}

#[tokio::test]
async fn missing_shopify_name_retries_readback_without_repeating_completion() {
    let fixture = common::fixture().await;
    let upstream = Providers::start().await;
    let client = configured(&upstream, ProviderConfig::default()).await;
    let input = purchase(&fixture, "stripe").await;
    accepted(&client, &input).await;
    let checkout = id(&input);
    upstream.pay();
    upstream.missing_order_name(true);
    due(&fixture, checkout).await;
    tick(&client).await;
    let row = stored(&fixture, checkout).await;
    assert_ne!(row["completion_state"], "completed");
    assert_eq!(row["recovery_manual"], false);
    assert_eq!(row["recovery_failures"], 1);
    assert!(future_due(&fixture, checkout).await);
    assert_eq!(upstream.count("CompleteCheckoutDraft"), 1);
    upstream.missing_order_name(false);
    due(&fixture, checkout).await;
    tick(&client).await;
    assert_eq!(
        stored(&fixture, checkout).await["completion_state"],
        "completed"
    );
    assert_eq!(upstream.count("CompleteCheckoutDraft"), 1);
}

async fn configured(upstream: &Providers, overrides: ProviderConfig<'_>) -> TestClient {
    common::client_with_providers(ProviderConfig {
        shopify_url: Some(&format!("{}/graphql.json", upstream.url)),
        stripe_url: Some(&format!("{}/stripe/", upstream.url)),
        paypal_url: Some(&upstream.url),
        checkout_enabled: true,
        recovery_enabled: true,
        scheduler_enabled: Some(false),
        ..overrides
    })
    .await
}

fn id(input: &Value) -> Uuid {
    Uuid::parse_str(input["operation_id"].as_str().unwrap()).unwrap()
}

async fn tick(client: &TestClient) {
    tick_status(client, 204).await;
}

async fn tick_status(client: &TestClient, expected: u16) {
    let response = client
        .send(
            Request::builder()
                .uri("/_cron")
                .header("authorization", "Bearer e2e-cron-secret")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.headers()["cache-control"], "no-store");
    let status = response.status();
    assert_eq!(
        status,
        expected,
        "cron response: {}",
        body_json(response).await
    );
    if expected >= 400 {
        let body = body_json(response).await;
        assert!(
            body["error"]
                .as_str()
                .unwrap()
                .starts_with("Checkout recovery ")
        );
        assert_private_diagnostics(&body.to_string(), &Value::Null);
    }
}

async fn due(fixture: &TestFixture, checkout: Uuid) {
    sqlx::query(
        "UPDATE checkouts
         SET next_recovery_at = NOW() - INTERVAL '1 second'
         WHERE id = $1",
    )
    .bind(checkout)
    .execute(&fixture.pool())
    .await
    .unwrap();
}

async fn stored(fixture: &TestFixture, checkout: Uuid) -> Value {
    sqlx::query_scalar(
        "SELECT to_jsonb(checkout)
         FROM checkouts AS checkout
         WHERE id = $1",
    )
    .bind(checkout)
    .fetch_one(&fixture.pool())
    .await
    .unwrap()
}

async fn future_due(fixture: &TestFixture, checkout: Uuid) -> bool {
    sqlx::query_scalar(
        "SELECT next_recovery_at > NOW() AND claim_token IS NULL
         FROM checkouts
         WHERE id = $1",
    )
    .bind(checkout)
    .fetch_one(&fixture.pool())
    .await
    .unwrap()
}

async fn wait_state(fixture: &TestFixture, checkout: Uuid, state: &str) {
    tokio::time::timeout(Duration::from_secs(8), async {
        while stored(fixture, checkout).await["completion_state"] != state {
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
    })
    .await
    .unwrap();
}

async fn expired_token(fixture: &TestFixture) {
    sqlx::query(
        "INSERT INTO refresh_tokens (admin_id, token_hash, expires_at)
         VALUES ($1, $2, NOW() - INTERVAL '1 day')",
    )
    .bind(fixture.root_id())
    .bind(fixture.username("checkout-expired"))
    .execute(&fixture.pool())
    .await
    .unwrap();
}

async fn token_count(fixture: &TestFixture) -> i64 {
    sqlx::query_scalar(
        "SELECT COUNT(*)
         FROM refresh_tokens
         WHERE admin_id = $1",
    )
    .bind(fixture.root_id())
    .fetch_one(&fixture.pool())
    .await
    .unwrap()
}

async fn abandon(fixture: &TestFixture, checkout: Uuid) {
    sqlx::query(
        "UPDATE checkouts
         SET claim_token = $2, claim_expires_at = NOW() - INTERVAL '1 second',
             next_recovery_at = NOW() - INTERVAL '1 second'
         WHERE id = $1",
    )
    .bind(checkout)
    .bind(Uuid::new_v4())
    .execute(&fixture.pool())
    .await
    .unwrap();
}

async fn missing_purchase(fixture: &TestFixture, source: Uuid, due_offset: i32) -> Uuid {
    let checkout = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO checkouts (
             id, guest_secret_hash, owner_expires_at, request_fingerprint, convention_id,
             provider, provider_account_id, provider_environment, evidence_hash,
             required_tag_hashes, next_recovery_at
         )
         SELECT $2, guest_secret_hash, owner_expires_at, request_fingerprint, convention_id,
                provider, provider_account_id, provider_environment, evidence_hash,
                required_tag_hashes, NOW() + make_interval(secs => $3)
         FROM checkouts
         WHERE id = $1",
    )
    .bind(source)
    .bind(checkout)
    .bind(f64::from(due_offset))
    .execute(&fixture.pool())
    .await
    .unwrap();
    checkout
}

fn captures(upstream: &Providers) -> usize {
    upstream
        .calls()
        .iter()
        .filter(|call| {
            call["path"]
                .as_str()
                .is_some_and(|path| path.ends_with("/capture"))
        })
        .count()
}

fn assert_private_diagnostics(logs: &str, row: &Value) {
    for forbidden in [
        SECRET,
        "buyer@example.test",
        "Bearer",
        "private provider diagnostic",
        "payment_intent_data",
        "purchase_units",
    ] {
        assert!(
            !logs.contains(forbidden),
            "private checkout data in process log"
        );
    }
    for field in ["provider_resource_id", "draft_id", "order_id"] {
        if let Some(value) = row[field].as_str() {
            assert!(
                !logs.contains(value),
                "private provider reference in process log"
            );
        }
    }
}
