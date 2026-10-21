use crate::{error::AppError, models::report_results::ResultFilter};
use axum::http::StatusCode;
use chrono::{DateTime, NaiveDateTime, Utc};
use domain::dto::ListReportResultsQuery;

pub(crate) fn filter(query: &ListReportResultsQuery) -> Result<ResultFilter, AppError> {
    let (page, per_page) = super::cron::page(query.page, query.per_page)?;
    let generated_from = query
        .generated_from
        .as_deref()
        .map(parse_timestamp)
        .transpose()?;
    let generated_to = query
        .generated_to
        .as_deref()
        .map(parse_timestamp)
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
pub(crate) fn parse_timestamp(value: &str) -> Result<NaiveDateTime, AppError> {
    Ok(DateTime::parse_from_rfc3339(value)
        .map_err(|_| invalid("Timestamps must be RFC3339 with an explicit offset"))?
        .with_timezone(&Utc)
        .naive_utc())
}
fn invalid(message: &'static str) -> AppError {
    AppError::new(StatusCode::BAD_REQUEST, message)
}

pub(crate) fn csv_filename(value: &str) -> bool {
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
pub(crate) fn saved_file(file: &crate::models::report_results::SavedFile) -> Result<(), AppError> {
    let invalid = crate::models::cron::saved_invalid;
    if !csv_filename(&file.filename)
        || file.content_type != crate::services::report_storage::CSV_CONTENT_TYPE
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
#[path = "../tests/validations/report_results.rs"]
mod tests;
