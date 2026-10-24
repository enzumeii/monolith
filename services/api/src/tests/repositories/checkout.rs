#![allow(clippy::unwrap_used)]
use super::*;
use crate::models::checkout::{CompletionState, DraftState, PaymentState};
use chrono::Duration;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use std::{str::FromStr, time::Duration as StdDuration};
const FIXTURE_LOCK: i64 = 6_843_819_200_123;

struct Fixture {
    pool: PgPool,
    ids: Vec<Uuid>,
    application_name: String,
    guard: sqlx::Transaction<'static, sqlx::Postgres>,
}

impl Fixture {
    async fn new() -> Self {
        let url = std::env::var("E2E_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://e2e:e2e@127.0.0.1:5433/e2e".into());
        let application_name = format!("checkout-repository-{}", Uuid::new_v4());
        let options = PgConnectOptions::from_str(&url)
            .unwrap()
            .application_name(&application_name);
        let pool = PgPoolOptions::new()
            .max_connections(8)
            .connect_with(options)
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        let mut guard = pool.begin().await.unwrap();
        sqlx::query("SELECT pg_advisory_xact_lock($1)")
            .bind(FIXTURE_LOCK)
            .execute(&mut *guard)
            .await
            .unwrap();
        Self {
            pool,
            ids: vec![],
            application_name,
            guard,
        }
    }

    async fn checkout(&mut self) -> Checkout {
        let now = Utc::now();
        let checkout = Checkout {
            id: Uuid::new_v4(),
            guest_secret_hash: vec![1; 32],
            owner_expires_at: now + Duration::days(7),
            request_fingerprint: vec![2; 32],
            convention_id: Uuid::new_v4(),
            provider: "stripe".into(),
            provider_account_id: "acct_test".into(),
            provider_environment: "test".into(),
            evidence_hash: vec![3; 32],
            required_tag_hashes: vec![vec![4; 32]],
            success_url: None,
            cancel_url: None,
            gateway_request_fingerprint: None,
            payment_state: PaymentState::NotStarted,
            draft_state: DraftState::NotStarted,
            completion_state: CompletionState::NotStarted,
            draft_id: None,
            provider_resource_id: None,
            order_id: None,
            reservation_expires_at: None,
            created_at: now,
        };
        insert(&mut self.pool.acquire().await.unwrap(), &checkout)
            .await
            .unwrap();
        self.ids.push(checkout.id);
        find(&self.pool, checkout.id).await.unwrap().unwrap()
    }

    async fn due(&self, id: Uuid, offset: i32) {
        sqlx::query(
            "UPDATE checkouts
             SET next_recovery_at = TIMESTAMPTZ '2000-01-01 00:00:00+00' + make_interval(secs => $2)
             WHERE id = $1",
        )
        .bind(id)
        .bind(f64::from(offset))
        .execute(&self.pool)
        .await
        .unwrap();
    }

    async fn metadata(
        &self,
        id: Uuid,
    ) -> (
        i32,
        Option<DateTime<Utc>>,
        bool,
        Option<String>,
        Option<Uuid>,
    ) {
        sqlx::query_as(
            "SELECT recovery_failures, next_recovery_at, recovery_manual, recovery_code, claim_token
             FROM checkouts
             WHERE id = $1",
        )
        .bind(id)
        .fetch_one(&self.pool)
        .await
        .unwrap()
    }

    async fn cleanup(self) {
        sqlx::query(
            "DELETE FROM checkouts
             WHERE id = ANY($1)",
        )
        .bind(&self.ids)
        .execute(&self.pool)
        .await
        .unwrap();
        self.guard.rollback().await.unwrap();
        self.pool.close().await;
    }
}

#[tokio::test]
async fn due_claims_are_fair_concurrent_and_exclude_ineligible_rows() {
    let mut fixture = Fixture::new().await;
    let fresh = fixture.checkout().await;
    let metadata = fixture.metadata(fresh.id).await;
    assert_eq!(
        metadata.1.unwrap(),
        fresh.created_at + Duration::seconds(60)
    );
    assert!(
        claim_due(&fixture.pool, Uuid::new_v4(), 90, 12)
            .await
            .unwrap()
            .is_none()
    );
    let oldest = fixture.checkout().await;
    let newer = fixture.checkout().await;
    fixture.due(oldest.id, 0).await;
    fixture.due(newer.id, 1).await;
    let live = fixture.checkout().await;
    fixture.due(live.id, -3).await;
    claim(&fixture.pool, live.id, Uuid::new_v4())
        .await
        .unwrap()
        .unwrap();
    let manual = fixture.checkout().await;
    fixture.due(manual.id, -2).await;
    sqlx::query(
        "UPDATE checkouts
         SET recovery_manual = TRUE
         WHERE id = $1",
    )
    .bind(manual.id)
    .execute(&fixture.pool)
    .await
    .unwrap();
    let terminal = fixture.checkout().await;
    fixture.due(terminal.id, -1).await;
    sqlx::query(
        "UPDATE checkouts
         SET payment_state = 'unpaid'
         WHERE id = $1",
    )
    .bind(terminal.id)
    .execute(&fixture.pool)
    .await
    .unwrap();
    let first_token = Uuid::new_v4();
    let first = claim_due(&fixture.pool, first_token, 90, 12)
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(first, DueClaim::Ready { checkout, failures: 0 } if checkout.id == oldest.id));
    let (one, two) = tokio::join!(
        claim_due(&fixture.pool, Uuid::new_v4(), 90, 12),
        claim_due(&fixture.pool, Uuid::new_v4(), 90, 12),
    );
    let claims: Vec<_> = [one.unwrap(), two.unwrap()].into_iter().flatten().collect();
    assert_eq!(claims.len(), 1);
    assert!(
        matches!(&claims[0], DueClaim::Ready { checkout, failures: 0 } if checkout.id == newer.id)
    );
    assert!(
        claim_due(&fixture.pool, Uuid::new_v4(), 90, 12)
            .await
            .unwrap()
            .is_none()
    );
    fixture.cleanup().await;
}

#[tokio::test]
async fn locked_oldest_row_does_not_block_another_due_claim() {
    let mut fixture = Fixture::new().await;
    let oldest = fixture.checkout().await;
    let next = fixture.checkout().await;
    fixture.due(oldest.id, 0).await;
    fixture.due(next.id, 1).await;
    let mut transaction = fixture.pool.begin().await.unwrap();
    sqlx::query(
        "SELECT id
         FROM checkouts
         WHERE id = $1
         FOR UPDATE",
    )
    .bind(oldest.id)
    .execute(&mut *transaction)
    .await
    .unwrap();
    let selected = tokio::time::timeout(
        StdDuration::from_secs(2),
        claim_due(&fixture.pool, Uuid::new_v4(), 90, 12),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(matches!(selected, Some(DueClaim::Ready { checkout, .. }) if checkout.id == next.id));
    transaction.commit().await.unwrap();
    assert!(
        matches!(claim_due(&fixture.pool, Uuid::new_v4(), 90, 12).await.unwrap(),
        Some(DueClaim::Ready { checkout, .. }) if checkout.id == oldest.id)
    );
    fixture.cleanup().await;
}

#[tokio::test]
async fn abandoned_claims_count_once_and_exhaustion_is_durable_even_after_cap_change() {
    let mut fixture = Fixture::new().await;
    let row = fixture.checkout().await;
    fixture.due(row.id, 0).await;
    let abandoned = Uuid::new_v4();
    claim(&fixture.pool, row.id, abandoned)
        .await
        .unwrap()
        .unwrap();
    sqlx::query(
        "UPDATE checkouts
         SET recovery_failures = 2,
             claim_expires_at = clock_timestamp() - INTERVAL '1 second'
         WHERE id = $1",
    )
    .bind(row.id)
    .execute(&fixture.pool)
    .await
    .unwrap();
    let token = Uuid::new_v4();
    assert!(
        matches!(claim_due(&fixture.pool, token, 90, 12).await.unwrap(),
        Some(DueClaim::Abandoned { id, failures: 3 }) if id == row.id)
    );
    assert!(
        claim_due(&fixture.pool, Uuid::new_v4(), 90, 12)
            .await
            .unwrap()
            .is_none()
    );
    assert!(release(&fixture.pool, row.id, abandoned).await.is_err());
    let later = Utc::now() + Duration::seconds(120);
    finish_recovery(
        &fixture.pool,
        row.id,
        token,
        3,
        Some(later),
        false,
        RecoveryCode::ClaimAbandoned,
    )
    .await
    .unwrap();
    let metadata = fixture.metadata(row.id).await;
    assert_eq!(metadata.0, 3);
    assert_eq!(metadata.3.as_deref(), Some("claim_abandoned"));
    assert!(
        claim_due(&fixture.pool, Uuid::new_v4(), 90, 12)
            .await
            .unwrap()
            .is_none()
    );
    fixture.due(row.id, 0).await;
    let last_token = Uuid::new_v4();
    assert!(matches!(
        claim_due(&fixture.pool, last_token, 90, 4).await.unwrap(),
        Some(DueClaim::Ready { failures: 3, .. })
    ));
    sqlx::query(
        "UPDATE checkouts
         SET claim_expires_at = clock_timestamp() - INTERVAL '1 second'
         WHERE id = $1",
    )
    .bind(row.id)
    .execute(&fixture.pool)
    .await
    .unwrap();
    assert!(
        matches!(claim_due(&fixture.pool, Uuid::new_v4(), 90, 4).await.unwrap(), Some(DueClaim::Stopped { id }) if id == row.id)
    );
    let metadata = fixture.metadata(row.id).await;
    assert_eq!(
        metadata,
        (4, None, true, Some("attempts_exhausted".into()), None)
    );
    assert!(
        claim_due(&fixture.pool, Uuid::new_v4(), 90, 4)
            .await
            .unwrap()
            .is_none()
    );
    let lower = fixture.checkout().await;
    fixture.due(lower.id, 0).await;
    sqlx::query(
        "UPDATE checkouts
         SET recovery_failures = 9
         WHERE id = $1",
    )
    .bind(lower.id)
    .execute(&fixture.pool)
    .await
    .unwrap();
    assert!(
        matches!(claim_due(&fixture.pool, Uuid::new_v4(), 90, 4).await.unwrap(), Some(DueClaim::Stopped { id }) if id == lower.id)
    );
    assert_eq!(fixture.metadata(lower.id).await.0, 9);
    fixture.cleanup().await;
}

#[tokio::test]
async fn all_owned_writes_reject_wrong_expired_and_successor_tokens() {
    let mut fixture = Fixture::new().await;
    let row = fixture.checkout().await;
    let token = Uuid::new_v4();
    claim(&fixture.pool, row.id, token).await.unwrap().unwrap();
    for invalid in [Uuid::new_v4(), token] {
        if invalid == token {
            sqlx::query(
                "UPDATE checkouts
                 SET claim_expires_at = clock_timestamp() - INTERVAL '1 second'
                 WHERE id = $1",
            )
            .bind(row.id)
            .execute(&fixture.pool)
            .await
            .unwrap();
        }
        assert!(save(&fixture.pool, &row, invalid).await.is_err());
        assert!(renew(&fixture.pool, row.id, invalid, 90).await.is_err());
        assert!(
            finish_recovery(
                &fixture.pool,
                row.id,
                invalid,
                1,
                None,
                true,
                RecoveryCode::TransientFailure
            )
            .await
            .is_err()
        );
        assert!(release(&fixture.pool, row.id, invalid).await.is_err());
    }
    let successor = Uuid::new_v4();
    claim(&fixture.pool, row.id, successor)
        .await
        .unwrap()
        .unwrap();
    assert!(save(&fixture.pool, &row, token).await.is_err());
    assert!(release(&fixture.pool, row.id, token).await.is_err());
    renew(&fixture.pool, row.id, successor, 90).await.unwrap();
    assert_eq!(fixture.metadata(row.id).await.4, Some(successor));
    release(&fixture.pool, row.id, successor).await.unwrap();
    fixture.cleanup().await;
}

#[tokio::test]
async fn lease_is_checked_after_waiting_for_row_lock_for_every_owned_write() {
    let mut fixture = Fixture::new().await;
    for operation in 0..5 {
        let row = fixture.checkout().await;
        let token = Uuid::new_v4();
        claim(&fixture.pool, row.id, token).await.unwrap().unwrap();
        renew(&fixture.pool, row.id, token, 1).await.unwrap();
        let mut transaction = fixture.pool.begin().await.unwrap();
        sqlx::query(
            "SELECT id
             FROM checkouts
             WHERE id = $1
             FOR UPDATE",
        )
        .bind(row.id)
        .execute(&mut *transaction)
        .await
        .unwrap();
        let pool = fixture.pool.clone();
        let task = tokio::spawn(async move {
            match operation {
                0 => save(&pool, &row, token).await,
                1 => renew(&pool, row.id, token, 90).await,
                2 => {
                    finish_recovery(&pool, row.id, token, 0, None, false, RecoveryCode::Observed)
                        .await
                }
                3 => release(&pool, row.id, token).await,
                _ => claim(&pool, row.id, Uuid::new_v4()).await.map(|claimed| {
                    assert!(
                        claimed.is_some(),
                        "public claim tested expiry before lock waiting"
                    );
                }),
            }
        });
        tokio::time::timeout(StdDuration::from_secs(5), async {
            loop {
                let waiting: bool = sqlx::query_scalar(
                    "SELECT EXISTS (
                         SELECT 1
                         FROM pg_stat_activity
                         WHERE application_name = $1
                           AND wait_event_type = 'Lock'
                     )",
                )
                .bind(&fixture.application_name)
                .fetch_one(&fixture.pool)
                .await
                .unwrap();
                if waiting {
                    break;
                }
                tokio::time::sleep(StdDuration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        tokio::time::sleep(StdDuration::from_millis(1100)).await;
        transaction.commit().await.unwrap();
        let result = task.await.unwrap();
        if operation == 4 {
            result.unwrap();
        } else {
            assert!(
                result.is_err(),
                "operation {operation} authorized an expired owner after lock waiting"
            );
        }
    }
    fixture.cleanup().await;
}

#[tokio::test]
async fn successful_observation_resets_failures_and_terminal_authority_clears_manual_state() {
    let mut fixture = Fixture::new().await;
    let mut row = fixture.checkout().await;
    fixture.due(row.id, 0).await;
    let token = Uuid::new_v4();
    claim(&fixture.pool, row.id, token).await.unwrap().unwrap();
    finish_recovery(
        &fixture.pool,
        row.id,
        token,
        5,
        Some(Utc::now() - Duration::seconds(1)),
        false,
        RecoveryCode::TransientFailure,
    )
    .await
    .unwrap();
    let observed = Uuid::new_v4();
    assert!(matches!(
        claim_due(&fixture.pool, observed, 90, 12).await.unwrap(),
        Some(DueClaim::Ready { failures: 5, .. })
    ));
    finish_recovery(
        &fixture.pool,
        row.id,
        observed,
        0,
        Some(Utc::now() + Duration::seconds(60)),
        false,
        RecoveryCode::WaitingForPayment,
    )
    .await
    .unwrap();
    assert_eq!(fixture.metadata(row.id).await.0, 0);
    let manual = Uuid::new_v4();
    claim(&fixture.pool, row.id, manual).await.unwrap().unwrap();
    finish_recovery(
        &fixture.pool,
        row.id,
        manual,
        12,
        None,
        true,
        RecoveryCode::EvidenceConflict,
    )
    .await
    .unwrap();
    let webhook = Uuid::new_v4();
    claim(&fixture.pool, row.id, webhook)
        .await
        .unwrap()
        .unwrap();
    row.payment_state = PaymentState::Paid;
    row.completion_state = CompletionState::Completed;
    row.order_id = Some(format!("gid://shopify/Order/{}", Uuid::new_v4()));
    save(&fixture.pool, &row, webhook).await.unwrap();
    assert_eq!(
        fixture.metadata(row.id).await,
        (0, None, false, None, Some(webhook))
    );
    finish_recovery(
        &fixture.pool,
        row.id,
        webhook,
        7,
        Some(Utc::now()),
        true,
        RecoveryCode::TransientFailure,
    )
    .await
    .unwrap();
    assert_eq!(fixture.metadata(row.id).await, (0, None, false, None, None));
    let mut unpaid = fixture.checkout().await;
    let token = Uuid::new_v4();
    claim(&fixture.pool, unpaid.id, token)
        .await
        .unwrap()
        .unwrap();
    unpaid.payment_state = PaymentState::Unpaid;
    save(&fixture.pool, &unpaid, token).await.unwrap();
    assert_eq!(
        fixture.metadata(unpaid.id).await,
        (0, None, false, None, Some(token))
    );
    release(&fixture.pool, unpaid.id, token).await.unwrap();
    fixture.cleanup().await;
}

#[tokio::test]
async fn late_authoritative_payment_rearms_previously_unpaid_checkout() {
    let mut fixture = Fixture::new().await;
    for late_state in [PaymentState::Processing, PaymentState::Paid] {
        let mut row = fixture.checkout().await;
        let token = Uuid::new_v4();
        claim(&fixture.pool, row.id, token).await.unwrap().unwrap();
        row.payment_state = PaymentState::Unpaid;
        save(&fixture.pool, &row, token).await.unwrap();
        assert!(fixture.metadata(row.id).await.1.is_none());
        row.payment_state = late_state;
        let before: DateTime<Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
            .fetch_one(&fixture.pool)
            .await
            .unwrap();
        save(&fixture.pool, &row, token).await.unwrap();
        let metadata = fixture.metadata(row.id).await;
        let after: DateTime<Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
            .fetch_one(&fixture.pool)
            .await
            .unwrap();
        let next_due = metadata.1.unwrap();
        assert!(next_due >= before + Duration::seconds(60));
        assert!(next_due <= after + Duration::seconds(60));
        assert_eq!((metadata.0, metadata.2, metadata.3), (0, false, None));
        release(&fixture.pool, row.id, token).await.unwrap();
        fixture.due(row.id, 0).await;
        let recovery = Uuid::new_v4();
        assert!(
            matches!(claim_due(&fixture.pool, recovery, 90, 12).await.unwrap(),
            Some(DueClaim::Ready { checkout, failures: 0 }) if checkout.id == row.id)
        );
        row.payment_state = PaymentState::Paid;
        row.completion_state = CompletionState::Completed;
        row.order_id = Some(format!("gid://shopify/Order/{}", Uuid::new_v4()));
        save(&fixture.pool, &row, recovery).await.unwrap();
        assert_eq!(
            fixture.metadata(row.id).await,
            (0, None, false, None, Some(recovery))
        );
        release(&fixture.pool, row.id, recovery).await.unwrap();
    }
    fixture.cleanup().await;
}
