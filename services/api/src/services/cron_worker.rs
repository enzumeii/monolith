use crate::{
    error::AppError,
    models::{
        cron::report_type,
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
use serde_json::{Value, json};
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
    let cutoff = now - chrono::Duration::hours(24);
    if let Err(error) =
        repositories::cron::mark_ambiguous_emails_unknown(&state.pool, cutoff, now).await
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

#[allow(clippy::too_many_lines)]
async fn send_one(state: &AppState) -> Result<bool, AppError> {
    let now = Utc::now().naive_utc();
    let token = Uuid::new_v4();
    let lease = now
        + chrono::Duration::seconds(duration_seconds(state.settings.scheduler.lease_secs.max(1)));
    let Some(job) = repositories::cron::claim_email(
        &state.pool,
        token,
        now,
        lease,
        generation_attempt_limit(state),
    )
    .await?
    else {
        return Ok(false);
    };
    let (datasets, _) = match job.report_type.as_str() {
        "TRANSACTION" => (vec!["transactions", "items"], 2),
        "INVENTORY" => (vec!["inventory"], 1),
        _ => {
            repositories::cron::fail_email(
                &state.pool,
                job.id,
                token,
                "Saved report type is invalid",
                None,
                false,
            )
            .await?;
            return Ok(true);
        }
    };
    let mut attachments = Vec::new();
    for dataset in datasets {
        let csv =
            services::report_results::csv_from_saved(&job.report_type, &job.report_data, dataset)?;
        attachments.push((
            format!("{}-{dataset}.csv", safe_report_name(&job.cron_name)),
            csv.into_bytes(),
        ));
    }
    let encoded_size = attachments
        .iter()
        .map(|(_, bytes)| bytes.len().saturating_mul(4).div_ceil(3))
        .sum::<usize>();
    if encoded_size > state.settings.scheduler.max_email_bytes {
        repositories::cron::fail_email(
            &state.pool,
            job.id,
            token,
            "Report attachments exceed configured email size limit",
            None,
            false,
        )
        .await?;
        return Ok(true);
    }
    let Some(sender) = state
        .settings
        .shipping_sender
        .as_ref()
        .map(|sender| sender.email.as_str())
    else {
        let retry = next_email_retry(state, job.email_attempts);
        repositories::cron::fail_email(
            &state.pool,
            job.id,
            token,
            "Report sender is not configured",
            retry,
            false,
        )
        .await?;
        return Ok(true);
    };
    let client = match super::common::resend_client(state) {
        Ok(client) => client,
        Err(error) => {
            let retry = next_email_retry(state, job.email_attempts);
            repositories::cron::fail_email(
                &state.pool,
                job.id,
                token,
                "Resend client unavailable",
                retry,
                false,
            )
            .await?;
            tracing::warn!(result_id=%job.id, error=%error, "scheduled report email provider unavailable");
            return Ok(true);
        }
    };
    match client
        .send_report_email(
            sender,
            &job.receiver_email,
            "Your scheduled report",
            "<p>Your scheduled report is attached.</p>",
            attachments,
            &format!("report-result-{}", job.id),
        )
        .await
    {
        Ok(message_id) => {
            repositories::cron::complete_email(
                &state.pool,
                job.id,
                token,
                message_id.as_ref(),
                Utc::now().naive_utc(),
            )
            .await?;
        }
        Err(error) => {
            let ambiguous = crate::error::client_timeout(&error);
            let retry = if ambiguous {
                Some(
                    Utc::now().naive_utc()
                        + chrono::Duration::seconds(duration_seconds(
                            state.settings.scheduler.retry_backoff_secs.max(1),
                        )),
                )
            } else {
                next_email_retry(state, job.email_attempts)
            };
            repositories::cron::fail_email(
                &state.pool,
                job.id,
                token,
                "Email delivery attempt failed",
                retry,
                ambiguous,
            )
            .await?;
            tracing::warn!(result_id=%job.id, error=%error, ambiguous, "scheduled report email delivery failed");
        }
    }
    Ok(true)
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

fn safe_report_name(name: &str) -> String {
    let name = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect::<String>();
    let name = name.trim_matches('-');
    if name.is_empty() {
        "report".to_owned()
    } else {
        name.to_owned()
    }
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

    let generated = generate_report(state, &job).await;
    match generated {
        Ok((data, row_count)) => {
            let size = serde_json::to_vec(&data)
                .map_err(|error| AppError::from(anyhow::Error::new(error)))?
                .len();
            if size > state.settings.scheduler.max_payload_bytes {
                fail_job(
                    state,
                    &job,
                    token,
                    "Report payload exceeds configured size limit",
                )
                .await?;
                return Ok(true);
            }
            repositories::cron::complete_generation(
                &state.pool,
                job.id,
                token,
                &data,
                row_count,
                Utc::now().naive_utc(),
            )
            .await?;
        }
        Err(error) => {
            tracing::warn!(result_id=%job.id, error=%error, "scheduled report generation failed");
            fail_job(state, &job, token, "Report generation failed").await?;
        }
    }
    Ok(true)
}

async fn fail_job(
    state: &AppState,
    job: &repositories::cron::GenerationJob,
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

async fn generate_report(
    state: &AppState,
    job: &repositories::cron::GenerationJob,
) -> Result<(Value, i64), AppError> {
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
            let range = crate::validations::accounting::AccountingDateRange {
                start: start.and_utc().fixed_offset(),
                end: end.and_utc().fixed_offset(),
            };
            let result: AccountingReportResponse =
                services::accounting::report(state, range).await?;
            let transaction_count = result
                .transactions
                .stripe
                .len()
                .saturating_add(result.transactions.paypal.len());
            let count = i64::try_from(transaction_count.saturating_add(result.items.len()))
                .unwrap_or(i64::MAX);
            let data = serde_json::to_value(result)
                .map_err(|error| AppError::from(anyhow::Error::new(error)))?;
            Ok((data, count))
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
            let count = i64::try_from(products.len()).unwrap_or(i64::MAX);
            Ok((json!({"inventory": products}), count))
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
