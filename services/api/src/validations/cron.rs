use crate::{
    error::AppError,
    services::cron_schedule::{CronScheduleError, next_occurrence, normalize_expression},
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
        CronScheduleError::InvalidExpression | CronScheduleError::InvalidPeriod => {
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

fn valid_inventory_id(id: &str) -> bool {
    !id.is_empty() && !id.starts_with('0') && id.bytes().all(|byte| byte.is_ascii_digit())
}

fn invalid(message: &'static str) -> AppError {
    AppError::new(StatusCode::BAD_REQUEST, message)
}

#[cfg(test)]
#[path = "../tests/validations/cron.rs"]
mod tests;
