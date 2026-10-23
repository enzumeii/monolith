#![allow(clippy::unwrap_used)]
use super::*;
use chrono::SubsecRound;

#[tokio::test]
async fn postgres_payment_replay_lease_fencing_evidence_and_event_recovery() {
    let url = std::env::var("E2E_DATABASE_URL")
        .unwrap_or_else(|_| "postgres://e2e:e2e@127.0.0.1:5433/e2e".into());
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(4)
        .connect(&url)
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    let merchant = format!("task2-{}", Uuid::new_v4());
    let mut input = NewPaypalPayment {
        id: Uuid::new_v4(),
        idempotency_key: Uuid::new_v4(),
        capability_digest: [1; 32],
        reference: format!("reference-{}", Uuid::new_v4()),
        amount: "12.34".into(),
        currency: "USD".into(),
        minor_units: 1234,
        merchant_id: merchant.clone(),
        environment: "sandbox".into(),
        api_origin: "https://api-m.sandbox.paypal.com/".into(),
        create_request_id: Uuid::new_v4(),
        capture_request_id: Uuid::new_v4(),
    };
    let first = assert_immutable_replay(&pool, &mut input).await;
    let (payment, token, stale) = assert_lease_recovery(&pool, &first).await;
    let evidence = assert_capture_persistence(&pool, &payment, token, stale).await;
    assert_duplicate_reference(&pool, &mut input, &first, &evidence).await;
    assert_event_recovery(&pool, &first, &evidence).await;
    cleanup(&pool, &merchant).await;
    pool.close().await;
}

async fn assert_immutable_replay(pool: &PgPool, input: &mut NewPaypalPayment) -> PaypalPayment {
    let (first, created) = create_or_get(pool, input).await.unwrap();
    assert!(created);
    input.amount = "99.99".into();
    input.capability_digest = [2; 32];
    let (replay, created) = create_or_get(pool, input).await.unwrap();
    assert!(!created);
    assert_eq!(replay.id, first.id);
    assert_eq!(replay.amount, "12.34");
    assert_eq!(replay.capability_digest, [1; 32]);
    assert_eq!(replay.create_request_id, first.create_request_id);
    assert_eq!(replay.capture_request_id, first.capture_request_id);
    input.amount = "12.34".into();
    first
}

async fn assert_lease_recovery(
    pool: &PgPool,
    first: &PaypalPayment,
) -> (PaypalPayment, Uuid, Uuid) {
    let merchant = &first.merchant_id;
    let until = Utc::now() + chrono::Duration::minutes(5);
    let stale = Uuid::new_v4();
    let claimed = claim_operation(pool, first.id, PaymentOperation::Create, stale, until)
        .await
        .unwrap()
        .unwrap();
    let operation: String = sqlx::query_scalar("SELECT operation FROM paypal_payments WHERE id=$1")
        .bind(claimed.id)
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(operation, "CREATE");
    assert!(claimed.create_started_at.is_some());
    assert!(
        claim_operation(
            pool,
            first.id,
            PaymentOperation::Create,
            Uuid::new_v4(),
            until
        )
        .await
        .unwrap()
        .is_none()
    );
    sqlx::query(
        "UPDATE paypal_payments SET lease_until=clock_timestamp()-interval '1 second' WHERE id=$1",
    )
    .bind(first.id)
    .execute(pool)
    .await
    .unwrap();
    let token = Uuid::new_v4();
    let recovered = claim_operation(pool, first.id, PaymentOperation::Create, token, until)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(recovered.create_started_at, claimed.create_started_at);
    assert!(
        !save_order(pool, first.id, stale, "wrong-order", None)
            .await
            .unwrap()
    );
    assert!(
        save_order(
            pool,
            first.id,
            token,
            "order-one",
            Some("https://www.sandbox.paypal.com/checkoutnow?token=order-one")
        )
        .await
        .unwrap()
    );
    let payment = find_by_order(pool, merchant, "sandbox", "order-one")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(payment.id, first.id);
    assert_eq!(payment.status, "AWAITING_APPROVAL");
    assert!(payment.lease_token.is_none());
    let token = Uuid::new_v4();
    assert!(
        claim_operation(pool, first.id, PaymentOperation::Capture, token, until)
            .await
            .unwrap()
            .unwrap()
            .capture_started_at
            .is_some()
    );
    (payment, token, stale)
}

async fn assert_capture_persistence(
    pool: &PgPool,
    payment: &PaypalPayment,
    token: Uuid,
    stale: Uuid,
) -> CaptureEvidence {
    let first = payment;
    let merchant = &payment.merchant_id;
    let evidence = CaptureEvidence {
        capture_id: format!("capture-{}", Uuid::new_v4()),
        payment_id: first.id,
        merchant_id: merchant.clone(),
        environment: "sandbox".into(),
        paypal_order_id: "order-one".into(),
        amount: "12.34".into(),
        currency: "USD".into(),
        minor_units: 1234,
        provider_status: "COMPLETED".into(),
        verified_at: Utc::now().trunc_subsecs(6),
    };
    let mut transaction = pool.begin().await.unwrap();
    lock_reference(&mut transaction, payment).await.unwrap();
    assert!(lock(&mut transaction, first.id).await.unwrap().is_some());
    assert!(save_capture(&mut transaction, &evidence).await.unwrap());
    assert!(save_capture(&mut transaction, &evidence).await.unwrap());
    let mut wrong = evidence.clone();
    wrong.amount = "12.35".into();
    assert!(!save_capture(&mut transaction, &wrong).await.unwrap());
    assert!(
        !complete(
            &mut transaction,
            first.id,
            Some(stale),
            &evidence.capture_id
        )
        .await
        .unwrap()
    );
    assert!(
        complete(
            &mut transaction,
            first.id,
            Some(token),
            &evidence.capture_id
        )
        .await
        .unwrap()
    );
    assert_eq!(
        captures(&mut transaction, first.id).await.unwrap(),
        std::slice::from_ref(&evidence)
    );
    assert_eq!(
        duplicate_successful_reference(&mut transaction, payment)
            .await
            .unwrap(),
        [] as [Uuid; 0]
    );
    transaction.commit().await.unwrap();
    assert!(
        !finish_operation(pool, first.id, token, "DENIED", None)
            .await
            .unwrap()
    );
    assert_eq!(
        find(pool, first.id).await.unwrap().unwrap().status,
        "COMPLETED"
    );

    evidence
}

async fn assert_duplicate_reference(
    pool: &PgPool,
    input: &mut NewPaypalPayment,
    first: &PaypalPayment,
    evidence: &CaptureEvidence,
) {
    let until = Utc::now() + chrono::Duration::minutes(5);
    input.id = Uuid::new_v4();
    input.idempotency_key = Uuid::new_v4();
    input.create_request_id = Uuid::new_v4();
    input.capture_request_id = Uuid::new_v4();
    let (second, _) = create_or_get(pool, input).await.unwrap();
    let token = Uuid::new_v4();
    claim_operation(pool, second.id, PaymentOperation::Create, token, until)
        .await
        .unwrap()
        .unwrap();
    assert!(
        save_order(pool, second.id, token, "order-two", None)
            .await
            .unwrap()
    );
    let mut second_evidence = evidence.clone();
    second_evidence.payment_id = second.id;
    second_evidence.paypal_order_id = "order-two".into();
    let mut transaction = pool.begin().await.unwrap();
    lock_reference(&mut transaction, &second).await.unwrap();
    assert!(
        !save_capture(&mut transaction, &second_evidence)
            .await
            .unwrap()
    );
    second_evidence.capture_id = format!("capture-{}", Uuid::new_v4());
    assert!(
        save_capture(&mut transaction, &second_evidence)
            .await
            .unwrap()
    );
    assert!(
        complete(
            &mut transaction,
            second.id,
            None,
            &second_evidence.capture_id
        )
        .await
        .unwrap()
    );
    assert_eq!(
        duplicate_successful_reference(&mut transaction, &second)
            .await
            .unwrap(),
        [first.id]
    );
    flag_exception(&mut transaction, first.id, "Duplicate reference")
        .await
        .unwrap();
    flag_exception(&mut transaction, second.id, "Duplicate reference")
        .await
        .unwrap();
    transaction.commit().await.unwrap();
    assert!(
        find(pool, first.id)
            .await
            .unwrap()
            .unwrap()
            .exception
            .is_some()
    );
}

async fn assert_event_recovery(pool: &PgPool, first: &PaypalPayment, evidence: &CaptureEvidence) {
    let merchant = &first.merchant_id;
    let mut event = NewPaypalPaymentEvent {
        merchant_id: merchant.clone(),
        environment: "sandbox".into(),
        event_id: format!("event-{}", Uuid::new_v4()),
        payload_digest: [3; 32],
        event_type: "PAYMENT.CAPTURE.COMPLETED".into(),
        paypal_order_id: Some("order-one".into()),
        capture_id: Some(evidence.capture_id.clone()),
        verified_at: Utc::now(),
    };
    let (receipt, inserted) = record_event(pool, &event).await.unwrap();
    assert!(inserted);
    assert_eq!(receipt.state, "PENDING");
    event.payload_digest = [4; 32];
    let (replay, inserted) = record_event(pool, &event).await.unwrap();
    assert!(!inserted);
    assert_eq!(replay.payload_digest, [3; 32]);
    let mut transaction = pool.begin().await.unwrap();
    assert_eq!(
        lock_event(&mut transaction, merchant, "sandbox", &event.event_id)
            .await
            .unwrap()
            .state,
        "PENDING"
    );
    assert!(
        finish_event(
            &mut transaction,
            merchant,
            "sandbox",
            &event.event_id,
            Some(first.id),
            false
        )
        .await
        .unwrap()
    );
    transaction.rollback().await.unwrap();
    assert_eq!(record_event(pool, &event).await.unwrap().0.state, "PENDING");
    let mut transaction = pool.begin().await.unwrap();
    assert!(
        finish_event(
            &mut transaction,
            merchant,
            "sandbox",
            &event.event_id,
            Some(first.id),
            false
        )
        .await
        .unwrap()
    );
    assert!(
        !finish_event(
            &mut transaction,
            merchant,
            "sandbox",
            &event.event_id,
            Some(first.id),
            true
        )
        .await
        .unwrap()
    );
    transaction.commit().await.unwrap();
    assert_eq!(
        record_event(pool, &event).await.unwrap().0.state,
        "PROCESSED"
    );
}

async fn cleanup(pool: &PgPool, merchant: &str) {
    sqlx::query("DELETE FROM paypal_payment_events WHERE merchant_id=$1")
        .bind(merchant)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("UPDATE paypal_payments SET verified_capture_id=NULL,verified_at=NULL,status='CREATING' WHERE merchant_id=$1").bind(merchant).execute(pool).await.unwrap();
    sqlx::query("DELETE FROM paypal_payment_captures WHERE merchant_id=$1")
        .bind(merchant)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM paypal_payments WHERE merchant_id=$1")
        .bind(merchant)
        .execute(pool)
        .await
        .unwrap();
}
