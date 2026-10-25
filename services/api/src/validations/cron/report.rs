use crate::{error::AppError, models::cron::report::SavedFile};
use axum::http::StatusCode;
use chrono::{DateTime, NaiveDateTime, Utc};
use domain::dto::ListReportResultsQuery;

pub(crate) fn validate_download_path(
    path: Result<axum::extract::Path<(String, String)>, axum::extract::rejection::PathRejection>,
) -> Result<(uuid::Uuid, String), AppError> {
    let axum::extract::Path((id, filename)) =
        path.map_err(|_| invalid("Invalid report file path"))?;
    let id = id
        .parse()
        .map_err(|_| invalid("Invalid report identifier"))?;
    if !SavedFile::valid_filename(&filename) {
        return Err(invalid("Invalid report filename"));
    }
    Ok((id, filename))
}

pub(crate) fn validate_filter(query: &ListReportResultsQuery) -> Result<ValidatedFilter, AppError> {
    let (page, per_page) = crate::validations::common::validate_page(query.page, query.per_page)?;
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
    Ok(ValidatedFilter {
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
pub(crate) struct ValidatedFilter {
    pub report_type: Option<domain::dto::ScheduleReportType>,
    pub generated_from: Option<NaiveDateTime>,
    pub generated_to: Option<NaiveDateTime>,
    pub page: u32,
    pub per_page: u32,
}

fn invalid(message: &'static str) -> AppError {
    AppError::new(StatusCode::BAD_REQUEST, message)
}
