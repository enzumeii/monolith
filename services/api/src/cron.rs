use crate::{
    error::AppError,
    repositories,
    services::cron::{config, job},
    state::AppState,
};
use chrono::Utc;
use tokio::task::JoinSet;

pub async fn run(state: AppState) {
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(
        state.settings.scheduler.poll_interval_secs,
    ));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        interval.tick().await;
        if let Err(error) = dispatch(&state).await {
            crate::error::log_background_error("cron dispatch", &error);
        }
        if let Err(error) = repositories::cron_jobs::recover(
            &state.pool,
            Utc::now().naive_utc(),
            job::max_attempts(&state),
        )
        .await
        {
            crate::error::log_background_error("cron recovery", &error);
        }
        let mut tasks = JoinSet::new();
        for _ in 0..state.settings.scheduler.concurrency {
            let state = state.clone();
            tasks.spawn(async move { execute_one(&state).await });
        }
        while let Some(outcome) = tasks.join_next().await {
            match outcome {
                Ok(Ok(())) => {}
                Ok(Err(error)) => crate::error::log_background_error("cron job", &error),
                Err(error) => tracing::error!(error=%error,"cron task failed"),
            }
        }
    }
}
pub(crate) async fn dispatch(state: &AppState) -> Result<(), AppError> {
    let now = Utc::now().naive_utc();
    let mut after = None;
    loop {
        let ids = repositories::cron_configurations::enabled_page(&state.pool, after).await?;
        if ids.is_empty() {
            break;
        }
        for id in ids {
            after = Some(id);
            let result = async {
                let mut transaction = state.pool.begin().await?;
                let Some(configuration) =
                    repositories::cron_configurations::try_lock(&mut transaction, id).await?
                else {
                    return Ok::<_, AppError>(());
                };
                if !configuration.enabled {
                    return Ok(());
                }
                let kind = crate::models::cron::report_type(&configuration.report_type)?;
                crate::validations::cron::inputs(
                    kind,
                    &configuration.cron_expression,
                    &configuration.parameter,
                )?;
                let cursor = repositories::cron_jobs::latest(&mut transaction, id)
                    .await?
                    .map_or(configuration.updated_at, |due| {
                        due.max(configuration.updated_at)
                    });
                let mut cursor = cursor.and_utc();
                let mut latest = None;
                for _ in 0..1000 {
                    let due =
                        config::schedule::next_occurrence(&configuration.cron_expression, cursor)
                            .map_err(config::schedule_error)?;
                    if due.naive_utc() > now {
                        break;
                    }
                    if kind == domain::dto::ScheduleReportType::Transaction {
                        repositories::cron_jobs::enqueue(&mut transaction, id, due.naive_utc())
                            .await?;
                    } else {
                        latest = Some(due.naive_utc());
                    }
                    cursor = due;
                }
                if let Some(due) = latest {
                    repositories::cron_jobs::enqueue(&mut transaction, id, due).await?;
                }
                transaction.commit().await?;
                Ok(())
            }
            .await;
            if let Err(error) = result {
                crate::error::log_background_error("cron configuration dispatch", &error);
            }
        }
    }
    Ok(())
}
pub(crate) fn retry_at(state: &AppState, attempts: i32) -> Option<chrono::NaiveDateTime> {
    if attempts >= job::max_attempts(state) {
        return None;
    }
    let delay = state
        .settings
        .scheduler
        .retry_backoff_secs
        .saturating_mul(u64::try_from(attempts).unwrap_or(1));
    Some(
        Utc::now().naive_utc()
            + chrono::Duration::seconds(i64::try_from(delay).unwrap_or(i64::MAX)),
    )
}
async fn execute_one(state: &AppState) -> Result<(), AppError> {
    let Some(claim) = job::claim(state).await? else {
        return Ok(());
    };
    if let Err(failure) = with_lease(state, &claim, job::execute(state, &claim)).await {
        crate::error::log_background_error("cron workflow", &failure.error);
        job::fail(state, &claim, failure.message).await?;
    }
    Ok(())
}
async fn with_lease<T>(
    state: &AppState,
    claim: &job::ClaimedJob,
    work: impl std::future::Future<Output = Result<T, job::Failure>>,
) -> Result<T, job::Failure> {
    let mut heartbeat = tokio::time::interval(std::time::Duration::from_millis(
        (state.settings.scheduler.lease_secs.saturating_mul(1000) / 3).max(1),
    ));
    let renewal = async {
        loop {
            heartbeat.tick().await;
            if !repositories::cron_jobs::renew(
                &state.pool,
                claim.job.id,
                claim.token,
                job::lease_until(state),
            )
            .await
            .map_err(|error| job::Failure {
                error,
                message: "Lease renewal failed; email delivery may be unconfirmed",
            })? {
                return Err(job::Failure {
                    error: job::lease_lost(),
                    message: "Job lease lost; email delivery may be unconfirmed",
                });
            }
        }
    };
    tokio::select! { result=work=>result, result=renewal=>result }
}

#[cfg(test)]
#[path = "tests/cron.rs"]
mod tests;
