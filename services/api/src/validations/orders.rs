use crate::error::AppError;
use axum::extract::{Query, rejection::QueryRejection};
use axum::http::StatusCode;
use domain::dto::OrderReportQuery;

pub fn extract_order_query(
    query: Result<Query<OrderReportQuery>, QueryRejection>,
) -> Result<String, AppError> {
    let invalid = || {
        AppError::new(
            StatusCode::BAD_REQUEST,
            "Expected a nonblank tags value without pagination",
        )
    };
    let Query(query) = query.map_err(|_| invalid())?;
    if query.tags.trim().is_empty() {
        return Err(invalid());
    }
    Ok(query.tags)
}

#[cfg(test)]
#[path = "../tests/validations/orders.rs"]
mod tests;
