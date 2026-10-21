#![allow(clippy::unwrap_used)]
use super::*;
use crate::{repositories, services::cron::job};
use repositories::cron_configurations::tests::{input, state};

#[tokio::test]
async fn whole_workflow_budget_fences_stale_tokens_and_keeps_eager_failure() {
    let (state, _) = state().await;
    let config = repositories::cron_configurations::insert(&state.pool, &input())
        .await
        .unwrap();
    let due = chrono::Utc::now().naive_utc();
    enqueue(&mut state.pool.acquire().await.unwrap(), config.id, due)
        .await
        .unwrap();
    enqueue(&mut state.pool.acquire().await.unwrap(), config.id, due)
        .await
        .unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM cron_jobs")
            .fetch_one(&state.pool)
            .await
            .unwrap(),
        1
    );
    let first = job::claim(&state).await.unwrap().unwrap();
    assert!(job::claim(&state).await.unwrap().is_none());
    assert!(
        !renew(
            &state.pool,
            first.job.id,
            Uuid::new_v4(),
            job::lease_until(&state)
        )
        .await
        .unwrap()
    );
    job::fail(&state, &first, "Email failed; delivery may be unconfirmed")
        .await
        .unwrap();
    let audit = repositories::report_results::find(&state.pool, first.job.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(audit.status, "FAILED");
    assert!(audit.error.unwrap().contains("Email failed"));
    assert!(job::claim(&state).await.unwrap().is_none());
    sqlx::query("UPDATE cron_jobs SET lease_until=(CURRENT_TIMESTAMP AT TIME ZONE 'UTC')-INTERVAL '1 second'").execute(&state.pool).await.unwrap();
    let second = job::claim(&state).await.unwrap().unwrap();
    assert_eq!(second.job.attempts, 2);
    assert!(
        !renew(
            &state.pool,
            first.job.id,
            first.token,
            job::lease_until(&state)
        )
        .await
        .unwrap()
    );
    job::fail(&state, &second, "Publication failed")
        .await
        .unwrap();
    assert!(job::claim(&state).await.unwrap().is_none());
    let latest = repositories::report_results::find(&state.pool, first.job.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(latest.error.as_deref(), Some("Publication failed"));
    state.pool.close().await;
}
#[tokio::test]
async fn final_expired_attempt_is_audited_and_claim_transaction_rolls_back_on_audit_error() {
    let (state, _) = state().await;
    let config = repositories::cron_configurations::insert(&state.pool, &input())
        .await
        .unwrap();
    enqueue(
        &mut state.pool.acquire().await.unwrap(),
        config.id,
        chrono::Utc::now().naive_utc(),
    )
    .await
    .unwrap();
    sqlx::raw_sql("CREATE FUNCTION pg_temp.reject_audit() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'owned audit failure'; END $$;
        CREATE TRIGGER reject_audit BEFORE INSERT ON report_results FOR EACH ROW EXECUTE FUNCTION pg_temp.reject_audit();").execute(&state.pool).await.unwrap();
    assert!(job::claim(&state).await.is_err());
    assert_eq!(
        sqlx::query_scalar::<_, i32>("SELECT attempts FROM cron_jobs")
            .fetch_one(&state.pool)
            .await
            .unwrap(),
        0
    );
    sqlx::query("DROP TRIGGER reject_audit ON report_results")
        .execute(&state.pool)
        .await
        .unwrap();
    let claim = job::claim(&state).await.unwrap().unwrap();
    sqlx::query("UPDATE cron_jobs SET attempts=2,lease_until=(CURRENT_TIMESTAMP AT TIME ZONE 'UTC')-INTERVAL '1 second'").execute(&state.pool).await.unwrap();
    recover(&state.pool, chrono::Utc::now().naive_utc(), 2)
        .await
        .unwrap();
    let audit = repositories::report_results::find(&state.pool, claim.job.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(audit.status, "FAILED");
    assert!(audit.error.unwrap().contains("final attempt"));
    assert!(job::claim(&state).await.unwrap().is_none());
    state.pool.close().await;
}
#[tokio::test]
async fn existing_audit_inputs_survive_configuration_changes() {
    let (state, _) = state().await;
    let config = repositories::cron_configurations::insert(&state.pool, &input())
        .await
        .unwrap();
    enqueue(
        &mut state.pool.acquire().await.unwrap(),
        config.id,
        chrono::Utc::now().naive_utc(),
    )
    .await
    .unwrap();
    let claim = job::claim(&state).await.unwrap().unwrap();
    job::fail(&state, &claim, "Generation failed")
        .await
        .unwrap();
    sqlx::raw_sql("UPDATE cron_configurations SET report_type='TRANSACTION',parameter='PREVIOUS_MONTH',cron_expression='0 0 1 * *'; UPDATE cron_jobs SET lease_until=(CURRENT_TIMESTAMP AT TIME ZONE 'UTC')-INTERVAL '1 second'").execute(&state.pool).await.unwrap();
    let retry = job::claim(&state).await.unwrap().unwrap();
    let audit = repositories::report_results::find(&state.pool, retry.job.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(audit.report_type, "INVENTORY");
    assert_eq!(audit.parameters, "123");
    state.pool.close().await;
}

#[tokio::test]
async fn expired_nonfinal_lease_is_failed_eagerly_and_reclaimable() {
    let (state, _) = state().await;
    let config = repositories::cron_configurations::insert(&state.pool, &input())
        .await
        .unwrap();
    enqueue(
        &mut state.pool.acquire().await.unwrap(),
        config.id,
        chrono::Utc::now().naive_utc(),
    )
    .await
    .unwrap();
    let first = job::claim(&state).await.unwrap().unwrap();
    sqlx::query("UPDATE cron_jobs SET lease_until=(CURRENT_TIMESTAMP AT TIME ZONE 'UTC')-INTERVAL '1 second'").execute(&state.pool).await.unwrap();
    recover(&state.pool, chrono::Utc::now().naive_utc(), 2)
        .await
        .unwrap();
    let audit = repositories::report_results::find(&state.pool, first.job.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(audit.status, "FAILED");
    assert!(audit.error.unwrap().contains("unconfirmed"));
    let retry = job::claim(&state).await.unwrap().unwrap();
    assert_eq!(retry.job.id, first.job.id);
    assert_eq!(retry.job.attempts, 2);
    state.pool.close().await;
}
