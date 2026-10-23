#![allow(clippy::unwrap_used)]
use super::*;
use crate::client::tests::{Fixture, Reply};
use serde_json::json;
use sqlx::postgres::PgPoolOptions;

#[tokio::test]
async fn trusted_verifier_rechecks_provider_and_fails_closed_for_expectations_lease_and_pending() {
    let VerifierFixture {
        fixture,
        config,
        client,
        pool,
        id,
        capture_id,
    } = verifier_fixture().await;
    let evidence = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        verify_completed_payment(
            &pool,
            &client,
            &config,
            true,
            id,
            ExpectedPayment {
                reference: "trusted-ref",
                amount: "12.34",
                currency: "USD",
            },
        ),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(evidence.capture_id, capture_id);
    reject_wrong_expectations(&pool, &client, &config, id).await;
    let busy = Uuid::new_v4();
    repository::claim_operation(
        &pool,
        id,
        PaymentOperation::Reconcile,
        busy,
        Utc::now() + Duration::minutes(1),
    )
    .await
    .unwrap();
    assert!(
        verify_completed_payment(
            &pool,
            &client,
            &config,
            true,
            id,
            ExpectedPayment {
                reference: "trusted-ref",
                amount: "12.34",
                currency: "USD"
            }
        )
        .await
        .is_err()
    );
    assert_eq!(fixture.requests().len(), 3);
    repository::finish_operation(&pool, id, busy, "COMPLETED", None)
        .await
        .unwrap();
    assert!(
        verify_completed_payment(
            &pool,
            &client,
            &config,
            true,
            id,
            ExpectedPayment {
                reference: "trusted-ref",
                amount: "12.34",
                currency: "USD"
            }
        )
        .await
        .is_err()
    );
    assert_eq!(fixture.requests().len(), 5);
    let current = load(&pool, id).await.unwrap();
    assert!(current.verified_at.is_some());
    flag(&pool, &current, "REFUND_OR_REVERSAL_REVIEW")
        .await
        .unwrap();
    assert!(
        verify_completed_payment(
            &pool,
            &client,
            &config,
            true,
            id,
            ExpectedPayment {
                reference: "trusted-ref",
                amount: "12.34",
                currency: "USD"
            }
        )
        .await
        .is_err()
    );
    assert_eq!(fixture.requests().len(), 5);
    cleanup_verifier(&pool, id).await;
}

async fn cleanup_verifier(pool: &PgPool, id: Uuid) {
    sqlx::query("UPDATE paypal_payments SET verified_capture_id=NULL,verified_at=NULL,status='CREATING' WHERE id=$1").bind(id).execute(pool).await.unwrap();
    sqlx::query("DELETE FROM paypal_payment_captures WHERE payment_id=$1")
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM paypal_payments WHERE id=$1")
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
    pool.close().await;
}

async fn reject_wrong_expectations(
    pool: &PgPool,
    client: &PaypalClient,
    config: &PaypalConfig,
    id: Uuid,
) {
    for (reference, amount, currency) in [
        ("wrong", "12.34", "USD"),
        ("trusted-ref", "12.35", "USD"),
        ("trusted-ref", "12.34", "EUR"),
    ] {
        assert!(
            verify_completed_payment(
                pool,
                client,
                config,
                true,
                id,
                ExpectedPayment {
                    reference,
                    amount,
                    currency
                }
            )
            .await
            .is_err()
        );
    }
}

struct VerifierFixture {
    fixture: Fixture,
    config: PaypalConfig,
    client: PaypalClient,
    pool: PgPool,
    id: Uuid,
    capture_id: String,
}

async fn verifier_fixture() -> VerifierFixture {
    let id = Uuid::new_v4();
    let merchant = format!("verifier-{id}");
    let order_id = format!("ORDER-{id}");
    let capture_id = format!("CAPTURE-{id}");
    let capture = json!({"id":capture_id,"status":"COMPLETED","amount":{"currency_code":"USD","value":"12.34"},"final_capture":true,"supplementary_data":{"related_ids":{"order_id":order_id}}});
    let order = json!({"id":order_id,"status":"COMPLETED","purchase_units":[{"reference_id":id.to_string(),"custom_id":"trusted-ref","amount":{"currency_code":"USD","value":"12.34"},"payee":{"merchant_id":merchant},"payments":{"captures":[capture]}}]});
    let mut pending = capture.clone();
    pending["status"] = json!("PENDING");
    let fixture = Fixture::new(vec![
        Reply::json(
            json!({"access_token":"fixture-token","token_type":"Bearer","expires_in":3600}),
        ),
        Reply::json(order.clone()),
        Reply::json(capture),
        Reply::json(order),
        Reply::json(pending),
    ])
    .await;
    let config = PaypalConfig {
        verification_environment: None,
        base_url: fixture.url.join("/").unwrap().to_string(),
        timeout_secs: 1,
        client_id: Some("fixture".into()),
        client_secret: Some("fixture".into()),
        payment: Some(PaypalPaymentConfig {
            environment: PaypalEnvironment::Sandbox,
            merchant_id: merchant.clone(),
            webhook_id: "webhook".into(),
            currencies: vec!["USD".into()],
            return_url: "http://localhost/success".into(),
            cancel_url: "http://localhost/cancel".into(),
        }),
    };
    let client = PaypalClient::new(&config).unwrap();
    let gateway = Gateway::new(&client, &config, true).unwrap();
    let url = std::env::var("E2E_DATABASE_URL")
        .unwrap_or_else(|_| "postgres://e2e:e2e@127.0.0.1:5433/e2e".into());
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    let input = NewPaypalPayment {
        id,
        idempotency_key: Uuid::new_v4(),
        capability_digest: [1; 32],
        reference: "trusted-ref".into(),
        amount: "12.34".into(),
        currency: "USD".into(),
        minor_units: 1234,
        merchant_id: merchant,
        environment: "sandbox".into(),
        api_origin: gateway.origin.clone(),
        create_request_id: Uuid::new_v4(),
        capture_request_id: Uuid::new_v4(),
    };
    repository::create_or_get(&pool, &input).await.unwrap();
    let token = Uuid::new_v4();
    repository::claim_operation(
        &pool,
        id,
        PaymentOperation::Create,
        token,
        Utc::now() + Duration::minutes(1),
    )
    .await
    .unwrap();
    repository::save_order(&pool, id, token, &order_id, None)
        .await
        .unwrap();
    VerifierFixture {
        fixture,
        config,
        client,
        pool,
        id,
        capture_id,
    }
}

#[tokio::test]
async fn gateway_binds_sandbox_fixture_permission_and_rejects_unsafe_destinations() {
    let fixture = Fixture::new(vec![]).await;
    let mut config = PaypalConfig {
        verification_environment: None,
        base_url: fixture.url.join("/").unwrap().to_string(),
        timeout_secs: 1,
        client_id: Some("fixture".into()),
        client_secret: Some("fixture".into()),
        payment: Some(PaypalPaymentConfig {
            environment: PaypalEnvironment::Sandbox,
            merchant_id: "merchant".into(),
            webhook_id: "webhook".into(),
            currencies: vec!["USD".into()],
            return_url: "http://localhost/success".into(),
            cancel_url: "http://localhost/cancel".into(),
        }),
    };
    let client = PaypalClient::new(&config).unwrap();
    assert!(Gateway::new(&client, &config, true).is_ok());
    assert!(Gateway::new(&client, &config, false).is_err());
    config.payment.as_mut().unwrap().environment = PaypalEnvironment::Live;
    assert!(Gateway::new(&client, &config, true).is_err());
    config.payment.as_mut().unwrap().environment = PaypalEnvironment::Sandbox;
    config.payment.as_mut().unwrap().return_url = "http://evil.example/return".into();
    assert!(Gateway::new(&client, &config, true).is_err());
    assert!(!expired(Some(Utc::now() - Duration::hours(5))));
    assert!(expired(Some(Utc::now() - Duration::hours(6))));
}

#[test]
fn terminal_capture_response_is_conflict_and_pending_is_accepted() {
    assert!(capture_response_status("DENIED", false).is_err());
    assert_eq!(
        capture_response_status("PENDING", false).unwrap(),
        StatusCode::ACCEPTED
    );
    assert_eq!(
        capture_response_status("COMPLETED", true).unwrap(),
        StatusCode::OK
    );
}

#[tokio::test]
async fn busy_claim_outcome_survives_concurrent_owner_releasing_lease() {
    let VerifierFixture {
        fixture,
        config,
        client,
        pool,
        id,
        ..
    } = verifier_fixture().await;
    let owner = Uuid::new_v4();
    repository::claim_operation(
        &pool,
        id,
        PaymentOperation::Reconcile,
        owner,
        Utc::now() + Duration::minutes(1),
    )
    .await
    .unwrap()
    .unwrap();
    let gateway = Gateway::new(&client, &config, true).unwrap();
    let payment = load(&pool, id).await.unwrap();
    let outcome = reconcile(&pool, &gateway, &payment, false, None)
        .await
        .unwrap();
    repository::finish_operation(&pool, id, owner, "AWAITING_APPROVAL", None)
        .await
        .unwrap();
    assert!(load(&pool, id).await.unwrap().lease_until.is_none());
    assert_eq!(outcome, ReconciliationOutcome::Busy);
    assert_eq!(fixture.requests(), []);
    cleanup_verifier(&pool, id).await;
}
