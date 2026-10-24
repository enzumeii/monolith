pub(crate) mod checkout;
pub(crate) mod job;
pub(crate) mod publication;
pub(crate) mod report;
pub mod runtime;
pub(crate) mod schedule;
use crate::{error::AppError, repositories, services::cron::config, state::AppState};
use chrono::Utc;
use tokio::task::JoinSet;

pub(crate) async fn invoke(state: &AppState) -> Result<(), AppError> {
    let settings = &state.settings.scheduler;
    let deadline = tokio::time::Instant::now()
        + std::time::Duration::from_secs(settings.invocation_budget_secs);
    let work_deadline = deadline - std::time::Duration::from_secs(settings.retention_reserve_secs);
    let recovery = checkout::run(state, work_deadline).await;
    let reports = if state.settings.scheduler.enabled && tokio::time::Instant::now() < work_deadline
    {
        drain(state, work_deadline).await
    } else {
        Ok(())
    };
    tokio::time::timeout_at(
        deadline,
        repositories::auth::cleanup_refresh_tokens(&state.pool),
    )
    .await
    .map_err(|_| {
        AppError::new(
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            "Refresh token retention exceeded invocation capacity",
        )
    })??;
    recovery.and(reports)
}

pub(crate) async fn run_once(state: &AppState) -> Result<bool, AppError> {
    if !state.settings.scheduler.enabled {
        return Ok(false);
    }
    let preparation = prepare(state).await.err();
    let (processed, failure) = batch(state, None).await;
    if let Some(error) = preparation.or(failure) {
        return Err(error);
    }
    Ok(processed)
}

async fn prepare(state: &AppState) -> Result<(), AppError> {
    let dispatched = dispatch(state).await;
    let recovered = repositories::cron_jobs::recover(
        &state.pool,
        Utc::now().naive_utc(),
        job::max_attempts(state),
    )
    .await;
    dispatched.and(recovered)
}

async fn drain(state: &AppState, deadline: tokio::time::Instant) -> Result<(), AppError> {
    let mut failure = tokio::time::timeout_at(deadline, prepare(state))
        .await
        .map_err(|_| capacity_error())?
        .err();
    loop {
        if tokio::time::Instant::now() >= deadline {
            return Err(capacity_error());
        }
        let (processed, error) = batch(state, Some(deadline)).await;
        if tokio::time::Instant::now() >= deadline {
            return Err(capacity_error());
        }
        if failure.is_none() {
            failure = error;
        }
        if !processed {
            return failure.map_or(Ok(()), Err);
        }
    }
}

async fn batch(
    state: &AppState,
    deadline: Option<tokio::time::Instant>,
) -> (bool, Option<AppError>) {
    // Dropping the owned JoinSet cancels every worker if the caller disappears.
    let mut tasks = JoinSet::new();
    for _ in 0..state.settings.scheduler.concurrency {
        let state = state.clone();
        tasks.spawn(async move {
            match deadline {
                Some(deadline) => tokio::time::timeout_at(deadline, execute_one(&state))
                    .await
                    .unwrap_or_else(|_| (false, Err(capacity_error()))),
                None => execute_one(&state).await,
            }
        });
    }
    let mut processed = false;
    let mut failure = None;
    // Every worker is awaited, including work cancelled at its deadline.
    while let Some(outcome) = tasks.join_next().await {
        let (claimed, result) =
            outcome.unwrap_or_else(|error| (false, Err(AppError::from(anyhow::Error::new(error)))));
        processed |= claimed;
        if let Err(error) = result
            && failure.is_none()
        {
            failure = Some(error);
        }
    }
    (processed, failure)
}

fn capacity_error() -> AppError {
    AppError::new(
        axum::http::StatusCode::SERVICE_UNAVAILABLE,
        "Scheduled reports exceeded invocation capacity; unfinished work will recover on a later trigger",
    )
}

pub(crate) async fn dispatch(state: &AppState) -> Result<(), AppError> {
    let now = Utc::now().naive_utc();
    let ids = repositories::cron_configurations::enabled(&state.pool).await?;
    let mut failure = None;
    for id in ids {
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
            crate::validations::cron::configs::validate_inputs(
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
                let due = schedule::next_occurrence(&configuration.cron_expression, cursor)
                    .map_err(config::schedule_error)?;
                if due.naive_utc() > now {
                    break;
                }
                if kind == domain::dto::ScheduleReportType::Transaction {
                    repositories::cron_jobs::enqueue(&mut transaction, id, due.naive_utc()).await?;
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
        if let Err(error) = result
            && failure.is_none()
        {
            failure = Some(error);
        }
    }
    failure.map_or(Ok(()), Err)
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
async fn execute_one(state: &AppState) -> (bool, Result<(), AppError>) {
    let claim = match job::claim(state).await {
        Ok(Some(claim)) => claim,
        Ok(None) => return (false, Ok(())),
        Err(failure) => return (failure.consumed, Err(failure.error)),
    };
    let outcome = match with_lease(state, &claim, job::execute(state, &claim)).await {
        Ok(()) => {
            tracing::info!(job_id = %claim.job.id, scheduled_for = %claim.job.scheduled_for, "cron job completed");
            Ok(())
        }
        Err(failure) => match job::fail(state, &claim, failure.message).await {
            Ok(()) => Err(failure.error),
            Err(error) => Err(error),
        },
    };
    (true, outcome)
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
                message: "Lease renewal failed; job side effect may be unconfirmed",
            })? {
                return Err(job::Failure {
                    error: job::lease_lost(),
                    message: "Job lease lost; side effect may be unconfirmed",
                });
            }
        }
    };
    tokio::select! { result=work=>result, result=renewal=>result }
}

#[cfg(test)]
#[path = "tests/cron.rs"]
mod tests;
