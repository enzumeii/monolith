#![allow(clippy::unwrap_used)]
use super::*;
use crate::services::checkout_completion::tests::{cleanup, fixture, pool, seed};
#[tokio::test]
async fn ownership_fences_stale_writers_and_expires_for_reconciliation() {
    let pool = pool().await;
    let (id, snapshot, attempt, _) = fixture();
    seed(&pool, id, &snapshot, &attempt).await;
    let token = Uuid::new_v4();
    let mut connection = pool.acquire().await.unwrap();
    assert!(
        claim(
            &mut connection,
            id,
            token,
            Utc::now() + chrono::Duration::seconds(60)
        )
        .await
        .unwrap()
        .is_some()
    );
    assert!(
        claim(
            &mut connection,
            id,
            Uuid::new_v4(),
            Utc::now() + chrono::Duration::seconds(60)
        )
        .await
        .unwrap()
        .is_none()
    );
    drop(connection);
    assert!(
        progress(
            &pool,
            id,
            Uuid::new_v4(),
            CompletionState::Complete,
            None,
            false,
            None,
            true
        )
        .await
        .is_err()
    );
    sqlx::query("UPDATE checkout_completions SET lease_until=CURRENT_TIMESTAMP-INTERVAL '1 second' WHERE id=$1").bind(id).execute(&pool).await.unwrap();
    let next = Uuid::new_v4();
    assert!(
        claim(
            &mut pool.acquire().await.unwrap(),
            id,
            next,
            Utc::now() + chrono::Duration::seconds(60)
        )
        .await
        .unwrap()
        .is_some()
    );
    assert!(
        progress(
            &pool,
            id,
            token,
            CompletionState::Uncertain,
            None,
            false,
            None,
            true
        )
        .await
        .is_err()
    );
    progress(
        &pool,
        id,
        next,
        CompletionState::Uncertain,
        None,
        true,
        Some("Completion uncertain"),
        true,
    )
    .await
    .unwrap();
    assert!(find(&pool, id).await.unwrap().unwrap().completion_intent);
    cleanup(&pool, id).await;
}
#[tokio::test]
async fn duplicate_alias_is_account_scoped_and_rejects_another_checkout() {
    let pool = pool().await;
    let (first, snapshot, attempt, payment) = fixture();
    seed(&pool, first, &snapshot, &attempt).await;
    let (second, snapshot2, _, _) = fixture();
    seed(&pool, second, &snapshot2, &attempt).await;
    let PaymentFact::Paid { aliases, .. } = payment.state else {
        panic!("paid fixture")
    };
    bind_aliases(
        &mut pool.acquire().await.unwrap(),
        first,
        &attempt,
        &aliases,
    )
    .await
    .unwrap();
    assert!(
        bind_aliases(
            &mut pool.acquire().await.unwrap(),
            second,
            &attempt,
            &aliases
        )
        .await
        .is_err()
    );
    let mut other = attempt.clone();
    other.account = "another-account".into();
    bind_aliases(&mut pool.acquire().await.unwrap(), second, &other, &aliases)
        .await
        .unwrap();
    let mappings = lookup_aliases(&pool, Provider::Stripe, &[aliases[0].value.clone()])
        .await
        .unwrap();
    assert_eq!(mappings.len(), 2);
    assert!(
        mappings
            .iter()
            .all(|mapping| mapping.completion_state == CompletionState::Ready)
    );
    cleanup(&pool, first).await;
    cleanup(&pool, second).await;
}
