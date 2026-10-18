use crate::error::AppError;
use axum::extract::{Query, rejection::QueryRejection};
use domain::dto::OrderReportQuery;

pub fn extract_order_query(
    query: Result<Query<OrderReportQuery>, QueryRejection>,
) -> Result<String, AppError> {
    let invalid = || {
        AppError::BadRequest("Expected a nonblank tags value without pagination".into())
            .at(module_path!())
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
