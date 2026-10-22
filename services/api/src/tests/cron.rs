#![allow(clippy::unwrap_used)]
use super::*;
use crate::repositories::cron_configurations::tests::{input, state};

#[tokio::test]
async fn dispatch_visits_every_page_and_coalesces_inventory_without_duplicates() {
    let state = state().await;
    for _ in 0..51 {
        repositories::cron_configurations::insert(&state.pool, &input())
            .await
            .unwrap();
    }
    sqlx::query("UPDATE cron_configurations SET updated_at=(CURRENT_TIMESTAMP AT TIME ZONE 'UTC')-INTERVAL '61 seconds'").execute(&state.pool).await.unwrap();
    dispatch(&state).await.unwrap();
    dispatch(&state).await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM cron_jobs")
            .fetch_one(&state.pool)
            .await
            .unwrap(),
        51
    );
    for (table, expected) in [
        ("cron_configurations", 9_i64),
        ("cron_jobs", 10),
        ("report_results", 8),
    ] {
        let actual:i64=sqlx::query_scalar("SELECT COUNT(*) FROM pg_attribute WHERE attrelid=$1::regclass AND attnum>0 AND NOT attisdropped")
            .bind(table).fetch_one(&state.pool).await.unwrap();
        assert_eq!(actual, expected);
    }
    state.pool.close().await;
}
#[tokio::test]
async fn malformed_configuration_does_not_starve_other_jobs() {
    let state = state().await;
    let bad = repositories::cron_configurations::insert(&state.pool, &input())
        .await
        .unwrap();
    let good = repositories::cron_configurations::insert(&state.pool, &input())
        .await
        .unwrap();
    repositories::cron_jobs::enqueue(
        &mut state.pool.acquire().await.unwrap(),
        bad.id,
        Utc::now().naive_utc() - chrono::Duration::seconds(1),
    )
    .await
    .unwrap();
    repositories::cron_jobs::enqueue(
        &mut state.pool.acquire().await.unwrap(),
        good.id,
        Utc::now().naive_utc(),
    )
    .await
    .unwrap();
    sqlx::query("UPDATE cron_configurations SET report_type='UNKNOWN' WHERE id=$1")
        .bind(bad.id)
        .execute(&state.pool)
        .await
        .unwrap();
    assert!(job::claim(&state).await.is_err());
    let next = job::claim(&state).await.unwrap().unwrap();
    assert_eq!(next.job.attempts, 1);
    let audit = repositories::report_results::find(&state.pool, next.job.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(audit.parameters, "123");
    state.pool.close().await;
}
#[tokio::test]
async fn missing_provider_failure_is_eagerly_audited() {
    let state = state().await;
    let config = repositories::cron_configurations::insert(&state.pool, &input())
        .await
        .unwrap();
    repositories::cron_jobs::enqueue(
        &mut state.pool.acquire().await.unwrap(),
        config.id,
        Utc::now().naive_utc(),
    )
    .await
    .unwrap();
    let claim = job::claim(&state).await.unwrap().unwrap();
    let failure = job::execute(&state, &claim).await.err().unwrap();
    job::fail(&state, &claim, failure.message).await.unwrap();
    let failed: (String, String) = sqlx::query_as("SELECT status,error FROM report_results")
        .fetch_one(&state.pool)
        .await
        .unwrap();
    assert_eq!(failed, ("FAILED".into(), "Report generation failed".into()));
    state.pool.close().await;
}
#[tokio::test]
async fn heartbeat_does_not_stop_polling_work_that_holds_the_only_database_connection() {
    let state = state().await;
    let config = repositories::cron_configurations::insert(&state.pool, &input())
        .await
        .unwrap();
    repositories::cron_jobs::enqueue(
        &mut state.pool.acquire().await.unwrap(),
        config.id,
        Utc::now().naive_utc(),
    )
    .await
    .unwrap();
    let claim = job::claim(&state).await.unwrap().unwrap();
    let work = async {
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let transaction = state.pool.begin().await.map_err(|error| job::Failure {
            error: error.into(),
            message: "test connection",
        })?;
        tokio::time::sleep(std::time::Duration::from_millis(1800)).await;
        transaction.commit().await.map_err(|error| job::Failure {
            error: error.into(),
            message: "test commit",
        })?;
        Ok(())
    };
    assert!(
        tokio::time::timeout(
            std::time::Duration::from_secs(3),
            with_lease(&state, &claim, work)
        )
        .await
        .unwrap()
        .is_ok()
    );
    state.pool.close().await;
}

#[tokio::test]
async fn transaction_dispatch_catches_up_and_edit_starts_a_future_revision() {
    let state = state().await;
    let mut request = input();
    request.report_type = domain::dto::ScheduleReportType::Transaction;
    request.parameter = "PREVIOUS_MONTH".into();
    request.cron_expression = "0 0 1 * *".into();
    let config = repositories::cron_configurations::insert(&state.pool, &request)
        .await
        .unwrap();
    sqlx::query("UPDATE cron_configurations SET updated_at=(CURRENT_TIMESTAMP AT TIME ZONE 'UTC')-INTERVAL '4 months'")
        .execute(&state.pool)
        .await
        .unwrap();
    dispatch(&state).await.unwrap();
    let before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM cron_jobs")
        .fetch_one(&state.pool)
        .await
        .unwrap();
    assert!(before >= 3);
    let observed = repositories::cron_configurations::find(&state.pool, config.id)
        .await
        .unwrap()
        .unwrap();
    repositories::cron_configurations::update(
        &state.pool,
        config.id,
        observed.updated_at,
        &request,
    )
    .await
    .unwrap()
    .unwrap();
    sqlx::query("DELETE FROM cron_jobs")
        .execute(&state.pool)
        .await
        .unwrap();
    dispatch(&state).await.unwrap();
    let after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM cron_jobs")
        .fetch_one(&state.pool)
        .await
        .unwrap();
    assert_eq!(after, 0);
    state.pool.close().await;
}

#[tokio::test]
async fn one_second_lease_renews_before_expiry_during_slow_work() {
    let mut state = state().await;
    let mut settings = (*state.settings).clone();
    settings.scheduler.lease_secs = 1;
    state.settings = std::sync::Arc::new(settings);
    let config = repositories::cron_configurations::insert(&state.pool, &input())
        .await
        .unwrap();
    repositories::cron_jobs::enqueue(
        &mut state.pool.acquire().await.unwrap(),
        config.id,
        Utc::now().naive_utc(),
    )
    .await
    .unwrap();
    let claim = job::claim(&state).await.unwrap().unwrap();
    let work = async {
        tokio::time::sleep(std::time::Duration::from_millis(2200)).await;
        Ok(())
    };
    assert!(with_lease(&state, &claim, work).await.is_ok());
    state.pool.close().await;
}
