use crate::{error::AppError, models::cron::report::ResultFilter};
use axum::http::StatusCode;
use chrono::{DateTime, NaiveDateTime, Timelike, Utc};
use domain::dto::ListReportResultsQuery;
use domain::dto::ScheduleReportType;

pub(crate) fn validate_filter(query: &ListReportResultsQuery) -> Result<ResultFilter, AppError> {
    let (page, per_page) = super::configs::validate_page(query.page, query.per_page)?;
    let generated_from = query
        .generated_from
        .as_deref()
        .map(validate_timestamp)
        .transpose()?;
    let generated_to = query
        .generated_to
        .as_deref()
        .map(validate_timestamp)
        .transpose()?;
    if generated_from
        .zip(generated_to)
        .is_some_and(|(from, to)| from >= to)
    {
        return Err(invalid("generated_from must be earlier than generated_to"));
    }
    Ok(ResultFilter {
        report_type: query.report_type,
        generated_from,
        generated_to,
        page,
        per_page,
    })
}
pub(crate) fn validate_timestamp(value: &str) -> Result<NaiveDateTime, AppError> {
    Ok(DateTime::parse_from_rfc3339(value)
        .map_err(|_| invalid("Timestamps must be RFC3339 with an explicit offset"))?
        .with_timezone(&Utc)
        .naive_utc())
}
pub(crate) fn validate_resolved(
    kind: ScheduleReportType,
    value: &str,
    due: chrono::NaiveDateTime,
) -> Result<String, AppError> {
    if kind == ScheduleReportType::Inventory {
        if !super::configs::valid_inventory_id(value) {
            return Err(super::configs::invalid(
                "Inventory schedules require a positive inventory ID",
            ));
        }
        return Ok(value.into());
    }
    let window = crate::services::cron::config::transaction_window(
        super::configs::validate_period(value)?,
        due.and_utc(),
    )
    .map_err(|_| super::configs::invalid("Invalid report period"))?;
    let start = window
        .start
        .and_utc()
        .with_timezone(&chrono_tz::Asia::Bangkok);
    let end = (window.end_exclusive - chrono::Duration::seconds(1))
        .and_utc()
        .with_timezone(&chrono_tz::Asia::Bangkok);
    Ok(format!(
        "{} - {}",
        start.to_rfc3339_opts(chrono::SecondsFormat::Secs, false),
        end.to_rfc3339_opts(chrono::SecondsFormat::Secs, false)
    ))
}
pub(crate) fn validate_saved_parameters(
    kind: ScheduleReportType,
    value: &str,
) -> Result<Option<crate::validations::accounting::AccountingDateRange>, AppError> {
    if kind == ScheduleReportType::Inventory {
        return if super::configs::valid_inventory_id(value) {
            Ok(None)
        } else {
            Err(crate::models::cron::saved_invalid())
        };
    }
    let invalid = crate::models::cron::saved_invalid;
    let (start, end) = value.split_once(" - ").ok_or_else(invalid)?;
    let start = chrono::DateTime::parse_from_rfc3339(start).map_err(|_| invalid())?;
    let end = chrono::DateTime::parse_from_rfc3339(end).map_err(|_| invalid())?;
    if start >= end
        || start.offset().local_minus_utc() != 25200
        || end.offset().local_minus_utc() != 25200
        || (
            start.hour(),
            start.minute(),
            start.second(),
            start.nanosecond(),
        ) != (0, 0, 0, 0)
        || (end.hour(), end.minute(), end.second(), end.nanosecond()) != (23, 59, 59, 0)
    {
        return Err(invalid());
    }
    Ok(Some(crate::validations::accounting::AccountingDateRange {
        start,
        end: end + chrono::Duration::seconds(1) - chrono::Duration::nanoseconds(1),
    }))
}

fn invalid(message: &'static str) -> AppError {
    AppError::new(StatusCode::BAD_REQUEST, message)
}

pub(crate) fn validate_csv_filename(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && !value.starts_with('.')
        && std::path::Path::new(value)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("csv"))
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
}
pub(crate) fn validate_saved_file(
    file: &crate::models::cron::report::SavedFile,
) -> Result<(), AppError> {
    let invalid = crate::models::cron::saved_invalid;
    if !validate_csv_filename(&file.filename)
        || file.content_type != report_generator::CSV_CONTENT_TYPE
    {
        return Err(invalid());
    }
    let url = reqwest::Url::parse(&file.download_url).map_err(|_| invalid())?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(invalid());
    }
    Ok(())
}

#[cfg(test)]
#[path = "../../tests/validations/report_results.rs"]
mod tests;
