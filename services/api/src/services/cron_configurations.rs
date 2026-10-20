use crate::{
    error::AppError,
    models::cron::{
        ConfigurationFilter, CronConfiguration, file_format, period_mode, report_type, status,
    },
    repositories,
    services::cron_schedule::{
        CronScheduleError, TransactionPeriodMode, describe, next_occurrence, next_occurrences,
        transaction_window,
    },
    validations::cron as validate,
};
use axum::http::StatusCode;
use chrono::Utc;
use domain::dto::{
    CreateCronConfiguration, CronConfigurationDTO, CronConfigurationListResponse,
    CronConfigurationPreview, CronOccurrencePreview, CronPeriodPreview, PatchCronConfiguration,
    PatchField, PreviewCronConfiguration, SchedulePeriodMode, ScheduleStatus,
};
use sqlx::PgPool;
use uuid::Uuid;

pub async fn list(
    pool: &PgPool,
    query: domain::dto::ListCronConfigurationsQuery,
) -> Result<CronConfigurationListResponse, AppError> {
    let page = query.page.unwrap_or(1);
    let per_page = query.per_page.unwrap_or(25);
    validate_page(page, per_page)?;
    let mut records = repositories::cron::list_configurations(
        pool,
        ConfigurationFilter {
            report_type: query.report_type,
            status: query.status,
            page,
            per_page,
        },
    )
    .await?;
    let has_next = records.len() > per_page as usize;
    records.truncate(per_page as usize);
    Ok(CronConfigurationListResponse {
        items: records
            .into_iter()
            .map(CronConfiguration::into_dto)
            .collect(),
        page,
        per_page,
        has_next,
    })
}

pub async fn get(pool: &PgPool, id: Uuid) -> Result<CronConfigurationDTO, AppError> {
    repositories::cron::find_configuration(pool, id)
        .await?
        .map(CronConfiguration::into_dto)
        .ok_or_else(missing_configuration)
}

pub async fn create(
    pool: &PgPool,
    request: CreateCronConfiguration,
) -> Result<CronConfigurationDTO, AppError> {
    let name = validate::validate_name(&request.name)?;
    let receiver_email = validate::validate_email(&request.receiver_email)?;
    let cron_expression = validate::validate_report_inputs(
        request.report_type,
        &request.cron_expression,
        request.period_mode,
        request.inventory_id.as_deref(),
    )?;
    let next_run_at = next_run(request.status, &cron_expression)?;
    let mut transaction = pool.begin().await?;
    let record = repositories::cron::insert_configuration(
        &mut transaction,
        &name,
        request.report_type,
        &cron_expression,
        &receiver_email,
        request.period_mode,
        request.inventory_id.as_deref(),
        request.status,
        next_run_at,
        request.file_format,
    )
    .await?;
    transaction.commit().await?;
    Ok(record.into_dto())
}

pub async fn update(
    pool: &PgPool,
    id: Uuid,
    patch: PatchCronConfiguration,
) -> Result<CronConfigurationDTO, AppError> {
    if is_empty_patch(&patch) {
        return Err(invalid("PATCH body must contain at least one field"));
    }
    let mut transaction = pool.begin().await?;
    let current = repositories::cron::lock_configuration(&mut transaction, id)
        .await?
        .ok_or_else(missing_configuration)?;
    let current_report_type = report_type(&current.report_type);
    let current_period_mode = current.period_mode.as_deref().map(period_mode);
    let format = merge_required(&patch.file_format, file_format(&current.file_format))?;
    let current_status = status(&current.status);
    let current_cron_expression = current.cron_expression.clone();
    let current_inventory_id = current.inventory_id.clone();

    let name = merge_required(&patch.name, current.name.clone())?;
    let name = validate::validate_name(&name)?;
    let receiver_email = merge_required(&patch.receiver_email, current.receiver_email.clone())?;
    let receiver_email = validate::validate_email(&receiver_email)?;
    let report_type = merge_required(&patch.report_type, current_report_type)?;
    let cron_expression = merge_required(&patch.cron_expression, current_cron_expression.clone())?;
    let period_mode = merge_optional(&patch.period_mode, current_period_mode);
    let inventory_id = merge_optional(&patch.inventory_id, current_inventory_id.clone());
    let status = merge_required(&patch.status, current_status)?;
    let cron_expression = validate::validate_report_inputs(
        report_type,
        &cron_expression,
        period_mode,
        inventory_id.as_deref(),
    )?;

    let schedule_changed = report_type != current_report_type
        || cron_expression != current_cron_expression
        || period_mode != current_period_mode
        || inventory_id != current_inventory_id;
    let became_enabled =
        current_status == ScheduleStatus::Disabled && status == ScheduleStatus::Enabled;
    let next_run_at = if status == ScheduleStatus::Disabled {
        None
    } else if schedule_changed || became_enabled {
        next_run(status, &cron_expression)?
    } else {
        current.next_run_at
    };
    let updated = repositories::cron::update_configuration(
        &mut transaction,
        id,
        &name,
        report_type,
        &cron_expression,
        &receiver_email,
        period_mode,
        inventory_id.as_deref(),
        status,
        next_run_at,
        format,
    )
    .await?;
    transaction.commit().await?;
    Ok(updated.into_dto())
}

pub async fn delete(pool: &PgPool, id: Uuid) -> Result<(), AppError> {
    let mut transaction = pool.begin().await?;
    repositories::cron::lock_configuration(&mut transaction, id)
        .await?
        .ok_or_else(missing_configuration)?;
    repositories::cron::delete_configuration(&mut transaction, id).await?;
    transaction.commit().await?;
    Ok(())
}

pub fn preview(request: &PreviewCronConfiguration) -> Result<CronConfigurationPreview, AppError> {
    let expression = validate::validate_report_inputs(
        request.report_type,
        &request.cron_expression,
        request.period_mode,
        request.inventory_id.as_deref(),
    )?;
    let description = describe(&expression)
        .map_err(|_| invalid("cron_expression must be a valid five-field cron expression"))?;
    let scheduled = next_occurrences(&expression, Utc::now(), 5)
        .map_err(|_| invalid("cron_expression has no future occurrence"))?;
    let occurrences = scheduled
        .into_iter()
        .map(|scheduled_for| {
            let period = request.period_mode.map(|mode| {
                let period_mode = match mode {
                    SchedulePeriodMode::PreviousMonth => TransactionPeriodMode::PreviousMonth,
                    SchedulePeriodMode::FirstHalfMonth => TransactionPeriodMode::FirstHalfMonth,
                };
                transaction_window(period_mode, scheduled_for)
                    .map(|window| CronPeriodPreview {
                        period_start: timestamp(window.start),
                        period_end_exclusive: timestamp(window.end_exclusive),
                    })
                    .map_err(|_| invalid("Unable to calculate transaction period"))
            });
            Ok(CronOccurrencePreview {
                scheduled_for: timestamp(scheduled_for.naive_utc()),
                period: period.transpose()?,
            })
        })
        .collect::<Result<Vec<_>, AppError>>()?;
    Ok(CronConfigurationPreview {
        schedule_timezone: "Asia/Bangkok".to_owned(),
        description,
        occurrences,
    })
}

fn next_run(
    status: ScheduleStatus,
    expression: &str,
) -> Result<Option<chrono::NaiveDateTime>, AppError> {
    if status == ScheduleStatus::Disabled {
        return Ok(None);
    }
    next_occurrence(expression, Utc::now())
        .map(|next| Some(next.naive_utc()))
        .map_err(|error| match error {
            CronScheduleError::NoFutureOccurrence => {
                invalid("cron_expression has no future occurrence")
            }
            CronScheduleError::InvalidExpression | CronScheduleError::InvalidPeriod => {
                invalid("cron_expression must be a valid five-field cron expression")
            }
        })
}

fn is_empty_patch(patch: &PatchCronConfiguration) -> bool {
    matches!(&patch.file_format, PatchField::Missing)
        && matches!(&patch.name, PatchField::Missing)
        && matches!(&patch.report_type, PatchField::Missing)
        && matches!(&patch.cron_expression, PatchField::Missing)
        && matches!(&patch.receiver_email, PatchField::Missing)
        && matches!(&patch.period_mode, PatchField::Missing)
        && matches!(&patch.inventory_id, PatchField::Missing)
        && matches!(&patch.status, PatchField::Missing)
}

fn merge_required<T: Clone>(field: &PatchField<T>, current: T) -> Result<T, AppError> {
    match field {
        PatchField::Missing => Ok(current),
        PatchField::Set(value) => Ok(value.clone()),
        PatchField::Clear => Err(invalid("This field cannot be cleared")),
    }
}

fn merge_optional<T: Clone>(field: &PatchField<T>, current: Option<T>) -> Option<T> {
    match field {
        PatchField::Missing => current,
        PatchField::Set(value) => Some(value.clone()),
        PatchField::Clear => None,
    }
}

fn validate_page(page: u32, per_page: u32) -> Result<(), AppError> {
    if page == 0 || !(1..=100).contains(&per_page) {
        return Err(invalid(
            "page must be positive and per_page must be between 1 and 100",
        ));
    }
    Ok(())
}

fn missing_configuration() -> AppError {
    AppError::new(StatusCode::NOT_FOUND, "Cron configuration not found")
}

fn invalid(message: &'static str) -> AppError {
    AppError::new(StatusCode::BAD_REQUEST, message)
}

fn timestamp(value: chrono::NaiveDateTime) -> String {
    chrono::DateTime::<Utc>::from_naive_utc_and_offset(value, Utc)
        .to_rfc3339_opts(chrono::SecondsFormat::AutoSi, true)
}
