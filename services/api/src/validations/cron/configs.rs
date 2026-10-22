use crate::{
    cron::schedule::{CronScheduleError, next_occurrence, normalize_expression},
    error::AppError,
};
use axum::http::StatusCode;
use chrono::Utc;
use domain::dto::{SchedulePeriodMode, ScheduleReportType};

pub fn validate_report_inputs(
    report_type: ScheduleReportType,
    cron_expression: &str,
    period_mode: Option<SchedulePeriodMode>,
    inventory_id: Option<&str>,
) -> Result<String, AppError> {
    let expression = normalize_expression(cron_expression)
        .map_err(|_| invalid("cron_expression must be a valid five-field cron expression"))?;
    next_occurrence(&expression, Utc::now()).map_err(|error| match error {
        CronScheduleError::NoFutureOccurrence => {
            invalid("cron_expression has no future occurrence")
        }
        CronScheduleError::InvalidExpression => {
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
            if !inventory_id.is_some_and(valid_inventory_id) {
                return Err(invalid(
                    "Inventory schedules require a positive inventory_id",
                ));
            }
        }
    }
    Ok(expression)
}

pub fn validate_name(name: &str) -> Result<String, AppError> {
    let name = name.trim();
    if !(1..=120).contains(&name.chars().count()) {
        return Err(invalid("name must contain 1 to 120 characters"));
    }
    Ok(name.to_owned())
}

pub fn validate_email(email: &str) -> Result<String, AppError> {
    let email = email.trim();
    let Some((local, domain)) = email.split_once('@') else {
        return Err(invalid("receiver_email must be a valid email address"));
    };
    if email.len() > 254
        || email.chars().any(char::is_whitespace)
        || local.is_empty()
        || local.starts_with('.')
        || local.ends_with('.')
        || local.contains("..")
        || domain.is_empty()
        || domain.starts_with('.')
        || domain.ends_with('.')
        || !domain.contains('.')
        || email.matches('@').count() != 1
    {
        return Err(invalid("receiver_email must be a valid email address"));
    }
    Ok(email.to_owned())
}

pub(super) fn valid_inventory_id(id: &str) -> bool {
    !id.is_empty() && !id.starts_with('0') && id.bytes().all(|byte| byte.is_ascii_digit())
}

pub(super) fn invalid(message: &'static str) -> AppError {
    AppError::new(StatusCode::BAD_REQUEST, message)
}

pub(crate) struct ValidatedPreview {
    pub expression: String,
    pub period: Option<crate::services::cron::config::TransactionPeriodMode>,
}
pub(crate) fn validate_preview(
    request: &domain::dto::PreviewCronConfiguration,
) -> Result<ValidatedPreview, AppError> {
    Ok(ValidatedPreview {
        expression: validate_inputs(
            request.report_type,
            &request.cron_expression,
            &request.parameter,
        )?,
        period: if request.report_type == ScheduleReportType::Transaction {
            Some(validate_period(&request.parameter)?)
        } else {
            None
        },
    })
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
pub(crate) fn validate_create(
    request: domain::dto::CreateCronConfiguration,
) -> Result<crate::models::cron::ConfigurationInput, AppError> {
    Ok(crate::models::cron::ConfigurationInput {
        name: validate_name(&request.name)?,
        receiver_email: validate_email(&request.receiver_email)?,
        cron_expression: validate_inputs(
            request.report_type,
            &request.cron_expression,
            &request.parameter,
        )?,
        report_type: request.report_type,
        parameter: request.parameter,
        enabled: request.enabled,
    })
}
pub(crate) fn validate_patch_shape(
    patch: &domain::dto::PatchCronConfiguration,
) -> Result<(), AppError> {
    use domain::dto::PatchField;
    let fields = [
        matches!(patch.name, PatchField::Missing),
        matches!(patch.report_type, PatchField::Missing),
        matches!(patch.cron_expression, PatchField::Missing),
        matches!(patch.receiver_email, PatchField::Missing),
        matches!(patch.parameter, PatchField::Missing),
        matches!(patch.enabled, PatchField::Missing),
    ];
    if fields.into_iter().all(|missing| missing) {
        return Err(invalid("PATCH body must contain at least one field"));
    }
    Ok(())
}
pub(crate) fn validate_patch(
    patch: domain::dto::PatchCronConfiguration,
    current: &crate::models::cron::CronConfiguration,
) -> Result<crate::models::cron::ConfigurationInput, AppError> {
    validate_create(domain::dto::CreateCronConfiguration {
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
pub(crate) fn validate_filter(
    query: &domain::dto::ListCronConfigurationsQuery,
) -> Result<crate::models::cron::ConfigurationFilter, AppError> {
    let (page, per_page) = validate_page(query.page, query.per_page)?;
    Ok(crate::models::cron::ConfigurationFilter {
        report_type: query.report_type,
        enabled: query.enabled,
        page,
        per_page,
    })
}
pub(crate) fn validate_page(
    page: Option<u32>,
    per_page: Option<u32>,
) -> Result<(u32, u32), AppError> {
    let page = page.unwrap_or(1);
    let per_page = per_page.unwrap_or(25);
    if page == 0 || !(1..=100).contains(&per_page) {
        return Err(invalid(
            "page must be positive and per_page must be between 1 and 100",
        ));
    }
    Ok((page, per_page))
}
pub(crate) fn validate_body<T>(
    value: Result<axum::Json<T>, axum::extract::rejection::JsonRejection>,
) -> Result<T, AppError> {
    value
        .map(|axum::Json(value)| value)
        .map_err(|_| invalid("Invalid cron request body"))
}
pub(crate) fn validate_query<T>(
    value: Result<axum::extract::Query<T>, axum::extract::rejection::QueryRejection>,
) -> Result<T, AppError> {
    value
        .map(|axum::extract::Query(value)| value)
        .map_err(|_| invalid("Invalid cron query"))
}
pub(crate) fn validate_id(
    value: Result<axum::extract::Path<String>, axum::extract::rejection::PathRejection>,
) -> Result<uuid::Uuid, AppError> {
    let axum::extract::Path(value) = value.map_err(|_| invalid("Invalid cron identifier"))?;
    value
        .parse()
        .map_err(|_| invalid("Invalid cron identifier"))
}
pub(crate) fn validate_period(
    value: &str,
) -> Result<crate::services::cron::config::TransactionPeriodMode, AppError> {
    use crate::services::cron::config::TransactionPeriodMode;
    match value {
        "PREVIOUS_MONTH" => Ok(TransactionPeriodMode::PreviousMonth),
        "FIRST_HALF_MONTH" => Ok(TransactionPeriodMode::FirstHalfMonth),
        _ => Err(invalid("Invalid transaction parameter")),
    }
}
fn merge<T>(patch: domain::dto::PatchField<T>, current: T) -> Result<T, AppError> {
    match patch {
        domain::dto::PatchField::Missing => Ok(current),
        domain::dto::PatchField::Set(value) => Ok(value),
        domain::dto::PatchField::Clear => Err(invalid("This field cannot be cleared")),
    }
}
#[cfg(test)]
#[path = "../../tests/validations/cron.rs"]
mod tests;
