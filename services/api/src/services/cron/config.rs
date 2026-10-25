use crate::cron;
use crate::{
    error::AppError,
    models::cron::{ConfigurationFilter, ConfigurationInput, CronConfiguration},
    repositories, transformers, validations,
};
use axum::http::StatusCode;
use chrono::{DateTime, Datelike, Months, NaiveDate, NaiveDateTime, TimeZone, Utc};
use chrono_tz::Asia::Bangkok;
use domain::dto::{
    CreateCronConfiguration, CronConfigurationDTO, CronConfigurationListResponse,
    CronConfigurationPreview, PatchCronConfiguration, PreviewCronConfiguration, SchedulePeriodMode,
    ScheduleReportType,
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
            .map(transformers::cron::configs::configuration)
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
    request: CreateCronConfiguration,
) -> Result<CronConfigurationDTO, AppError> {
    let input = configuration_input(request)?;
    transformers::cron::configs::configuration(
        repositories::cron_configurations::insert(pool, &input).await?,
    )
}

pub(crate) async fn update(
    pool: &PgPool,
    id: Uuid,
    patch: PatchCronConfiguration,
) -> Result<CronConfigurationDTO, AppError> {
    let current = get(pool, id).await?;
    let request = merge_configuration(patch, &current)?;
    let request = validations::cron::configs::validate_create(request)?;
    let input = configuration_input(request)?;
    if let Some(record) =
        repositories::cron_configurations::update(pool, id, current.updated_at, &input).await?
    {
        return transformers::cron::configs::configuration(record);
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
    request: &PreviewCronConfiguration,
) -> Result<CronConfigurationPreview, AppError> {
    let expression = validate_inputs(
        request.report_type,
        &request.cron_expression,
        &request.parameter,
    )?;
    let mode = if request.report_type == ScheduleReportType::Transaction {
        Some(validate_period(&request.parameter)?)
    } else {
        None
    };
    let scheduled =
        cron::schedule::next_occurrences(&expression, Utc::now(), 5).map_err(schedule_error)?;
    let mut occurrences = Vec::new();
    for due in scheduled {
        let period = mode
            .map(|mode| transaction_window(mode, due))
            .transpose()
            .map_err(|_| period_error())?;
        occurrences.push(transformers::cron::configs::occurrence(
            due.naive_utc(),
            period,
        ));
    }
    let description = cron::schedule::describe(&expression).map_err(schedule_error)?;
    Ok(transformers::cron::configs::preview(
        description,
        occurrences,
    ))
}

pub(crate) fn schedule_error(_: cron::schedule::CronScheduleError) -> AppError {
    AppError::new(
        StatusCode::INTERNAL_SERVER_ERROR,
        "Unable to evaluate saved cron schedule",
    )
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

pub fn validate_report_inputs(
    report_type: ScheduleReportType,
    cron_expression: &str,
    period_mode: Option<SchedulePeriodMode>,
    inventory_id: Option<&str>,
) -> Result<String, AppError> {
    let expression = validations::cron::configs::validate_expression(cron_expression)?;
    cron::schedule::next_occurrence(&expression, Utc::now()).map_err(|error| match error {
        cron::schedule::CronScheduleError::NoFutureOccurrence => {
            invalid("cron_expression has no future occurrence")
        }
        cron::schedule::CronScheduleError::InvalidExpression => {
            invalid("cron_expression must be a valid five-field cron expression")
        }
    })?;
    let fields = expression.split_whitespace().collect::<Vec<_>>();
    match report_type {
        ScheduleReportType::Transaction => {
            let Some(mode) = period_mode else {
                return Err(invalid("Transaction schedules require period_mode"));
            };
            if inventory_id.is_some() {
                return Err(invalid("Transaction schedules cannot include inventory_id"));
            }
            let expected_day = match mode {
                SchedulePeriodMode::PreviousMonth => "1",
                SchedulePeriodMode::FirstHalfMonth => "16",
            };
            let valid_minute = fields[0].parse::<u8>().is_ok_and(|minute| minute <= 59);
            let valid_hour = fields[1].parse::<u8>().is_ok_and(|hour| hour <= 23);
            if fields[2] != expected_day
                || fields[3] != "*"
                || fields[4] != "*"
                || !valid_minute
                || !valid_hour
            {
                return Err(invalid(
                    "Transaction cron day and calendar fields must match period_mode",
                ));
            }
        }
        ScheduleReportType::Inventory => {
            if period_mode.is_some() {
                return Err(invalid("Inventory schedules cannot include period_mode"));
            }
            if !inventory_id.is_some_and(crate::models::cron::configs::valid_inventory_id) {
                return Err(invalid(
                    "Inventory schedules require a positive inventory_id",
                ));
            }
        }
    }
    Ok(expression)
}

pub(crate) fn validate_inputs(
    kind: ScheduleReportType,
    expression: &str,
    parameter: &str,
) -> Result<String, AppError> {
    let (mode, inventory) = match kind {
        ScheduleReportType::Transaction => {
            let mode = match parameter {
                "PREVIOUS_MONTH" => SchedulePeriodMode::PreviousMonth,
                "FIRST_HALF_MONTH" => SchedulePeriodMode::FirstHalfMonth,
                _ => {
                    return Err(invalid(
                        "Transaction parameter must be PREVIOUS_MONTH or FIRST_HALF_MONTH",
                    ));
                }
            };
            (Some(mode), None)
        }
        ScheduleReportType::Inventory => (None, Some(parameter)),
    };
    validate_report_inputs(kind, expression, mode, inventory)
}
pub(crate) fn validate_period(value: &str) -> Result<TransactionPeriodMode, AppError> {
    match value {
        "PREVIOUS_MONTH" => Ok(TransactionPeriodMode::PreviousMonth),
        "FIRST_HALF_MONTH" => Ok(TransactionPeriodMode::FirstHalfMonth),
        _ => Err(invalid("Invalid transaction parameter")),
    }
}
fn configuration_input(request: CreateCronConfiguration) -> Result<ConfigurationInput, AppError> {
    let expression = validate_inputs(
        request.report_type,
        &request.cron_expression,
        &request.parameter,
    )?;
    Ok(transformers::cron::configs::input(request, expression))
}

fn merge_configuration(
    patch: PatchCronConfiguration,
    current: &CronConfiguration,
) -> Result<CreateCronConfiguration, AppError> {
    Ok(CreateCronConfiguration {
        name: merge(patch.name, current.name.clone())?,
        report_type: merge(
            patch.report_type,
            crate::models::cron::report_type(&current.report_type)?,
        )?,
        cron_expression: merge(patch.cron_expression, current.cron_expression.clone())?,
        receiver_email: merge(patch.receiver_email, current.receiver_email.clone())?,
        parameter: merge(patch.parameter, current.parameter.clone())?,
        enabled: merge(patch.enabled, current.enabled)?,
    })
}

fn invalid(message: &'static str) -> AppError {
    AppError::new(StatusCode::BAD_REQUEST, message)
}

fn missing() -> AppError {
    AppError::new(StatusCode::NOT_FOUND, "Cron configuration not found")
}

fn merge<T>(patch: domain::dto::PatchField<T>, current: T) -> Result<T, AppError> {
    match patch {
        domain::dto::PatchField::Missing => Ok(current),
        domain::dto::PatchField::Set(value) => Ok(value),
        domain::dto::PatchField::Clear => Err(invalid("This field cannot be cleared")),
    }
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

#[cfg(test)]
#[path = "../../tests/services/cron/config/inputs.rs"]
mod input_tests;
