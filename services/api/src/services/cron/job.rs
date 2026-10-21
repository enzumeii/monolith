use super::report;
use crate::{
    error::AppError,
    models::cron::{Job, report_type},
    repositories,
    state::AppState,
    validations,
};
use axum::http::StatusCode;
use chrono::{NaiveDateTime, Utc};
use uuid::Uuid;

pub(crate) struct ClaimedJob {
    pub job: Job,
    pub token: Uuid,
    pub recipient: String,
}
pub(crate) async fn claim(state: &AppState) -> Result<Option<ClaimedJob>, AppError> {
    let now = Utc::now().naive_utc();
    let Some(configuration) =
        repositories::cron_jobs::candidate(&state.pool, now, max_attempts(state)).await?
    else {
        return Ok(None);
    };
    let mut transaction = state.pool.begin().await?;
    let Some(config) =
        repositories::cron_configurations::lock(&mut transaction, configuration).await?
    else {
        return Ok(None);
    };
    let token = Uuid::new_v4();
    let Some(job) = repositories::cron_jobs::claim(
        &mut transaction,
        configuration,
        now,
        token,
        lease_until(state),
        max_attempts(state),
    )
    .await?
    else {
        return Ok(None);
    };
    // Existing audit context wins over later configuration edits.
    let existing = repositories::report_results::context(&mut transaction, job.id).await?;
    let context = if let Some(existing) = existing {
        Ok(existing)
    } else {
        (|| {
            let kind = report_type(&config.report_type)?;
            Ok::<_, AppError>((
                config.report_type.clone(),
                validations::cron::resolved(kind, &config.parameter, job.scheduled_for)?,
            ))
        })()
    };
    let (kind, parameters) = match context {
        Ok(context) => context,
        Err(error) => {
            repositories::cron_jobs::fail(
                &mut transaction,
                job.id,
                token,
                "Invalid saved report configuration",
                None,
            )
            .await?;
            transaction.commit().await?;
            return Err(error);
        }
    };
    repositories::report_results::start(&mut transaction, job.id, &kind, &parameters, now).await?;
    transaction.commit().await?;
    Ok(Some(ClaimedJob {
        job,
        token,
        recipient: config.receiver_email,
    }))
}
pub(crate) struct Failure {
    pub error: AppError,
    pub message: &'static str,
}
pub(crate) async fn execute(state: &AppState, claim: &ClaimedJob) -> Result<(), Failure> {
    let mut phase = "Report generation failed";
    let outcome = async {
        let audit = repositories::report_results::find(&state.pool, claim.job.id)
            .await?
            .ok_or_else(lease_lost)?;
        let root = state.settings.reports.storage.directory();
        let existing = crate::services::report_storage::load(root, claim.job.id).await?;
        let recovering = existing.is_some();
        let prepared = if let Some(prepared) = existing {
            prepared
        } else {
            if audit.generated_at.is_some() {
                return Err(AppError::new(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Generated files are missing; regeneration is disabled",
                ));
            }
            report::generate(state, &audit).await?
        };
        if !repositories::report_results::generated(
            &state.pool,
            claim.job.id,
            claim.token,
            prepared.manifest.generated_at,
        )
        .await?
        {
            return Err(lease_lost());
        }
        phase = "Email delivery failed or unconfirmed; may have been sent";
        crate::services::email::report(
            state,
            claim.job.id,
            &claim.recipient,
            prepared,
            recovering && claim.job.attempts > 1,
        )
        .await
        .map_err(|e| e.context("email delivery unconfirmed; may have been sent"))?;
        phase = "Stored report publication failed";
        let files =
            crate::services::report_storage::publish(&state.settings.reports.storage, claim.job.id)
                .await
                .map_err(|e| e.context("publication"))?;
        let mut transaction = state.pool.begin().await?;
        if !repositories::cron_jobs::finish(&mut transaction, claim.job.id, claim.token).await?
            || !repositories::report_results::complete(&mut transaction, claim.job.id, &files)
                .await?
        {
            return Err(lease_lost());
        }
        transaction.commit().await?;
        Ok(())
    }
    .await;
    outcome.map_err(|error| Failure {
        error,
        message: phase,
    })
}
pub(crate) async fn fail(
    state: &AppState,
    claim: &ClaimedJob,
    error: &str,
) -> Result<(), AppError> {
    let retry = crate::cron::retry_at(state, claim.job.attempts);
    let mut transaction = state.pool.begin().await?;
    if repositories::cron_jobs::fail(&mut transaction, claim.job.id, claim.token, error, retry)
        .await?
    {
        repositories::report_results::fail(&mut transaction, claim.job.id, error).await?;
    }
    transaction.commit().await?;
    Ok(())
}
pub(crate) fn lease_until(state: &AppState) -> NaiveDateTime {
    Utc::now().naive_utc() + chrono::Duration::seconds(seconds(state.settings.scheduler.lease_secs))
}
pub(crate) fn max_attempts(state: &AppState) -> i32 {
    i32::try_from(state.settings.scheduler.max_attempts).unwrap_or(i32::MAX)
}
pub(crate) fn lease_lost() -> AppError {
    AppError::new(
        StatusCode::CONFLICT,
        "Scheduled report lease is no longer owned",
    )
}
fn seconds(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

#[cfg(test)]
#[path = "../../tests/services/cron/job.rs"]
mod tests;
