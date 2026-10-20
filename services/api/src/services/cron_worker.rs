use super::report_storage::{
    self,
    encode::{self, ReportData},
};
use crate::{
    error::AppError,
    models::{
        cron::{Job, file_format, report_type},
        shopify::{PageRequest, ShopifyId},
    },
    repositories,
    services::{
        self,
        cron_schedule::{
            CronScheduleError, TransactionPeriodMode, next_occurrence, transaction_window,
        },
    },
    state::AppState,
};
use axum::http::StatusCode;
use chrono::{DateTime, NaiveDateTime, Utc};
use domain::dto::{AccountingReportResponse, ScheduleReportType};
use sqlx::PgPool;
use std::collections::HashSet;
use tokio::task::JoinSet;
use uuid::Uuid;

const DISPATCH_BATCH: i64 = 50;
const MAX_CATCH_UP: usize = 1000;

pub async fn run(state: AppState) {
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(
        state.settings.scheduler.poll_interval_secs.max(1),
    ));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        interval.tick().await;
        if let Err(failure) = dispatch_due(&state.pool).await {
            crate::error::log_background_error("scheduled report dispatch", &failure);
        }
        process_generation_batch(&state).await;
        process_email_batch(&state).await;
        process_publication_batch(&state).await;
    }
}

pub async fn dispatch_due(pool: &PgPool) -> Result<(), AppError> {
    let now = Utc::now().naive_utc();
    let mut transaction = pool.begin().await?;
    let configurations =
        repositories::cron::lock_due_configurations(&mut transaction, now, DISPATCH_BATCH).await?;
    for config in configurations {
        let Some(first_due) = config.next_run_at else {
            continue;
        };
        let mut due = first_due;
        let mut next = next_run(&config.cron_expression, due)?;
        let mut latest_inventory_due = None;
        let mut count = 0;
        while due <= now && count < MAX_CATCH_UP {
            match report_type(&config.report_type) {
                ScheduleReportType::Transaction => {
                    let mode = match config.period_mode.as_deref() {
                        Some("PREVIOUS_MONTH") => TransactionPeriodMode::PreviousMonth,
                        Some("FIRST_HALF_MONTH") => TransactionPeriodMode::FirstHalfMonth,
                        _ => {
                            return Err(AppError::new(
                                StatusCode::INTERNAL_SERVER_ERROR,
                                "Invalid saved transaction schedule",
                            ));
                        }
                    };
                    let scheduled = DateTime::<Utc>::from_naive_utc_and_offset(due, Utc);
                    let period = transaction_window(mode, scheduled).map_err(schedule_error)?;
                    repositories::cron::insert_queued_result(
                        &mut transaction,
                        &config,
                        due,
                        Some(period.start),
                        Some(period.end_exclusive),
                    )
                    .await?;
                }
                ScheduleReportType::Inventory => latest_inventory_due = Some(due),
            }
            due = next;
            next = next_run(&config.cron_expression, due)?;
            count += 1;
        }
        if let Some(scheduled) = latest_inventory_due {
            repositories::cron::insert_queued_result(
                &mut transaction,
                &config,
                scheduled,
                None,
                None,
            )
            .await?;
        }
        repositories::cron::advance_schedule(&mut transaction, config.id, due).await?;
    }
    transaction.commit().await?;
    Ok(())
}

pub async fn process_generation_batch(state: &AppState) {
    let mut tasks = JoinSet::new();
    for _ in 0..state.settings.scheduler.concurrency.max(1) {
        let state = state.clone();
        tasks.spawn(async move { generate_one(&state).await });
    }
    while let Some(result) = tasks.join_next().await {
        match result {
            Ok(Ok(_)) => {}
            Ok(Err(error)) => {
                crate::error::log_background_error("scheduled report generation", &error);
            }
            Err(error) => {
                tracing::error!(error = %error, "scheduled report generation task failed");
            }
        }
    }
}

pub async fn process_email_batch(state: &AppState) {
    let now = Utc::now().naive_utc();
    if let Err(error) =
        repositories::cron::recover_expired(&state.pool, now, generation_attempt_limit(state)).await
    {
        crate::error::log_background_error("scheduled report email ambiguity recovery", &error);
        return;
    }
    let mut tasks = JoinSet::new();
    for _ in 0..state.settings.scheduler.concurrency.max(1) {
        let state = state.clone();
        tasks.spawn(async move { send_one(&state).await });
    }
    while let Some(result) = tasks.join_next().await {
        match result {
            Ok(Ok(_)) => {}
            Ok(Err(error)) => crate::error::log_background_error("scheduled report email", &error),
            Err(error) => tracing::error!(error = %error, "scheduled report email task failed"),
        }
    }
}

pub async fn process_publication_batch(state: &AppState) {
    for _ in 0..state.settings.scheduler.concurrency {
        if let Err(error) = publish_one(state).await {
            crate::error::log_background_error("scheduled report publication", &error);
        }
    }
}

async fn publish_one(state: &AppState) -> Result<bool, AppError> {
    let token = Uuid::new_v4();
    let now = Utc::now().naive_utc();
    let Some(job) = repositories::cron::claim_publication(
        &state.pool,
        token,
        now,
        lease_until(state),
        generation_attempt_limit(state),
    )
    .await?
    else {
        return Ok(false);
    };
    let outcome = with_lease(state, job.id, token, async {
        let files =
            report_storage::publish(&state.settings.reports.storage_directory, job.id).await?;
        let mut transaction = state.pool.begin().await?;
        if !repositories::cron::finish_publication(&mut transaction, job.id, Some(token)).await? {
            return Err(lease_lost());
        }
        if !repositories::report_results::save_files(&mut transaction, job.id, &files).await? {
            return Err(AppError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Report audit is missing",
            ));
        }
        transaction.commit().await?;
        Ok(())
    })
    .await;
    if let Err(error) = outcome {
        repositories::cron::record_publication_failure(
            &state.pool,
            job.id,
            token,
            next_email_retry(state, job.publication_attempts),
        )
        .await?;
        return Err(error);
    }
    Ok(true)
}

async fn send_one(state: &AppState) -> Result<bool, AppError> {
    let token = Uuid::new_v4();
    let Some(job) = repositories::cron::claim_email(
        &state.pool,
        token,
        Utc::now().naive_utc(),
        lease_until(state),
        generation_attempt_limit(state),
    )
    .await?
    else {
        return Ok(false);
    };
    let mut ambiguous = false;
    let outcome = with_lease(state, job.id, token, async {
        let prepared = report_storage::load(&state.settings.reports.storage_directory, job.id)
            .await?
            .ok_or_else(|| {
                AppError::new(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Generated report files are missing",
                )
            })?;
        let attachments = prepared
            .files
            .into_iter()
            .map(|file| (file.filename, file.bytes))
            .collect::<Vec<_>>();
        let encoded_size = attachments
            .iter()
            .map(|(_, bytes)| bytes.len().div_ceil(3).saturating_mul(4))
            .sum::<usize>();
        if encoded_size > state.settings.scheduler.max_email_bytes {
            return Err(AppError::new(
                StatusCode::PAYLOAD_TOO_LARGE,
                "Report attachments exceed configured email size limit",
            ));
        }
        let sender = state.settings.shipping_sender.as_ref().ok_or_else(|| {
            AppError::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "Report sender is not configured",
            )
        })?;
        let client = super::common::resend_client(state)?;
        ambiguous = true;
        let message_id = client
            .send_report_email(
                &sender.email,
                &job.receiver_email,
                "Your scheduled report",
                "<p>Your scheduled report is attached.</p>",
                attachments,
                &format!("report-result-{}", job.id),
            )
            .await
            .map_err(|error| {
                ambiguous = email_outcome_is_uncertain(&error);
                AppError::from(error)
            })?;
        ambiguous = true;
        if !repositories::cron::complete_email(
            &state.pool,
            job.id,
            token,
            message_id.as_ref(),
            Utc::now().naive_utc(),
        )
        .await?
        {
            return Err(lease_lost());
        }
        Ok(())
    })
    .await;
    if let Err(error) = outcome {
        let retry = next_email_retry(state, job.email_attempts);
        repositories::cron::fail_email(
            &state.pool,
            job.id,
            token,
            "Email delivery attempt failed",
            retry,
            ambiguous,
        )
        .await?;
        crate::error::log_background_error("scheduled report email", &error);
    }
    Ok(true)
}

async fn with_lease<T>(
    state: &AppState,
    id: Uuid,
    token: Uuid,
    work: impl std::future::Future<Output = Result<T, AppError>>,
) -> Result<T, AppError> {
    let mut heartbeat = tokio::time::interval(std::time::Duration::from_secs(
        (state.settings.scheduler.lease_secs / 3).max(1),
    ));
    let renewal = async {
        loop {
            heartbeat.tick().await;
            match repositories::cron::renew_lease(&state.pool, id, token, lease_until(state)).await
            {
                Ok(true) => {}
                Ok(false) => return lease_lost(),
                Err(error) => return error,
            }
        }
    };
    tokio::select! {
        result = work => result,
        error = renewal => Err(error),
    }
}

fn email_outcome_is_uncertain(error: &crate::client::ClientError) -> bool {
    use crate::client::ClientErrorKind;
    if crate::error::client_timeout(error) {
        return true;
    }
    match error.kind() {
        ClientErrorKind::InvalidInput(_) => false,
        ClientErrorKind::Upstream { source, .. } => !matches!(
            source.downcast_ref::<resend_rs::Error>(),
            Some(resend_rs::Error::Resend(_) | resend_rs::Error::RateLimit { .. })
        ),
        _ => true,
    }
}

fn lease_until(state: &AppState) -> NaiveDateTime {
    Utc::now().naive_utc()
        + chrono::Duration::seconds(duration_seconds(state.settings.scheduler.lease_secs))
}
fn lease_lost() -> AppError {
    AppError::new(
        StatusCode::CONFLICT,
        "Scheduled report lease is no longer owned",
    )
}

fn next_email_retry(state: &AppState, attempts: i32) -> Option<chrono::NaiveDateTime> {
    if attempts >= generation_attempt_limit(state) {
        return None;
    }
    let multiplier = u64::try_from(attempts.max(1)).unwrap_or(1);
    let seconds = state
        .settings
        .scheduler
        .retry_backoff_secs
        .saturating_mul(multiplier)
        .min(i64::MAX.cast_unsigned())
        .cast_signed();
    Some(Utc::now().naive_utc() + chrono::Duration::seconds(seconds))
}

async fn generate_one(state: &AppState) -> Result<bool, AppError> {
    let now = Utc::now().naive_utc();
    let token = Uuid::new_v4();
    let lease_until = now
        + chrono::Duration::seconds(duration_seconds(state.settings.scheduler.lease_secs.max(1)));
    let Some(job) = repositories::cron::claim_generation(
        &state.pool,
        token,
        now,
        lease_until,
        generation_attempt_limit(state),
    )
    .await?
    else {
        return Ok(false);
    };

    let outcome = with_lease(state, job.id, token, async {
        let prepared = if let Some(prepared) =
            report_storage::load(&state.settings.reports.storage_directory, job.id).await?
        {
            prepared
        } else {
            let data = generate_report(state, &job).await?;
            let generated_at = Utc::now().naive_utc();
            let format = file_format(&job.file_format);
            let files = tokio::task::spawn_blocking(move || encode::generate(data, format))
                .await
                .map_err(|error| AppError::from(anyhow::Error::new(error)))??;
            let size = files.iter().map(|file| file.bytes.len()).sum::<usize>();
            if size > state.settings.scheduler.max_payload_bytes {
                return Err(AppError::new(
                    StatusCode::PAYLOAD_TOO_LARGE,
                    "Report files exceed configured size limit",
                ));
            }
            report_storage::prepare(
                &state.settings.reports.storage_directory,
                job.id,
                generated_at,
                format,
                files,
            )
            .await?
        };
        if !repositories::cron::complete_generation(
            &state.pool,
            job.id,
            token,
            prepared.manifest.generated_at,
        )
        .await?
        {
            return Err(lease_lost());
        }
        Ok(())
    })
    .await;
    if let Err(error) = outcome {
        crate::error::log_background_error("scheduled report generation", &error);
        fail_job(state, &job, token, "Report generation failed").await?;
    }
    Ok(true)
}

async fn fail_job(
    state: &AppState,
    job: &Job,
    token: Uuid,
    message: &'static str,
) -> Result<(), AppError> {
    let retries = generation_attempt_limit(state);
    let backoff = if job.generation_attempts < retries {
        Some(
            Utc::now().naive_utc()
                + chrono::Duration::seconds(
                    state
                        .settings
                        .scheduler
                        .retry_backoff_secs
                        .saturating_mul(
                            u32::try_from(job.generation_attempts.max(1))
                                .unwrap_or(u32::MAX)
                                .into(),
                        )
                        .min(i64::MAX.cast_unsigned())
                        .cast_signed(),
                ),
        )
    } else {
        None
    };
    repositories::cron::fail_generation(&state.pool, job.id, token, message, backoff).await?;
    Ok(())
}

async fn generate_report(state: &AppState, job: &Job) -> Result<ReportData, AppError> {
    match report_type(&job.report_type) {
        ScheduleReportType::Transaction => {
            let start = job.period_start.ok_or_else(|| {
                AppError::new(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Transaction period is missing",
                )
            })?;
            let end = job.period_end_exclusive.ok_or_else(|| {
                AppError::new(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Transaction period is missing",
                )
            })?;
            let range = transaction_range(start, end);
            let result: AccountingReportResponse =
                services::accounting::report(state, range).await?;
            Ok(ReportData::Transaction(result))
        }
        ScheduleReportType::Inventory => {
            let id = job.inventory_id.as_deref().ok_or_else(|| {
                AppError::new(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Inventory location is missing",
                )
            })?;
            let location =
                ShopifyId::new(format!("gid://shopify/Location/{id}")).map_err(|error| {
                    AppError::new(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "Saved inventory location is invalid",
                    )
                    .with_source(error)
                })?;
            let mut cursor = None;
            let mut seen_cursors = HashSet::new();
            let mut products = Vec::new();
            loop {
                let page = services::inventories::inventory_report(
                    state,
                    location.clone(),
                    PageRequest {
                        first: 250,
                        after: cursor.clone(),
                    },
                )
                .await?;
                products.extend(page.products);
                if !page.page_info.has_next_page {
                    break;
                }
                let next = page.page_info.end_cursor.ok_or_else(|| {
                    AppError::new(
                        StatusCode::BAD_GATEWAY,
                        "Shopify inventory pagination cursor is missing",
                    )
                })?;
                if !seen_cursors.insert(next.clone()) {
                    return Err(AppError::new(
                        StatusCode::BAD_GATEWAY,
                        "Shopify inventory pagination cursor did not advance",
                    ));
                }
                cursor = Some(next);
            }
            Ok(ReportData::Inventory(products))
        }
    }
}

fn next_run(expression: &str, after: NaiveDateTime) -> Result<NaiveDateTime, AppError> {
    next_occurrence(
        expression,
        DateTime::<Utc>::from_naive_utc_and_offset(after, Utc),
    )
    .map(|value| value.naive_utc())
    .map_err(schedule_error)
}

fn schedule_error(error: CronScheduleError) -> AppError {
    match error {
        CronScheduleError::NoFutureOccurrence => AppError::new(
            StatusCode::BAD_REQUEST,
            "Cron schedule has no future occurrence",
        ),
        CronScheduleError::InvalidExpression | CronScheduleError::InvalidPeriod => AppError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to advance saved cron schedule",
        ),
    }
}

fn duration_seconds(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

fn generation_attempt_limit(state: &AppState) -> i32 {
    i32::try_from(state.settings.scheduler.max_generation_attempts.max(1)).unwrap_or(i32::MAX)
}

fn transaction_range(
    start: NaiveDateTime,
    end_exclusive: NaiveDateTime,
) -> crate::validations::accounting::AccountingDateRange {
    crate::validations::accounting::AccountingDateRange {
        start: start.and_utc().fixed_offset(),
        end: (end_exclusive - chrono::Duration::nanoseconds(1))
            .and_utc()
            .fixed_offset(),
    }
}

#[cfg(test)]
#[path = "../tests/services/cron_worker.rs"]
mod tests;
