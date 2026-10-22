use crate::cron::schedule;
use crate::{
    error::AppError,
    models::cron::{ConfigurationFilter, ConfigurationInput, CronConfiguration, timestamp},
    repositories,
};
use axum::http::StatusCode;
use chrono::{DateTime, Datelike, Months, NaiveDate, NaiveDateTime, TimeZone, Utc};
use chrono_tz::Asia::Bangkok;
use domain::dto::{
    CronConfigurationDTO, CronConfigurationListResponse, CronConfigurationPreview,
    CronOccurrencePreview, CronPeriodPreview,
};
use sqlx::PgPool;
use uuid::Uuid;

pub(crate) async fn list(
    pool: &PgPool,
    filter: ConfigurationFilter,
) -> Result<CronConfigurationListResponse, AppError> {
    let mut records = repositories::cron_configurations::list(pool, filter).await?;
    let has_next = records.len() > filter.per_page as usize;
    records.truncate(filter.per_page as usize);
    Ok(CronConfigurationListResponse {
        items: records
            .into_iter()
            .map(CronConfiguration::into_dto)
            .collect::<Result<_, _>>()?,
        page: filter.page,
        per_page: filter.per_page,
        has_next,
    })
}
pub(crate) async fn get(pool: &PgPool, id: Uuid) -> Result<CronConfiguration, AppError> {
    repositories::cron_configurations::find(pool, id)
        .await?
        .ok_or_else(missing)
}
pub(crate) async fn create(
    pool: &PgPool,
    input: &ConfigurationInput,
) -> Result<CronConfigurationDTO, AppError> {
    repositories::cron_configurations::insert(pool, input)
        .await?
        .into_dto()
}
pub(crate) async fn update(
    pool: &PgPool,
    id: Uuid,
    observed: chrono::NaiveDateTime,
    input: &ConfigurationInput,
) -> Result<CronConfigurationDTO, AppError> {
    if let Some(record) =
        repositories::cron_configurations::update(pool, id, observed, input).await?
    {
        return record.into_dto();
    }
    Err(AppError::new(
        StatusCode::CONFLICT,
        "Cron configuration changed; reload before saving",
    ))
}
pub(crate) async fn delete(pool: &PgPool, id: Uuid) -> Result<(), AppError> {
    let mut transaction = pool.begin().await?;
    repositories::cron_configurations::lock(&mut transaction, id)
        .await?
        .ok_or_else(missing)?;
    repositories::cron_jobs::cancel(&mut transaction, id).await?;
    repositories::cron_configurations::delete(&mut transaction, id).await?;
    transaction.commit().await?;
    Ok(())
}
pub(crate) fn preview(
    request: &crate::validations::cron::configs::ValidatedPreview,
) -> Result<CronConfigurationPreview, AppError> {
    let scheduled =
        schedule::next_occurrences(&request.expression, Utc::now(), 5).map_err(schedule_error)?;
    let mut occurrences = Vec::new();
    for due in scheduled {
        let period = if let Some(mode) = request.period {
            let window = transaction_window(mode, due).map_err(|_| period_error())?;
            Some(CronPeriodPreview {
                period_start: timestamp(window.start),
                period_end_exclusive: timestamp(window.end_exclusive),
            })
        } else {
            None
        };
        occurrences.push(CronOccurrencePreview {
            scheduled_for: timestamp(due.naive_utc()),
            period,
        });
    }
    Ok(CronConfigurationPreview {
        schedule_timezone: "Asia/Bangkok".into(),
        description: schedule::describe(&request.expression).map_err(schedule_error)?,
        occurrences,
    })
}
pub(crate) fn schedule_error(_: schedule::CronScheduleError) -> AppError {
    AppError::new(
        StatusCode::INTERNAL_SERVER_ERROR,
        "Unable to evaluate saved cron schedule",
    )
}
fn missing() -> AppError {
    AppError::new(StatusCode::NOT_FOUND, "Cron configuration not found")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransactionPeriodMode {
    PreviousMonth,
    FirstHalfMonth,
}

#[derive(Debug, PartialEq, Eq)]
pub struct TransactionWindow {
    pub start: NaiveDateTime,
    pub end_exclusive: NaiveDateTime,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransactionWindowError {
    InvalidPeriod,
}

pub fn transaction_window(
    mode: TransactionPeriodMode,
    scheduled_for: DateTime<Utc>,
) -> Result<TransactionWindow, TransactionWindowError> {
    let local_date = scheduled_for.with_timezone(&Bangkok).date_naive();
    let month_start = NaiveDate::from_ymd_opt(local_date.year(), local_date.month(), 1)
        .ok_or(TransactionWindowError::InvalidPeriod)?;
    let (start_date, end_date) = match mode {
        TransactionPeriodMode::PreviousMonth => (
            month_start
                .checked_sub_months(Months::new(1))
                .ok_or(TransactionWindowError::InvalidPeriod)?,
            month_start,
        ),
        TransactionPeriodMode::FirstHalfMonth => (
            month_start,
            month_start
                .checked_add_days(chrono::Days::new(15))
                .ok_or(TransactionWindowError::InvalidPeriod)?,
        ),
    };
    let start = bangkok_utc_naive(start_date.and_time(chrono::NaiveTime::MIN))?;
    let end_exclusive = bangkok_utc_naive(end_date.and_time(chrono::NaiveTime::MIN))?;
    Ok(TransactionWindow {
        start,
        end_exclusive,
    })
}

fn bangkok_utc_naive(local: NaiveDateTime) -> Result<NaiveDateTime, TransactionWindowError> {
    Bangkok
        .from_local_datetime(&local)
        .single()
        .map(|instant| instant.with_timezone(&Utc).naive_utc())
        .ok_or(TransactionWindowError::InvalidPeriod)
}

fn period_error() -> AppError {
    AppError::new(
        StatusCode::INTERNAL_SERVER_ERROR,
        "Unable to evaluate saved report period",
    )
}

#[cfg(test)]
#[path = "../../tests/services/cron/config/period.rs"]
mod tests;
