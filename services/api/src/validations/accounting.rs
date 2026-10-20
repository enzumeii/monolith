use crate::error::AppError;
use axum::{
    extract::{Query, rejection::QueryRejection},
    http::StatusCode,
};
use chrono::{DateTime, FixedOffset};
use domain::dto::AccountingReportQuery;

#[derive(Clone, Debug)]
pub struct AccountingDateRange {
    pub start: DateTime<FixedOffset>,
    pub end: DateTime<FixedOffset>,
}

pub fn extract_accounting_query(
    query: Result<Query<AccountingReportQuery>, QueryRejection>,
) -> Result<AccountingDateRange, AppError> {
    let invalid = || {
        AppError::new(
            StatusCode::BAD_REQUEST,
            "Expected valid RFC3339 date_from and date_to values with date_from before date_to",
        )
    };
    let Query(query) = query.map_err(|_| invalid())?;
    let start = DateTime::parse_from_rfc3339(&query.date_from).map_err(|_| invalid())?;
    let end = DateTime::parse_from_rfc3339(&query.date_to).map_err(|_| invalid())?;
    if start >= end {
        return Err(invalid());
    }
    Ok(AccountingDateRange { start, end })
}

#[cfg(test)]
#[path = "../tests/validations/accounting.rs"]
mod tests;
