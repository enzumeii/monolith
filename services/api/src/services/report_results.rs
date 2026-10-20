use crate::{error::AppError, models::report_results::ResultFilter, repositories, state::AppState};
use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use domain::dto::{ListReportResultsQuery, ReportResultDTO};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(serde::Serialize)]
pub struct ReportPage {
    pub items: Vec<ReportResultDTO>,
    pub page: u32,
    pub per_page: u32,
    pub has_next: bool,
}

pub async fn list(pool: &PgPool, query: ListReportResultsQuery) -> Result<ReportPage, AppError> {
    let filter = make_filter(&query)?;
    let mut records = repositories::report_results::list(pool, filter).await?;
    let has_next = records.len() > filter.per_page as usize;
    records.truncate(filter.per_page as usize);
    Ok(ReportPage {
        items: records
            .into_iter()
            .map(crate::models::report_results::ReportResult::into_dto)
            .collect(),
        page: filter.page,
        per_page: filter.per_page,
        has_next,
    })
}

pub async fn detail(pool: &PgPool, id: Uuid) -> Result<ReportResultDTO, AppError> {
    repositories::report_results::find(pool, id)
        .await?
        .map(crate::models::report_results::ReportResult::into_dto)
        .ok_or_else(missing)
}

pub async fn download(
    state: &AppState,
    id: Uuid,
    filename: Option<&str>,
) -> Result<super::report_storage::GeneratedFile, AppError> {
    let record = repositories::report_results::find(&state.pool, id)
        .await?
        .ok_or_else(missing)?;
    if record.files.0.is_empty() {
        return Err(AppError::new(
            StatusCode::CONFLICT,
            "Report files are not ready",
        ));
    }
    let file = match filename {
        Some(name) => record.files.0.iter().find(|file| file.filename == name),
        None => record.files.0.first(),
    }
    .ok_or_else(|| invalid("File is not part of this report"))?;
    super::report_storage::read_published(&state.settings.reports.storage_directory, id, file).await
}

fn make_filter(query: &ListReportResultsQuery) -> Result<ResultFilter, AppError> {
    let page = query.page.unwrap_or(1);
    let per_page = query.per_page.unwrap_or(25);
    if page == 0 || !(1..=100).contains(&per_page) {
        return Err(invalid(
            "page must be positive and per_page must be between 1 and 100",
        ));
    }
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

fn parse_timestamp(value: &str) -> Result<chrono::NaiveDateTime, AppError> {
    Ok(DateTime::parse_from_rfc3339(value)
        .map_err(|_| invalid("Timestamps must be RFC3339 with an explicit offset"))?
        .with_timezone(&Utc)
        .naive_utc())
}

fn missing() -> AppError {
    AppError::new(StatusCode::NOT_FOUND, "Report result not found")
}
fn invalid(message: &'static str) -> AppError {
    AppError::new(StatusCode::BAD_REQUEST, message)
}

#[cfg(test)]
#[path = "../tests/services/report_results.rs"]
mod tests;
